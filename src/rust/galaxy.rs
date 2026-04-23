//! Galaxy simulation — cell grid, Newtonian gravity, in-place tick.
//!
//! Data is stored as Struct-of-Arrays (parallel `Vec<f32>` / `Vec<u16>`) so
//! the physics inner loop is a tight numeric kernel the optimizer can
//! auto-vectorize. Acceleration is accumulated in cartesian (ax, ay) — the
//! old polar (magnitude, degrees) representation required four trig calls
//! per pair, which dominated the tick cost.
//!
//! Hot path is `tick()`:
//!   1. `gravitate_all()` — O(N²/2) pair sweep, symmetric (Newton's 3rd
//!      law). Skips mass=0 on either side.
//!   2. `apply_acceleration()` — integrate one step, reassign cells to
//!      destination grid indices, accumulate mass on collision. Uses a
//!      `Vec<u32>` (size N²) instead of a `HashMap` to coalesce masses.
//!
//! `tick` returns a new `Galaxy` to preserve the existing JS API, but
//! internally reuses scratch buffers and moves the resulting arrays.

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use wasm_bindgen::prelude::*;

/// Initial-condition presets selectable from the UI. Each produces a
/// visibly different long-term evolution; see `seed_with_mode` for the
/// per-mode mass and velocity distributions.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialCondition {
    /// Current behavior: uniform random mass across the grid, zero initial velocity.
    Uniform = 0,
    /// Disk with angular velocity — cells get tangential velocity around the
    /// grid center scaled by distance, producing a rotating galaxy.
    Rotation = 1,
    /// Big-bang-style central explosion — mass concentrated in the center,
    /// outward radial velocity.
    Bang = 2,
    /// Two distinct mass clusters on intercept trajectory.
    Collision = 3,
}

#[wasm_bindgen]
pub struct Galaxy {
    size: u16,
    n: usize,

    mass: Vec<u16>,
    acc_x: Vec<f32>,
    acc_y: Vec<f32>,

    // Persistent velocity (per mass bucket). Without this the sim
    // restarts from rest every tick and produces imperceptible motion.
    vel_x: Vec<f32>,
    vel_y: Vec<f32>,

    // Sub-grid fractional offsets so a cell can "accumulate" toward its
    // next grid cell across several ticks instead of snapping immediately.
    frac_x: Vec<f32>,
    frac_y: Vec<f32>,

    // Integer (x, y) for each cell. Since grid positions are integers we
    // can take the diff as int and index an inv-r³ lookup with r² — no
    // `sqrt` in the hot loop.
    xs_i: Vec<i16>,
    ys_i: Vec<i16>,

    // Precomputed `g · (r² + soft)^(-3/2)` indexed by integer r². Size-
    // dependent: populated in `new()` and re-used across seeds/ticks.
    inv_r3: Vec<f32>,

    // Scratch buffers reused across ticks.
    scratch_mass: Vec<u32>,
}

impl Galaxy {
    // Newton's G is ~6.67e-11 in SI. At this grid scale (distances of 1-100,
    // masses of 1-65535) that's numerically invisible. Pick a value big
    // enough to produce motion in a reasonable number of ticks; the
    // velocity integrator (`apply_acceleration`) caps step size via
    // `MAX_SUBGRID_STEP` so blowup isn't a concern even if G is punchy.
    pub const GRAVATIONAL_CONSTANT: f32 = 5.0e-2;

    // Softening length to avoid division by ~0 when cells share a grid cell.
    const SOFTENING_SQ: f32 = 1.0;

    // Cap the per-tick position delta so we don't teleport halfway across
    // the grid on a tight mass concentration.
    const MAX_SUBGRID_STEP: f32 = 0.5;
}

#[wasm_bindgen]
impl Galaxy {
    #[wasm_bindgen(constructor)]
    pub fn new(size: u16, cell_initial_mass: u16) -> Galaxy {
        console_error_panic_hook::set_once();
        let n = (size as usize) * (size as usize);
        let size_i = size as i32;

        let mut xs_i = Vec::with_capacity(n);
        let mut ys_i = Vec::with_capacity(n);
        for i in 0..n {
            xs_i.push((i as i32 % size_i) as i16);
            ys_i.push((i as i32 / size_i) as i16);
        }

        // inv_r3[r²] = G · (r² + soft)^(-3/2)
        // max integer r² = (size-1)² + (size-1)² = 2·(size-1)²
        let max_r2 = 2 * ((size as i32 - 1).max(0) as usize).pow(2);
        let mut inv_r3 = Vec::with_capacity(max_r2 + 1);
        for r2_int in 0..=max_r2 {
            let r2 = r2_int as f32 + Galaxy::SOFTENING_SQ;
            let inv_r = 1.0 / r2.sqrt();
            inv_r3.push(Galaxy::GRAVATIONAL_CONSTANT * inv_r * inv_r * inv_r);
        }

        Galaxy {
            size,
            n,
            mass: vec![cell_initial_mass; n],
            acc_x: vec![0.0; n],
            acc_y: vec![0.0; n],
            vel_x: vec![0.0; n],
            vel_y: vec![0.0; n],
            frac_x: vec![0.0; n],
            frac_y: vec![0.0; n],
            xs_i,
            ys_i,
            inv_r3,
            scratch_mass: vec![0; n],
        }
    }

    /// Uniform random mass, zero initial velocity. Preserved for
    /// backwards-compatibility with the JS `Frontend.seed(mass)` call.
    pub fn seed(&self, additional: u16) -> Galaxy {
        self.seed_with_mode(additional, InitialCondition::Uniform)
    }

    /// Seed the galaxy with a named initial condition. Each mode produces
    /// a distinct long-term evolution. Tuning constants are chosen so that
    /// the default UI params (size=50, seed_mass=25) produce visible motion
    /// within a few hundred ticks.
    pub fn seed_with_mode(&self, additional: u16, mode: InitialCondition) -> Galaxy {
        let mut rng = rand::rng();
        let mut mass = self.mass.clone();
        let mut vel_x = vec![0.0f32; self.n];
        let mut vel_y = vec![0.0f32; self.n];

        let size = self.size as f32;
        let cx = size * 0.5;
        let cy = size * 0.5;

        match mode {
            InitialCondition::Uniform => {
                if additional > 0 {
                    for m in mass.iter_mut() {
                        *m = m.saturating_add(rng.random_range(0..=additional));
                    }
                }
            }
            InitialCondition::Rotation => {
                // Fill the grid with a random mass bias (like uniform) so
                // there's something to rotate, then give each cell a
                // tangential velocity around the center. Magnitude grows
                // with distance so the disk rotates roughly rigid-body-like
                // until gravity pulls it into arms.
                let base = additional.max(1);
                // Peak velocity tuned so a 50-grid takes a few hundred ticks
                // to complete one revolution at dt=0.5.
                const V_SCALE: f32 = 0.6;
                let max_r = (size * 0.5).max(1.0);
                for i in 0..self.n {
                    mass[i] = mass[i].saturating_add(rng.random_range(0..=base));
                    let x = self.xs_i[i] as f32 - cx;
                    let y = self.ys_i[i] as f32 - cy;
                    let r = (x * x + y * y).sqrt();
                    if r < 1e-3 {
                        continue;
                    }
                    // Tangential unit vector: (-y, x) / r; scale by r/max_r.
                    let s = V_SCALE * (r / max_r);
                    vel_x[i] = -y / r * s;
                    vel_y[i] = x / r * s;
                }
            }
            InitialCondition::Bang => {
                // Concentrate mass in a small disc at the center and give
                // every massed cell an outward radial velocity. Produces
                // an expanding ring that gravity can later re-collapse.
                // Clear baseline mass; we want a visible central cluster
                // against empty space.
                for m in mass.iter_mut() {
                    *m = 0;
                }
                let core_radius = (size * 0.15).max(2.0);
                let core_r2 = core_radius * core_radius;
                // Average mass per cell in the core, scaled by how many
                // cells we're squeezing into. Use `additional` (which
                // acts as an "intensity" knob via the seed-mass slider).
                let core_fill = additional.max(1000);
                const V_SCALE: f32 = 1.5;
                for i in 0..self.n {
                    let x = self.xs_i[i] as f32 - cx;
                    let y = self.ys_i[i] as f32 - cy;
                    let r2 = x * x + y * y;
                    if r2 > core_r2 {
                        continue;
                    }
                    mass[i] = core_fill.saturating_add(rng.random_range(0..=core_fill / 2));
                    let r = r2.sqrt().max(1e-3);
                    // Radial outward unit vector; slight jitter so the
                    // shell doesn't stay perfectly symmetric.
                    let jitter = rng.random_range(-0.1f32..=0.1f32);
                    vel_x[i] = (x / r) * (V_SCALE + jitter);
                    vel_y[i] = (y / r) * (V_SCALE + jitter);
                }
            }
            InitialCondition::Collision => {
                // Two disc-shaped clusters offset horizontally, each given
                // a velocity toward the other plus a small vertical offset
                // so they graze rather than perfectly head-on.
                for m in mass.iter_mut() {
                    *m = 0;
                }
                let cluster_radius = (size * 0.12).max(2.0);
                let cr2 = cluster_radius * cluster_radius;
                let offset = size * 0.25;
                let left_x = cx - offset;
                let left_y = cy - size * 0.05;
                let right_x = cx + offset;
                let right_y = cy + size * 0.05;
                let cluster_fill = additional.max(800);
                const V_APPROACH: f32 = 0.8;
                for i in 0..self.n {
                    let fx = self.xs_i[i] as f32;
                    let fy = self.ys_i[i] as f32;
                    let dxl = fx - left_x;
                    let dyl = fy - left_y;
                    let dxr = fx - right_x;
                    let dyr = fy - right_y;
                    if dxl * dxl + dyl * dyl <= cr2 {
                        mass[i] =
                            cluster_fill.saturating_add(rng.random_range(0..=cluster_fill / 2));
                        // Move right, slight downward drift.
                        vel_x[i] = V_APPROACH;
                        vel_y[i] = 0.1;
                    } else if dxr * dxr + dyr * dyr <= cr2 {
                        mass[i] =
                            cluster_fill.saturating_add(rng.random_range(0..=cluster_fill / 2));
                        // Move left, slight upward drift.
                        vel_x[i] = -V_APPROACH;
                        vel_y[i] = -0.1;
                    }
                }
            }
        }

        Galaxy {
            size: self.size,
            n: self.n,
            mass,
            acc_x: vec![0.0; self.n],
            acc_y: vec![0.0; self.n],
            vel_x,
            vel_y,
            frac_x: vec![0.0; self.n],
            frac_y: vec![0.0; self.n],
            xs_i: self.xs_i.clone(),
            ys_i: self.ys_i.clone(),
            inv_r3: self.inv_r3.clone(),
            scratch_mass: vec![0; self.n],
        }
    }

    /// Reproducible variant of [`seed`] that draws randomness from a
    /// `u64`-seeded ChaCha-based RNG (`StdRng`). Two galaxies seeded with
    /// the same `(additional, seed)` pair produce byte-identical state —
    /// this is what enables `?seed=…` URL sharing on the JS side.
    pub fn seed_with(&self, additional: u16, seed: u64) -> Galaxy {
        let mut rng = StdRng::seed_from_u64(seed);
        self.seed_with_rng(additional, &mut rng)
    }

    pub fn tick(&self, time: f32) -> Galaxy {
        let mut next = Galaxy {
            size: self.size,
            n: self.n,
            mass: self.mass.clone(),
            acc_x: self.acc_x.clone(),
            acc_y: self.acc_y.clone(),
            vel_x: self.vel_x.clone(),
            vel_y: self.vel_y.clone(),
            frac_x: self.frac_x.clone(),
            frac_y: self.frac_y.clone(),
            xs_i: self.xs_i.clone(),
            ys_i: self.ys_i.clone(),
            inv_r3: self.inv_r3.clone(),
            scratch_mass: vec![0; self.n],
        };
        next.gravitate_all();
        next.apply_acceleration(time);
        next
    }

    /// Flat-buffer exposure for zero-copy JS reads via wasm.memory.
    pub fn mass_ptr(&self) -> *const u16 {
        self.mass.as_ptr()
    }
    pub fn mass_len(&self) -> usize {
        self.n
    }

    // Positions are pure functions of index + size; JS can derive these
    // without a copy. Kept as convenience for tests and older callers.
    pub fn mass(&self) -> Vec<u16> {
        self.mass.clone()
    }
    pub fn x(&self) -> Vec<u16> {
        (0..self.n as u16)
            .map(|i| self.index_to_col_row(i).0)
            .collect()
    }
    pub fn y(&self) -> Vec<u16> {
        (0..self.n as u16)
            .map(|i| self.index_to_col_row(i).1)
            .collect()
    }
}

impl Galaxy {
    /// Shared seeding kernel used by both the non-deterministic `seed()`
    /// and the reproducible `seed_with(additional, seed)`. Splitting on
    /// RNG lets the two expose different public APIs (wasm-bindgen doesn't
    /// take generics) while keeping the mass-fill loop identical.
    fn seed_with_rng<R: Rng + ?Sized>(&self, additional: u16, rng: &mut R) -> Galaxy {
        let mut mass = self.mass.clone();
        if additional > 0 {
            for m in mass.iter_mut() {
                *m = m.saturating_add(rng.random_range(0..=additional));
            }
        }
        Galaxy {
            size: self.size,
            n: self.n,
            mass,
            acc_x: vec![0.0; self.n],
            acc_y: vec![0.0; self.n],
            vel_x: vec![0.0; self.n],
            vel_y: vec![0.0; self.n],
            frac_x: vec![0.0; self.n],
            frac_y: vec![0.0; self.n],
            xs_i: self.xs_i.clone(),
            ys_i: self.ys_i.clone(),
            inv_r3: self.inv_r3.clone(),
            scratch_mass: vec![0; self.n],
        }
    }

    // (col, row) — x is column, y is row. Matches the pre-rewrite convention.
    #[inline]
    fn index_to_col_row(&self, index: u16) -> (u16, u16) {
        (index % self.size, index / self.size)
    }

    #[inline]
    fn col_row_to_index(&self, col: u16, row: u16) -> u16 {
        row * self.size + col
    }

    /// Dispatches between two force-calc strategies:
    ///   * **Direct O(N²)** for small active sets, where building a tree
    ///     costs more than it saves. Uses an integer-r² lookup so the
    ///     inner loop has no sqrt. O(A²) where A is active cell count.
    ///   * **Barnes-Hut quadtree O(N log N)** for large active sets.
    ///     Each body traverses the tree once; distant clumps of mass
    ///     are summarized by their center-of-mass when `s/d < θ`.
    ///
    /// Both paths read/write the same acc_x / acc_y buffers the
    /// integrator consumes.
    fn gravitate_all(&mut self) {
        let n = self.n;

        // Build active list once: cells with nonzero mass. Pre-collapse a
        // 250×250 sim has ~60k active cells; post-collapse often <200.
        // Either way, iterating the active list instead of the full N²
        // skips all the empty space.
        let mut active: Vec<usize> = Vec::with_capacity(n);
        for i in 0..n {
            if self.mass[i] != 0 {
                active.push(i);
            }
        }

        // Clear accelerations for inactive cells up front.
        for i in 0..n {
            self.acc_x[i] = 0.0;
            self.acc_y[i] = 0.0;
        }

        // Heuristic: O(A²) is cheaper than Barnes-Hut until A is big
        // enough that log-tree traversal pays for building the tree.
        // ~1000 is roughly where the crossover happens in WASM (all-pairs
        // inner loop is ~2ns/pair, BH node visit is ~20ns).
        const BH_THRESHOLD: usize = 1000;

        if active.len() < BH_THRESHOLD {
            self.gravitate_direct(&active);
        } else {
            self.gravitate_barnes_hut(&active);
        }
    }

    /// O(A²) direct-sum over the active list. With the integer-r² lookup
    /// table the inner loop is six adds / six muls / zero transcendentals.
    fn gravitate_direct(&mut self, active: &[usize]) {
        let xs_i = self.xs_i.as_slice();
        let ys_i = self.ys_i.as_slice();
        let inv_r3_tbl = self.inv_r3.as_slice();

        // Prebuild f32 masses for the active set so the inner loop stays
        // cast-free.
        let mut mass_f: Vec<f32> = Vec::with_capacity(active.len());
        for &j in active {
            mass_f.push(self.mass[j] as f32);
        }

        for (ai, &i) in active.iter().enumerate() {
            let ix = xs_i[i] as i32;
            let iy = ys_i[i] as i32;
            let mut ax = 0.0f32;
            let mut ay = 0.0f32;

            for (aj, &j) in active.iter().enumerate() {
                if ai == aj {
                    continue;
                }
                let dx_i = xs_i[j] as i32 - ix;
                let dy_i = ys_i[j] as i32 - iy;
                let r2_idx = (dx_i * dx_i + dy_i * dy_i) as usize;
                let k = inv_r3_tbl[r2_idx] * mass_f[aj];
                ax += k * dx_i as f32;
                ay += k * dy_i as f32;
            }

            self.acc_x[i] = ax;
            self.acc_y[i] = ay;
        }
    }

    /// Barnes-Hut via flat-arena quadtree. θ = 0.7 gives good accuracy
    /// for galaxy-scale gravity; smaller θ = more accurate but slower.
    fn gravitate_barnes_hut(&mut self, active: &[usize]) {
        const THETA: f32 = 0.7;
        const THETA_SQ: f32 = THETA * THETA;
        let soft = Galaxy::SOFTENING_SQ;
        let g = Galaxy::GRAVATIONAL_CONSTANT;

        // Collect f32 positions and masses for the active set.
        let mut px: Vec<f32> = Vec::with_capacity(active.len());
        let mut py: Vec<f32> = Vec::with_capacity(active.len());
        let mut pm: Vec<f32> = Vec::with_capacity(active.len());
        for &idx in active {
            px.push(self.xs_i[idx] as f32);
            py.push(self.ys_i[idx] as f32);
            pm.push(self.mass[idx] as f32);
        }

        // Root bounds cover the full grid.
        let size_f = self.size as f32;
        let tree = build_quadtree(&px, &py, &pm, 0.0, 0.0, size_f);

        for (ai, &i) in active.iter().enumerate() {
            let (ax, ay) = tree.force(px[ai], py[ai], THETA_SQ, soft, g);
            self.acc_x[i] = ax;
            self.acc_y[i] = ay;
        }
    }

    /// Semi-implicit Euler integration. For each mass-carrying grid cell:
    ///   v += a · dt          (velocity carries across ticks — this is
    ///                         what makes the galaxy actually *move* over
    ///                         time instead of twitching once and freezing)
    ///   Δ = clamp(v · dt, ±MAX_SUBGRID_STEP)
    ///   frac += Δ            (accumulate sub-grid motion)
    ///   when |frac| ≥ 1 we transfer to the neighboring grid cell, keep
    ///   remainder in frac, and bring velocity with us.
    ///
    /// Mass merging: when two cells land on the same grid index we sum
    /// their masses and take the momentum-weighted average velocity of
    /// their components (p = Σmᵢvᵢ ⇒ v = p / Σmᵢ) so collisions conserve
    /// momentum.
    fn apply_acceleration(&mut self, time: f32) {
        let size = self.size as i32;
        let max_step = Galaxy::MAX_SUBGRID_STEP;

        // Zero the mass scratch; we'll also accumulate momentum here into
        // parallel scratch vectors kept locally (small & stack-allocated
        // per-tick is fine).
        for m in self.scratch_mass.iter_mut() {
            *m = 0;
        }
        let mut p_x = vec![0.0f32; self.n];
        let mut p_y = vec![0.0f32; self.n];
        let mut frac_next_x = vec![0.0f32; self.n];
        let mut frac_next_y = vec![0.0f32; self.n];

        for i in 0..self.n {
            let m = self.mass[i];
            if m == 0 {
                // Clear velocity for empty cells so stale values don't
                // propagate when this slot gets re-occupied later.
                self.vel_x[i] = 0.0;
                self.vel_y[i] = 0.0;
                self.frac_x[i] = 0.0;
                self.frac_y[i] = 0.0;
                continue;
            }

            // v += a · dt
            let mut vx = self.vel_x[i] + self.acc_x[i] * time;
            let mut vy = self.vel_y[i] + self.acc_y[i] * time;

            // Damping so energy doesn't run away (no dissipation in an
            // ideal N-body; but a grid-quantized sim integrates poorly
            // at large dt and the system overheats without this).
            vx *= 0.995;
            vy *= 0.995;

            // Sub-grid position update
            let mut fx = self.frac_x[i] + (vx * time).clamp(-max_step, max_step);
            let mut fy = self.frac_y[i] + (vy * time).clamp(-max_step, max_step);

            let (col, row) = (i as i32 % size, i as i32 / size);

            // Transfer to neighboring cell(s) as fractional offset crosses
            // ±0.5 (half-cell).
            let mut new_col = col;
            let mut new_row = row;
            if fx >= 0.5 {
                new_col += 1;
                fx -= 1.0;
            } else if fx <= -0.5 {
                new_col -= 1;
                fx += 1.0;
            }
            if fy >= 0.5 {
                new_row += 1;
                fy -= 1.0;
            } else if fy <= -0.5 {
                new_row -= 1;
                fy += 1.0;
            }

            let new_col = wrap(new_col, size) as u16;
            let new_row = wrap(new_row, size) as u16;
            let ni = self.col_row_to_index(new_col, new_row) as usize;

            // Merge: sum mass, accumulate momentum, keep the fraction of
            // the *arriving* cell (approx — good enough for visuals).
            let sum = self.scratch_mass[ni].saturating_add(m as u32);
            self.scratch_mass[ni] = sum;
            p_x[ni] += vx * m as f32;
            p_y[ni] += vy * m as f32;
            frac_next_x[ni] = fx;
            frac_next_y[ni] = fy;
        }

        for i in 0..self.n {
            let m32 = self.scratch_mass[i].min(u16::MAX as u32);
            self.mass[i] = m32 as u16;
            if m32 > 0 {
                let mf = m32 as f32;
                self.vel_x[i] = p_x[i] / mf;
                self.vel_y[i] = p_y[i] / mf;
                self.frac_x[i] = frac_next_x[i];
                self.frac_y[i] = frac_next_y[i];
            } else {
                self.vel_x[i] = 0.0;
                self.vel_y[i] = 0.0;
                self.frac_x[i] = 0.0;
                self.frac_y[i] = 0.0;
            }
            self.acc_x[i] = 0.0;
            self.acc_y[i] = 0.0;
        }
    }

}

/// Clamp with wrap-around so a cell that accelerates past the edge
/// reappears on the other side (matches the pre-rewrite behaviour that
/// the old `clamp()` actually produced via `value.abs() % max`).
#[inline]
fn wrap(value: i32, size: i32) -> i32 {
    let m = value % size;
    if m < 0 {
        m + size
    } else {
        m
    }
}

// ---------------------------------------------------------------------------
// Barnes-Hut quadtree (flat-arena)
// ---------------------------------------------------------------------------

const NO_CHILD: u32 = u32::MAX;

#[derive(Clone)]
struct Node {
    // Node's bounding box — centered quadrants split at (cx, cy) with
    // half-side `h`. Root covers (0,0)-(size,size) so cx=h, cy=h.
    cx: f32,
    cy: f32,
    h: f32,

    // Aggregate mass and center-of-mass. For internal nodes these are
    // running sums of descendants; for leaves they represent the one
    // body the leaf contains.
    mass: f32,
    com_x: f32,
    com_y: f32,

    // Leaf state: index of contained body, or NO_CHILD if empty. An
    // internal node has `body == NO_CHILD` and non-NO_CHILD child
    // indices.
    body: u32,

    // Children (NE=0, NW=1, SW=2, SE=3). NO_CHILD means "empty quadrant".
    children: [u32; 4],
}

impl Node {
    fn empty(cx: f32, cy: f32, h: f32) -> Self {
        Node {
            cx,
            cy,
            h,
            mass: 0.0,
            com_x: 0.0,
            com_y: 0.0,
            body: NO_CHILD,
            children: [NO_CHILD; 4],
        }
    }

    fn is_leaf(&self) -> bool {
        self.children.iter().all(|&c| c == NO_CHILD)
    }
}

struct Tree {
    nodes: Vec<Node>,
}

/// Build the Barnes-Hut quadtree. The root covers (0,0)..(size, size).
fn build_quadtree(px: &[f32], py: &[f32], pm: &[f32], ox: f32, oy: f32, size: f32) -> Tree {
    let h = size * 0.5;
    let mut nodes: Vec<Node> = Vec::with_capacity(px.len() * 2);
    // Root at index 0.
    nodes.push(Node::empty(ox + h, oy + h, h));

    for i in 0..px.len() {
        if pm[i] == 0.0 {
            continue;
        }
        insert(&mut nodes, 0, i as u32, px[i], py[i], pm[i]);
    }
    Tree { nodes }
}

/// Insert body `b` into the subtree rooted at `node_idx`. Grows the arena
/// via `nodes.push(...)` — uses indices to avoid borrow-checker fights on
/// recursive `&mut Vec<Node>`.
fn insert(nodes: &mut Vec<Node>, node_idx: usize, b: u32, bx: f32, by: f32, bm: f32) {
    let (h, existing_body, is_leaf) = {
        let node = &nodes[node_idx];
        (node.h, node.body, node.is_leaf())
    };

    if is_leaf && existing_body == NO_CHILD {
        // Empty leaf — just drop the body in.
        let n = &mut nodes[node_idx];
        n.body = b;
        n.mass = bm;
        n.com_x = bx;
        n.com_y = by;
        return;
    }

    if is_leaf {
        // Leaf with one body — subdivide and reinsert both into the
        // appropriate quadrants.
        let old_body = existing_body;
        let old_x = nodes[node_idx].com_x;
        let old_y = nodes[node_idx].com_y;
        let old_m = nodes[node_idx].mass;

        // Convert this node into an internal. Update CoM once at the end
        // via the mass-weighted running sum.
        {
            let n = &mut nodes[node_idx];
            n.body = NO_CHILD;
            n.mass = 0.0;
            n.com_x = 0.0;
            n.com_y = 0.0;
        }

        // If both bodies hash to the same quadrant at a very deep level
        // (e.g. two cells on the exact same grid point), just merge into
        // a single leaf — further subdivision won't separate them.
        if h < 1e-6 {
            let n = &mut nodes[node_idx];
            n.mass = old_m + bm;
            n.com_x = (old_x * old_m + bx * bm) / n.mass;
            n.com_y = (old_y * old_m + by * bm) / n.mass;
            return;
        }

        subdivide_and_insert(nodes, node_idx, old_body, old_x, old_y, old_m);
        subdivide_and_insert(nodes, node_idx, b, bx, by, bm);
    } else {
        // Internal — keep drilling.
        subdivide_and_insert(nodes, node_idx, b, bx, by, bm);
    }

    // Update running mass + center-of-mass after the recursive insert.
    let n = &mut nodes[node_idx];
    let new_mass = n.mass + bm;
    if new_mass > 0.0 {
        n.com_x = (n.com_x * n.mass + bx * bm) / new_mass;
        n.com_y = (n.com_y * n.mass + by * bm) / new_mass;
    }
    n.mass = new_mass;
}

fn subdivide_and_insert(
    nodes: &mut Vec<Node>,
    parent_idx: usize,
    b: u32,
    bx: f32,
    by: f32,
    bm: f32,
) {
    let (pcx, pcy, ph) = {
        let p = &nodes[parent_idx];
        (p.cx, p.cy, p.h)
    };
    let child_h = ph * 0.5;

    // Quadrant index: 0=NE, 1=NW, 2=SW, 3=SE
    let qi = if bx >= pcx {
        if by >= pcy {
            0
        } else {
            3
        }
    } else if by >= pcy {
        1
    } else {
        2
    };

    let (child_cx, child_cy) = match qi {
        0 => (pcx + child_h, pcy + child_h),
        1 => (pcx - child_h, pcy + child_h),
        2 => (pcx - child_h, pcy - child_h),
        _ => (pcx + child_h, pcy - child_h),
    };

    let child_idx = nodes[parent_idx].children[qi];
    if child_idx == NO_CHILD {
        // Allocate a fresh empty child.
        let new_idx = nodes.len() as u32;
        nodes.push(Node::empty(child_cx, child_cy, child_h));
        nodes[parent_idx].children[qi] = new_idx;
        insert(nodes, new_idx as usize, b, bx, by, bm);
    } else {
        insert(nodes, child_idx as usize, b, bx, by, bm);
    }
}

impl Tree {
    /// Compute force on a body at (bx, by) from every mass in the tree
    /// using the θ criterion: if `s/d < θ` for a node (s = node size,
    /// d = distance to node CoM), treat the whole subtree as one point
    /// mass at its center of mass.
    fn force(&self, bx: f32, by: f32, theta_sq: f32, soft: f32, g: f32) -> (f32, f32) {
        let mut ax = 0.0f32;
        let mut ay = 0.0f32;
        // Iterative DFS via an explicit stack to avoid recursion depth
        // on large/deep trees. Flat arena lets us walk by index.
        let mut stack: Vec<u32> = Vec::with_capacity(64);
        stack.push(0);

        while let Some(idx) = stack.pop() {
            let n = &self.nodes[idx as usize];
            if n.mass == 0.0 {
                continue;
            }
            let dx = n.com_x - bx;
            let dy = n.com_y - by;
            let d2 = dx * dx + dy * dy;

            // Same-body check: leaf at our exact position.
            if d2 < 1e-6 {
                continue;
            }

            let s = n.h * 2.0; // node side length
            let s2 = s * s;

            if n.is_leaf() || s2 < theta_sq * d2 {
                // Accept this node as a point mass.
                let r2 = d2 + soft;
                let inv_r = 1.0 / r2.sqrt();
                let inv_r3 = inv_r * inv_r * inv_r;
                let k = g * inv_r3 * n.mass;
                ax += k * dx;
                ay += k * dy;
            } else {
                for &c in &n.children {
                    if c != NO_CHILD {
                        stack.push(c);
                    }
                }
            }
        }

        (ax, ay)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests_intial_generation {
    use super::*;
    #[test]
    fn test_inital_generation_no_panic() {
        Galaxy::new(10, 0);
    }
    #[test]
    fn test_seed_no_panic() {
        Galaxy::new(10, 0).seed(1);
    }
    #[test]
    fn test_seed_tick_no_panic() {
        Galaxy::new(10, 1).seed(1).tick(1.0);
    }
    #[test]
    fn test_seed_alters_data() {
        let g = Galaxy::new(10, 0);
        let before = g.mass.clone();
        let g = g.seed(1);
        assert_ne!(before, g.mass);
    }
    #[test]
    fn test_seed_doesnt_alter_when_zero() {
        let g = Galaxy::new(10, 0);
        let before = g.mass.clone();
        let g = g.seed(0);
        assert_eq!(before, g.mass);
    }
    #[test]
    fn test_seed_with_same_u64_is_reproducible() {
        // Two galaxies seeded with the same (additional, seed) pair must
        // produce identical cell state. This is the invariant the
        // `?seed=…` URL feature relies on.
        let a = Galaxy::new(10, 0).seed_with(100, 42);
        let b = Galaxy::new(10, 0).seed_with(100, 42);
        assert_eq!(a.mass, b.mass);
    }

    #[test]
    fn test_seed_with_different_u64_differs() {
        let a = Galaxy::new(10, 0).seed_with(100, 42);
        let b = Galaxy::new(10, 0).seed_with(100, 43);
        assert_ne!(a.mass, b.mass);
    }

    #[test]
    fn test_seed_with_zero_additional_matches_base() {
        let base = Galaxy::new(10, 0);
        let seeded = base.seed_with(0, 42);
        assert_eq!(base.mass, seeded.mass);
    }

    #[test]
    fn test_seed_with_mode_uniform_matches_default_seed() {
        // Uniform mode should match the plain `seed()` behaviour (random mass
        // fill, zero velocity).
        let g = Galaxy::new(10, 0).seed_with_mode(0, InitialCondition::Uniform);
        assert!(g.vel_x.iter().all(|&v| v == 0.0));
        assert!(g.vel_y.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_seed_rotation_produces_tangential_velocity() {
        let g = Galaxy::new(20, 0).seed_with_mode(5, InitialCondition::Rotation);
        // At least some cells should have nonzero velocity.
        let nonzero_v = g
            .vel_x
            .iter()
            .zip(g.vel_y.iter())
            .filter(|(vx, vy)| vx.abs() > 1e-6 || vy.abs() > 1e-6)
            .count();
        assert!(
            nonzero_v > 0,
            "rotation mode must set nonzero velocities on some cells"
        );
        // Tangential means r · v ≈ 0 (velocity perpendicular to radius).
        // Pick a cell off-center and verify.
        let size = g.size as f32;
        let cx = size * 0.5;
        let cy = size * 0.5;
        let mut tangential_checked = false;
        for i in 0..g.n {
            let x = g.xs_i[i] as f32 - cx;
            let y = g.ys_i[i] as f32 - cy;
            let r = (x * x + y * y).sqrt();
            if r < 2.0 {
                continue;
            }
            let vx = g.vel_x[i];
            let vy = g.vel_y[i];
            let vmag = (vx * vx + vy * vy).sqrt();
            if vmag < 1e-4 {
                continue;
            }
            // Normalized dot between radius and velocity should be ~0.
            let dot = (x * vx + y * vy) / (r * vmag);
            assert!(
                dot.abs() < 1e-3,
                "rotation velocity should be tangential (cell {} dot={})",
                i,
                dot
            );
            tangential_checked = true;
            break;
        }
        assert!(tangential_checked, "did not find a cell to check tangency");
    }

    #[test]
    fn test_seed_bang_produces_outward_radial_velocity() {
        let g = Galaxy::new(30, 0).seed_with_mode(1000, InitialCondition::Bang);
        let size = g.size as f32;
        let cx = size * 0.5;
        let cy = size * 0.5;

        // Total mass should be concentrated in the central disc.
        let total_mass: u64 = g.mass.iter().map(|&m| m as u64).sum();
        assert!(total_mass > 0, "bang must seed some mass");

        // Every cell with mass should have positive dot(radius, velocity).
        let mut checked = 0;
        for i in 0..g.n {
            if g.mass[i] == 0 {
                continue;
            }
            let x = g.xs_i[i] as f32 - cx;
            let y = g.ys_i[i] as f32 - cy;
            let r = (x * x + y * y).sqrt();
            if r < 1.0 {
                continue;
            }
            let dot = x * g.vel_x[i] + y * g.vel_y[i];
            assert!(
                dot > 0.0,
                "bang cell {} should move outward (dot={})",
                i,
                dot
            );
            checked += 1;
        }
        assert!(checked > 0, "expected at least one off-center bang cell");
    }

    #[test]
    fn test_seed_rotation_has_positive_total_angular_momentum() {
        // Net L_z = Σ m_i (x_i v_{y,i} - y_i v_{x,i}) around the grid center
        // must be strongly positive — that's the whole point of the mode.
        let g = Galaxy::new(30, 0).seed_with_mode(10, InitialCondition::Rotation);
        let size = g.size as f32;
        let cx = size * 0.5;
        let cy = size * 0.5;
        let mut lz: f64 = 0.0;
        for i in 0..g.n {
            let m = g.mass[i] as f64;
            if m == 0.0 {
                continue;
            }
            let x = (g.xs_i[i] as f32 - cx) as f64;
            let y = (g.ys_i[i] as f32 - cy) as f64;
            let vx = g.vel_x[i] as f64;
            let vy = g.vel_y[i] as f64;
            lz += m * (x * vy - y * vx);
        }
        assert!(
            lz > 1.0,
            "rotation mode must have strongly positive total angular momentum, got {}",
            lz
        );
    }

    #[test]
    fn test_seed_collision_produces_two_distinct_mass_clusters() {
        let g = Galaxy::new(40, 0).seed_with_mode(800, InitialCondition::Collision);
        let size = g.size as f32;
        let cx = size * 0.5;
        // Sum mass + mass-weighted centroid in each half; both should be
        // populated and the centroids should be on opposite sides of the
        // grid center, separated by roughly the seed offset (size * 0.5).
        let mut left_mass: u64 = 0;
        let mut right_mass: u64 = 0;
        let mut left_cx: f64 = 0.0;
        let mut right_cx: f64 = 0.0;
        for i in 0..g.n {
            if g.mass[i] == 0 {
                continue;
            }
            let x = g.xs_i[i] as f32;
            let m = g.mass[i] as u64;
            if x < cx {
                left_mass += m;
                left_cx += (x as f64) * (m as f64);
            } else {
                right_mass += m;
                right_cx += (x as f64) * (m as f64);
            }
        }
        assert!(left_mass > 0, "collision: left cluster has no mass");
        assert!(right_mass > 0, "collision: right cluster has no mass");
        let lcx = left_cx / left_mass as f64;
        let rcx = right_cx / right_mass as f64;
        assert!(
            lcx < cx as f64,
            "left centroid should be left of grid center"
        );
        assert!(
            rcx > cx as f64,
            "right centroid should be right of grid center"
        );
        // Clusters seeded at cx ± size*0.25 — centroids should be separated
        // by at least size * 0.3 (slack for the jittered-radius seed).
        assert!(
            (rcx - lcx) > (size as f64) * 0.3,
            "collision centroids too close: left={} right={}",
            lcx,
            rcx
        );

        // Velocities in the left cluster should point right (vx > 0) and
        // vice versa — i.e. the clusters are on intercept.
        let mut left_right_moving = 0;
        let mut right_left_moving = 0;
        for i in 0..g.n {
            if g.mass[i] == 0 {
                continue;
            }
            let x = g.xs_i[i] as f32;
            if x < cx && g.vel_x[i] > 0.0 {
                left_right_moving += 1;
            }
            if x >= cx && g.vel_x[i] < 0.0 {
                right_left_moving += 1;
            }
        }
        assert!(left_right_moving > 0);
        assert!(right_left_moving > 0);
    }

    #[test]
    fn test_seed_alters_data_twice() {
        let g = Galaxy::new(10, 0);
        let first = g.mass.clone();
        let g = g.seed(1);
        let second = g.mass.clone();
        assert_ne!(first, second);
        let g = g.seed(1);
        let third = g.mass.clone();
        assert_ne!(first, third);
        assert_ne!(second, third);
    }
}

#[cfg(test)]
mod tests_indexing {
    use super::*;
    #[test]
    fn test_index_to_col_row_start() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.index_to_col_row(0), (0, 0));
    }
    #[test]
    fn test_col_row_to_index_start() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.col_row_to_index(0, 0), 0);
    }
    #[test]
    fn test_index_to_col_row_center() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.index_to_col_row(4), (1, 1));
    }
    #[test]
    fn test_col_row_to_index_center() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.col_row_to_index(1, 1), 4);
    }
    #[test]
    fn test_index_to_col_row_end() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.index_to_col_row(8), (2, 2));
    }
    #[test]
    fn test_col_row_to_index_end() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.col_row_to_index(2, 2), 8);
    }
    #[test]
    fn test_index_edge_transform_top_right() {
        let g = Galaxy::new(3, 0);
        let index = 2;
        let (x, y) = g.index_to_col_row(index);
        assert_eq!(g.col_row_to_index(x, y), index);
        assert_eq!((x, y), (2, 0));
    }
    #[test]
    fn test_index_edge_transform_bottom_left() {
        let g = Galaxy::new(3, 0);
        let index = 6;
        let (x, y) = g.index_to_col_row(index);
        assert_eq!(g.col_row_to_index(x, y), index);
        assert_eq!((x, y), (0, 2));
    }
}

#[cfg(test)]
mod tests_position_accessors {
    use super::*;
    #[test]
    fn test_mass() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.mass(), vec![0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
    #[test]
    fn test_x() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.x(), vec![0, 1, 2, 0, 1, 2, 0, 1, 2]);
    }
    #[test]
    fn test_y() {
        let g = Galaxy::new(3, 0);
        assert_eq!(g.y(), vec![0, 0, 0, 1, 1, 1, 2, 2, 2]);
    }
}

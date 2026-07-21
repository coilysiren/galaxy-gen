//! Galaxy simulation. See docs/galaxy-rust.md.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use wasm_bindgen::prelude::*;

/// Initial-condition presets. See `seed_with_mode`. Every mode seeds a
/// circular disk with orbital rotation baked in.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialCondition {
    /// Uniform random mass across the disk, circular-orbit velocity.
    Uniform = 0,
    /// Central explosion: mass concentrated, outward radial velocity plus
    /// the shared disk rotation.
    Bang = 1,
}

#[wasm_bindgen]
pub struct Galaxy {
    size: u16,
    n: usize,

    mass: Vec<u16>,
    acc_x: Vec<f32>,
    acc_y: Vec<f32>,

    // See docs/galaxy-rust.md for buffer layout rationale.
    vel_x: Vec<f32>,
    vel_y: Vec<f32>,
    frac_x: Vec<f32>,
    frac_y: Vec<f32>,
    xs_i: Vec<i16>,
    ys_i: Vec<i16>,
    inv_r3: Vec<f32>,
    scratch_mass: Vec<u32>,
}

impl Galaxy {
    // See docs/galaxy-rust.md for constant rationale.
    pub const GRAVATIONAL_CONSTANT: f32 = 5.0e-4;
    const SOFTENING_SQ: f32 = 1.0;
    const MAX_SUBGRID_STEP: f32 = 0.5;
    const DRAG_COEFF: f32 = 0.001;
    /// Integer r² at or below which gravity flips repulsive - a crude
    /// contact-pressure proxy. Without it every same-cell contact is a
    /// perfectly inelastic merge and any bound system ratchets down to a
    /// single max-mass cell. Placeholder until a real equation of state.
    const REPULSE_R2: f32 = 2.0;
    /// Max mass a transfer may pack into one cell (incompressibility
    /// floor). A full destination bounces the mover instead of fusing, so
    /// a bound core saturates into a cluster of full cells rather than a
    /// point. ~10x the default uniform cell fill. Placeholder EOS, same
    /// bucket as REPULSE_R2.
    const CELL_MASS_CAP: u32 = 128;
    /// Velocity retained when a mover is rejected by a full cell. No sign
    /// flip: in a co-rotating region a blocked cell is in traffic, not a
    /// head-on hit, and reflecting it thermalizes the disk's rotation
    /// within a few hundred ticks. 1.0 - a jammed core is blocked nearly
    /// every tick, so any per-block bleed spins it down fast. DRAG_COEFF
    /// is the energy sink instead.
    const BLOCKED_FRICTION: f32 = 1.0;
    /// Spring stiffness of the circular world boundary. Beyond the disk
    /// radius (size/2 - 1) cells feel a gentle inward pull proportional
    /// to overshoot, replacing hard square-wall behavior. The toroidal
    /// wrap stays as a backstop but is effectively unreachable.
    const CONFINE_STIFFNESS: f32 = 0.02;
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
            let mut k = Galaxy::GRAVATIONAL_CONSTANT * inv_r * inv_r * inv_r;
            // Contact repulsion: see REPULSE_R2.
            if (r2_int as f32) <= Galaxy::REPULSE_R2 {
                k = -k;
            }
            inv_r3.push(k);
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

    /// Uniform-mode seed. Preserved for backwards-compatibility with the
    /// JS `Frontend.seed(mass)` call.
    pub fn seed(&self, additional: u16) -> Galaxy {
        self.seed_with_mode(additional, InitialCondition::Uniform)
    }

    /// Seed with a named initial condition. Tuning constants assume
    /// default UI params (size=250, seed_mass=25).
    pub fn seed_with_mode(&self, additional: u16, mode: InitialCondition) -> Galaxy {
        let mut rng = rand::rng();
        self.seed_mode_kernel(additional, mode, &mut rng)
    }

    /// Reproducible [`seed_with_mode`]: same `(additional, mode, seed)`
    /// gives byte-identical state, enabling `?seed=...` URL sharing for
    /// every initial condition, not just Uniform.
    pub fn seed_with_mode_seeded(
        &self,
        additional: u16,
        mode: InitialCondition,
        seed: u64,
    ) -> Galaxy {
        let mut rng = StdRng::seed_from_u64(seed);
        self.seed_mode_kernel(additional, mode, &mut rng)
    }

    // Private, so wasm-bindgen skips it; `dyn` because bindgen impls
    // cannot hold generics.
    fn seed_mode_kernel(
        &self,
        additional: u16,
        mode: InitialCondition,
        rng: &mut dyn rand::Rng,
    ) -> Galaxy {
        let mut mass = self.mass.clone();
        let mut vel_x = vec![0.0f32; self.n];
        let mut vel_y = vec![0.0f32; self.n];

        let size = self.size as f32;
        let cx = size * 0.5;
        let cy = size * 0.5;

        // The world is a disk: mass seeds only within this radius, and the
        // boundary spring (CONFINE_STIFFNESS) takes over past it.
        let disk_r = self.disk_radius();
        let disk_r2 = disk_r * disk_r;

        match mode {
            InitialCondition::Uniform => {
                if additional > 0 {
                    for i in 0..self.n {
                        let x = self.xs_i[i] as f32 - cx;
                        let y = self.ys_i[i] as f32 - cy;
                        if x * x + y * y > disk_r2 {
                            continue;
                        }
                        mass[i] = mass[i].saturating_add(rng.random_range(0..=additional));
                    }
                }
            }
            InitialCondition::Bang => {
                for m in mass.iter_mut() {
                    *m = 0;
                }
                let core_radius = (size * 0.15).max(2.0);
                let core_r2 = core_radius * core_radius;
                // `additional` is the intensity knob (`?mass=` URL param).
                let core_fill = additional.saturating_mul(6).max(150);
                for i in 0..self.n {
                    let x = self.xs_i[i] as f32 - cx;
                    let y = self.ys_i[i] as f32 - cy;
                    if x * x + y * y > core_r2 {
                        continue;
                    }
                    mass[i] = core_fill.saturating_add(rng.random_range(0..=core_fill / 2));
                }
                // Ejection speed keyed to the seeded core's own escape
                // velocity - a fixed speed stops scaling once core mass
                // grows with size² and the "explosion" jams into a ball.
                let m_core: f64 = mass.iter().map(|&m| m as f64).sum();
                let v_esc = (2.0 * Galaxy::GRAVATIONAL_CONSTANT * m_core as f32 / core_radius)
                    .sqrt();
                let v_eject = 1.15 * v_esc;
                for i in 0..self.n {
                    if mass[i] == 0 {
                        continue;
                    }
                    let x = self.xs_i[i] as f32 - cx;
                    let y = self.ys_i[i] as f32 - cy;
                    let r = (x * x + y * y).sqrt().max(1e-3);
                    // Radial outward unit vector; slight jitter so the
                    // shell doesn't stay perfectly symmetric.
                    let jitter = rng.random_range(-0.1f32..=0.1f32);
                    vel_x[i] = (x / r) * (v_eject * (1.0 + jitter));
                    vel_y[i] = (y / r) * (v_eject * (1.0 + jitter));
                }
            }
        }

        // Every mode gets orbital support on top of its mode-specific
        // velocities: v += sqrt(G·M_enc/r) tangentially, with M_enc
        // prefix-summed over cells sorted by radius. A hand-tuned linear
        // ramp under-spins the disk and it free-falls to the center
        // within a few hundred ticks.
        let mut order: Vec<usize> = (0..self.n).collect();
        let r2_of = |i: usize, xs: &[i16], ys: &[i16]| {
            let x = xs[i] as f32 - cx;
            let y = ys[i] as f32 - cy;
            x * x + y * y
        };
        order.sort_by(|&a, &b| {
            r2_of(a, &self.xs_i, &self.ys_i).total_cmp(&r2_of(b, &self.xs_i, &self.ys_i))
        });
        let mut m_enc: f64 = 0.0;
        for &i in &order {
            m_enc += mass[i] as f64;
            if mass[i] == 0 {
                continue;
            }
            let x = self.xs_i[i] as f32 - cx;
            let y = self.ys_i[i] as f32 - cy;
            let r = (x * x + y * y).sqrt();
            if r < 1e-3 {
                continue;
            }
            let v = (Galaxy::GRAVATIONAL_CONSTANT * m_enc as f32 / r).sqrt();
            vel_x[i] += -y / r * v;
            vel_y[i] += x / r * v;
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

    /// Reproducible [`seed`] variant. Same `(additional, seed)` gives
    /// byte-identical state, enabling `?seed=...` URL sharing.
    pub fn seed_with(&self, additional: u16, seed: u64) -> Galaxy {
        self.seed_with_mode_seeded(additional, InitialCondition::Uniform, seed)
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

    /// Tick using externally-computed forces (e.g. WebGPU compute shader).
    /// Mismatched slice lengths default to zero-force.
    pub fn tick_with_accel(&self, time: f32, acc_x: &[f32], acc_y: &[f32]) -> Galaxy {
        let n = self.n;
        let mut next = Galaxy {
            size: self.size,
            n,
            mass: self.mass.clone(),
            acc_x: if acc_x.len() == n {
                acc_x.to_vec()
            } else {
                vec![0.0; n]
            },
            acc_y: if acc_y.len() == n {
                acc_y.to_vec()
            } else {
                vec![0.0; n]
            },
            vel_x: self.vel_x.clone(),
            vel_y: self.vel_y.clone(),
            frac_x: self.frac_x.clone(),
            frac_y: self.frac_y.clone(),
            xs_i: self.xs_i.clone(),
            ys_i: self.ys_i.clone(),
            inv_r3: self.inv_r3.clone(),
            scratch_mass: vec![0; n],
        };
        next.apply_acceleration(time);
        next
    }

    /// Physics constants for JS-side force backends (WGSL kernel params).
    /// Rust is the single source; never hardcode these in JS.
    pub fn gravitational_constant() -> f32 {
        Galaxy::GRAVATIONAL_CONSTANT
    }
    pub fn softening_sq() -> f32 {
        Galaxy::SOFTENING_SQ
    }
    pub fn repulse_r2() -> f32 {
        Galaxy::REPULSE_R2
    }

    /// Flat-buffer exposure for zero-copy JS reads via wasm.memory.
    pub fn mass_ptr(&self) -> *const u16 {
        self.mass.as_ptr()
    }
    pub fn mass_len(&self) -> usize {
        self.n
    }

    // Positions derivable from index + size. Kept for tests/older callers.
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

    // State-transfer accessors for Worker round-trip via transferable buffers.
    pub fn vel_x(&self) -> Vec<f32> {
        self.vel_x.clone()
    }
    pub fn vel_y(&self) -> Vec<f32> {
        self.vel_y.clone()
    }
    pub fn frac_x(&self) -> Vec<f32> {
        self.frac_x.clone()
    }
    pub fn frac_y(&self) -> Vec<f32> {
        self.frac_y.clone()
    }

    /// Hydrate a Galaxy from a state snapshot. Inverse of the getters.
    pub fn from_state(
        size: u16,
        mass: Vec<u16>,
        vel_x: Vec<f32>,
        vel_y: Vec<f32>,
        frac_x: Vec<f32>,
        frac_y: Vec<f32>,
    ) -> Galaxy {
        let base = Galaxy::new(size, 0);
        let n = base.n;
        assert_eq!(mass.len(), n, "mass length mismatch");
        assert_eq!(vel_x.len(), n, "vel_x length mismatch");
        assert_eq!(vel_y.len(), n, "vel_y length mismatch");
        assert_eq!(frac_x.len(), n, "frac_x length mismatch");
        assert_eq!(frac_y.len(), n, "frac_y length mismatch");
        Galaxy {
            size,
            n,
            mass,
            acc_x: vec![0.0; n],
            acc_y: vec![0.0; n],
            vel_x,
            vel_y,
            frac_x,
            frac_y,
            xs_i: base.xs_i,
            ys_i: base.ys_i,
            inv_r3: base.inv_r3,
            scratch_mass: vec![0; n],
        }
    }
}

impl Galaxy {
    /// World-disk radius: seeding stays inside it, the boundary spring
    /// engages past it.
    fn disk_radius(&self) -> f32 {
        (self.size as f32 * 0.5 - 1.0).max(1.0)
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

    /// Picks direct O(A squared) or Barnes-Hut O(N log N) by active count.
    fn gravitate_all(&mut self) {
        let n = self.n;

        // Iterate active cells (nonzero mass) instead of full N squared.
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

        // Crossover ~1000 active cells in WASM (measured).
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

        // Prebuild f32 masses so the inner loop stays cast-free.
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

    /// Semi-implicit Euler integration; merges collisions by momentum.
    fn apply_acceleration(&mut self, time: f32) {
        let size = self.size as i32;
        let max_step = Galaxy::MAX_SUBGRID_STEP;
        // dt-scaled drag; one exp per tick, not per cell.
        let drag = (-Galaxy::DRAG_COEFF * time).exp();
        // Circular-boundary spring (see CONFINE_STIFFNESS). Applied here,
        // not in the force kernels, so the CPU, Barnes-Hut, and WebGPU
        // paths all get it for free.
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        for i in 0..self.n {
            if self.mass[i] == 0 {
                continue;
            }
            let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
            let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
            let r = (x * x + y * y).sqrt();
            if r <= disk_r || r < 1e-3 {
                continue;
            }
            let k = Galaxy::CONFINE_STIFFNESS * (r - disk_r) / r;
            self.acc_x[i] -= k * x;
            self.acc_y[i] -= k * y;
        }

        // Zero scratch; momentum accumulators are local per-tick.
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
                // Empty cells: clear so stale values don't propagate later.
                self.vel_x[i] = 0.0;
                self.vel_y[i] = 0.0;
                self.frac_x[i] = 0.0;
                self.frac_y[i] = 0.0;
                continue;
            }

            // v += a · dt
            let mut vx = self.vel_x[i] + self.acc_x[i] * time;
            let mut vy = self.vel_y[i] + self.acc_y[i] * time;

            // Drag: grid-quantized sim overheats at large dt without it.
            // Must stay weak enough that rotation disks keep their angular
            // momentum for minutes of wall-clock, not seconds.
            vx *= drag;
            vy *= drag;

            // Sub-grid position update
            let mut fx = self.frac_x[i] + (vx * time).clamp(-max_step, max_step);
            let mut fy = self.frac_y[i] + (vy * time).clamp(-max_step, max_step);

            let (col, row) = (i as i32 % size, i as i32 / size);

            // Transfer to neighboring cell(s) as fractional offset crosses
            // ±0.5 (half-cell).
            let mut new_col = col;
            let mut new_row = row;
            let mut step_dx = 0i32;
            let mut step_dy = 0i32;
            if fx >= 0.5 {
                new_col += 1;
                fx -= 1.0;
                step_dx = 1;
            } else if fx <= -0.5 {
                new_col -= 1;
                fx += 1.0;
                step_dx = -1;
            }
            if fy >= 0.5 {
                new_row += 1;
                fy -= 1.0;
                step_dy = 1;
            } else if fy <= -0.5 {
                new_row -= 1;
                fy += 1.0;
                step_dy = -1;
            }

            let new_col = wrap(new_col, size) as u16;
            let new_row = wrap(new_row, size) as u16;
            let mut ni = self.col_row_to_index(new_col, new_row) as usize;

            // Incompressibility: a full destination rejects the transfer.
            // The mover parks at its cell edge with velocity intact (minus
            // friction) and flows through when a gap opens. Occupancy =
            // this tick's arrivals so far, plus the resident mass when the
            // resident (ni > i, row-major order) has not yet been
            // re-deposited into scratch. Slightly strict when the resident
            // is about to vacate - acceptable for a visual sim.
            let dest_occ = if ni > i {
                self.scratch_mass[ni].saturating_add(self.mass[ni] as u32)
            } else {
                self.scratch_mass[ni]
            };
            if ni != i
                && dest_occ > 0
                && dest_occ.saturating_add(m as u32) > Galaxy::CELL_MASS_CAP
            {
                ni = i;
                vx *= Galaxy::BLOCKED_FRICTION;
                vy *= Galaxy::BLOCKED_FRICTION;
                if step_dx != 0 {
                    fx = 0.49 * step_dx as f32;
                }
                if step_dy != 0 {
                    fy = 0.49 * step_dy as f32;
                }
            }

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

        // Pressure overflow: cells above the cap shed the excess to their
        // four neighbors, carrying momentum with the shed mass. Without
        // this a capped region gridlocks permanently (transfer rejection
        // alone freezes rms_radius within ~500 ticks). Sequential in-place
        // sweep - a shed can cascade within the same tick, which just
        // propagates the pressure wave faster.
        let cap = Galaxy::CELL_MASS_CAP as u16;
        for i in 0..self.n {
            let m = self.mass[i];
            if m <= cap {
                continue;
            }
            let share = (m - cap) / 4;
            if share == 0 {
                continue;
            }
            let (col, row) = (i as i32 % size, i as i32 / size);
            let (svx, svy) = (self.vel_x[i], self.vel_y[i]);
            for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let nc = wrap(col + dc, size) as u16;
                let nr = wrap(row + dr, size) as u16;
                let ni = self.col_row_to_index(nc, nr) as usize;
                let nm = self.mass[ni];
                let new_m = nm.saturating_add(share);
                let moved = new_m - nm;
                if moved == 0 {
                    continue;
                }
                let mf = new_m as f32;
                self.vel_x[ni] = (self.vel_x[ni] * nm as f32 + svx * moved as f32) / mf;
                self.vel_y[ni] = (self.vel_y[ni] * nm as f32 + svy * moved as f32) / mf;
                self.mass[ni] = new_m;
                self.mass[i] -= moved;
            }
        }
    }

}

/// Wrap-around: cells past the edge reappear on the other side.
#[inline]
fn wrap(value: i32, size: i32) -> i32 {
    let m = value % size;
    if m < 0 {
        m + size
    } else {
        m
    }
}

// Barnes-Hut quadtree (flat-arena).

const NO_CHILD: u32 = u32::MAX;

#[derive(Clone)]
struct Node {
    // Bounding box: centered at (cx, cy), half-side h. Root has cx=cy=h.
    cx: f32,
    cy: f32,
    h: f32,
    mass: f32,
    com_x: f32,
    com_y: f32,
    // Leaf: body index. Internal: NO_CHILD.
    body: u32,
    // Quadrants: NE=0, NW=1, SW=2, SE=3.
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

/// Insert body `b` into the subtree at `node_idx`. Indices avoid borrow fights.
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

        // Coincident bodies at deep levels: merge instead of subdividing.
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
    /// Force on (bx, by). Theta criterion: s/d < theta accepts subtree CoM.
    fn force(&self, bx: f32, by: f32, theta_sq: f32, soft: f32, g: f32) -> (f32, f32) {
        let mut ax = 0.0f32;
        let mut ay = 0.0f32;
        // Iterative DFS to bound recursion on deep trees.
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
                let mut k = g * inv_r3 * n.mass;
                // Contact repulsion, matching the direct-sum lookup table.
                if d2 <= Galaxy::REPULSE_R2 {
                    k = -k;
                }
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
        // Invariant for `?seed=...` URL sharing.
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
    fn test_seed_with_mode_seeded_is_reproducible_for_all_modes() {
        // Invariant for `?seed=...` URL sharing across initial conditions.
        for mode in [InitialCondition::Uniform, InitialCondition::Bang] {
            let a = Galaxy::new(20, 0).seed_with_mode_seeded(25, mode, 7);
            let b = Galaxy::new(20, 0).seed_with_mode_seeded(25, mode, 7);
            assert_eq!(a.mass, b.mass, "mass must be reproducible for {mode:?}");
            assert_eq!(a.vel_x, b.vel_x, "vel_x must be reproducible for {mode:?}");
            assert_eq!(a.vel_y, b.vel_y, "vel_y must be reproducible for {mode:?}");
            let c = Galaxy::new(20, 0).seed_with_mode_seeded(25, mode, 8);
            assert_ne!(a.mass, c.mass, "different seeds must differ for {mode:?}");
        }
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
    fn test_seed_uniform_produces_tangential_velocity() {
        // Orbital rotation is baked into every mode, uniform included.
        let g = Galaxy::new(20, 0).seed_with_mode(5, InitialCondition::Uniform);
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
    fn test_seed_uniform_has_positive_total_angular_momentum() {
        // Net L_z = Σ m_i (x_i v_{y,i} - y_i v_{x,i}) around the grid center
        // must be strongly positive — every mode carries disk rotation.
        let g = Galaxy::new(30, 0).seed_with_mode(10, InitialCondition::Uniform);
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

    #[test]
    fn test_tick_with_accel_no_panic() {
        let g = Galaxy::new(8, 1).seed(1);
        let n = g.n;
        let acc_x = vec![0.1f32; n];
        let acc_y = vec![-0.1f32; n];
        let next = g.tick_with_accel(0.5, &acc_x, &acc_y);
        assert_eq!(next.mass.len(), n);
    }

    #[test]
    fn test_tick_with_accel_zero_forces_keeps_mass_total() {
        // With zero forces, velocities don't grow so mass shouldn't
        // redistribute in the first tick. Total mass must be preserved.
        let g = Galaxy::new(6, 3).seed(0);
        let before: u64 = g.mass.iter().map(|&m| m as u64).sum();
        let n = g.n;
        let zeros = vec![0.0f32; n];
        let next = g.tick_with_accel(0.5, &zeros, &zeros);
        let after: u64 = next.mass.iter().map(|&m| m as u64).sum();
        assert_eq!(before, after);
    }

    #[test]
    fn test_tick_with_accel_mismatched_slice_no_panic() {
        // Caller-supplied slice length mismatch is treated as zero-force
        // so a bad caller can't panic across the WASM boundary.
        let g = Galaxy::new(4, 1);
        let bad = vec![1.0f32; 3];
        let _ = g.tick_with_accel(0.5, &bad, &bad);
    }

    #[test]
    fn test_tick_with_accel_positive_x_force_moves_mass_right() {
        // Single mass + uniform +x force: centroid must end up at larger x.
        let mut g = Galaxy::new(12, 0);
        let start_col: i32 = 2;
        let start_row: i32 = 6;
        let start_idx = (start_row * 12 + start_col) as usize;
        g.mass[start_idx] = 100;

        let centroid_x = |g: &Galaxy| -> f64 {
            let mut sum_mx: f64 = 0.0;
            let mut sum_m: f64 = 0.0;
            for i in 0..g.n {
                let m = g.mass[i] as f64;
                if m > 0.0 {
                    let col = (i as u16 % g.size) as f64;
                    sum_mx += col * m;
                    sum_m += m;
                }
            }
            if sum_m == 0.0 {
                0.0
            } else {
                sum_mx / sum_m
            }
        };

        let c0 = centroid_x(&g);

        // Uniform +x force for a small number of ticks - enough to move
        // but not enough to wrap around the 12-wide toroidal grid.
        let n = g.n;
        let ax = vec![5.0f32; n];
        let ay = vec![0.0f32; n];
        let mut cur = g;
        for _ in 0..6 {
            cur = cur.tick_with_accel(0.5, &ax, &ay);
        }

        let c1 = centroid_x(&cur);
        assert!(
            c1 > c0,
            "uniform +x force should push centroid right: before={c0}, after={c1}"
        );
    }

    #[test]
    fn test_tick_with_accel_matches_tick_when_forces_are_zero() {
        // With zero external forces AND zero existing velocity, nothing
        // moves: mass field must be identical after one tick.
        let g = Galaxy::new(8, 2).seed(42);
        let n = g.n;
        let zeros = vec![0.0f32; n];

        let no_force = g.tick_with_accel(0.5, &zeros, &zeros);

        assert_eq!(no_force.mass, g.mass);
    }
}

#[cfg(test)]
mod tests_dynamics {
    use super::*;

    fn angular_momentum(g: &Galaxy) -> f64 {
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
            lz += m * (x * g.vel_y[i] as f64 - y * g.vel_x[i] as f64);
        }
        lz
    }

    #[test]
    fn test_rotation_disk_retains_angular_momentum_over_long_run() {
        // Guards the drag coefficient: with the old flat 0.995/tick damping
        // a disk lost >60% of its L_z inside 200 ticks and every initial
        // condition collapsed into the same central blob within ~30s of
        // wall-clock. Drag must stay weak enough that orbits persist.
        let mut g = Galaxy::new(30, 0).seed_with_mode_seeded(10, InitialCondition::Uniform, 42);
        let l0 = angular_momentum(&g);
        assert!(l0 > 1.0, "rotation seed must start with positive L_z");
        for _ in 0..200 {
            g = g.tick(0.5);
        }
        let l1 = angular_momentum(&g);
        assert!(
            l1 > l0 * 0.5,
            "disk lost too much angular momentum: L_z {l0:.1} -> {l1:.1}"
        );
    }
}

#[cfg(test)]
mod tests_state_transfer {
    use super::*;

    #[test]
    fn roundtrips_mass_and_velocity() {
        // Seed + tick a galaxy to get non-trivial vel/frac state.
        let g = Galaxy::new(8, 1).seed(5).tick(1.0).tick(1.0);

        let mass = g.mass();
        let vx = g.vel_x();
        let vy = g.vel_y();
        let fx = g.frac_x();
        let fy = g.frac_y();

        let rehydrated = Galaxy::from_state(
            8,
            mass.clone(),
            vx.clone(),
            vy.clone(),
            fx.clone(),
            fy.clone(),
        );

        assert_eq!(rehydrated.mass, mass);
        assert_eq!(rehydrated.vel_x, vx);
        assert_eq!(rehydrated.vel_y, vy);
        assert_eq!(rehydrated.frac_x, fx);
        assert_eq!(rehydrated.frac_y, fy);

        // Ticking the rehydrated galaxy should produce the same next state
        // as ticking the original — i.e. state transfer is complete.
        let next_orig = g.tick(1.0);
        let next_rehyd = rehydrated.tick(1.0);
        assert_eq!(next_orig.mass, next_rehyd.mass);
        assert_eq!(next_orig.vel_x, next_rehyd.vel_x);
        assert_eq!(next_orig.vel_y, next_rehyd.vel_y);
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

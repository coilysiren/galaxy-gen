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

use rand::Rng;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Galaxy {
    size: u16,
    n: usize,
    min_star_mass: u16,

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
    pub const TYPE_INDEX_GAS: u8 = 0;
    pub const TYPE_INDEX_PLANET: u8 = 1;
    pub const TYPE_INDEX_STAR: u8 = 2;
    pub const TYPE_INDEX_WHITE_HOLE: u8 = 3;
    pub const STAR_MAX_MASS: u16 = u16::MAX / 2;

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
    pub fn new(size: u16, cell_initial_mass: u16, min_star_mass: u16) -> Galaxy {
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
            min_star_mass,
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

    pub fn seed(&self, additional: u16) -> Galaxy {
        let mut rng = rand::rng();
        let mut mass = self.mass.clone();
        if additional > 0 {
            for m in mass.iter_mut() {
                *m = m.saturating_add(rng.random_range(0..=additional));
            }
        }
        Galaxy {
            size: self.size,
            n: self.n,
            min_star_mass: self.min_star_mass,
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

    pub fn tick(&self, time: f32) -> Galaxy {
        let mut next = Galaxy {
            size: self.size,
            n: self.n,
            min_star_mass: self.min_star_mass,
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
    // (col, row) — x is column, y is row. Matches the pre-rewrite convention.
    #[inline]
    fn index_to_col_row(&self, index: u16) -> (u16, u16) {
        (index % self.size, index / self.size)
    }

    #[inline]
    fn col_row_to_index(&self, col: u16, row: u16) -> u16 {
        row * self.size + col
    }

    /// O(N²) all-pairs sweep. We deliberately *don't* use Newton's-3rd-law
    /// symmetry: the scatter writes that lets us sum a pair's effect into
    /// both cells in one shot are a cache nightmare in WASM (no aliasing
    /// proof → memory round-trip per write). Doing 2× the math but
    /// keeping the inner loop to local scalars is substantially faster in
    /// practice — the per-pair cost drops enough that the 2× overhead
    /// pays for itself several times over, and the loop becomes
    /// auto-vectorization-friendly.
    ///
    /// Pre-computed f32 masses + (x,y) coords so the inner loop has zero
    /// casts / zero allocations / zero memory traffic except the flat
    /// reads from three contiguous `Vec<f32>`s.
    fn gravitate_all(&mut self) {
        let n = self.n;

        // Pre-convert masses to f32 once. The inner loop then touches four
        // flat `Vec`s, all hot in cache for N ≤ ~15k.
        let mut mass_f = Vec::<f32>::with_capacity(n);
        for i in 0..n {
            mass_f.push(self.mass[i] as f32);
        }

        let mass_s = mass_f.as_slice();
        let xs_i = self.xs_i.as_slice();
        let ys_i = self.ys_i.as_slice();
        let inv_r3_tbl = self.inv_r3.as_slice();

        for i in 0..n {
            let mi = mass_s[i];
            if mi == 0.0 {
                self.acc_x[i] = 0.0;
                self.acc_y[i] = 0.0;
                continue;
            }
            let ix = xs_i[i] as i32;
            let iy = ys_i[i] as i32;
            let mut ax = 0.0f32;
            let mut ay = 0.0f32;

            for j in 0..n {
                let mj = mass_s[j];
                // Integer diffs → integer r² → table lookup. No sqrt, no
                // transcendental math in the hot path.
                let dx_i = xs_i[j] as i32 - ix;
                let dy_i = ys_i[j] as i32 - iy;
                let r2_idx = (dx_i * dx_i + dy_i * dy_i) as usize;
                let k = inv_r3_tbl[r2_idx] * mj;
                ax += k * dx_i as f32;
                ay += k * dy_i as f32;
            }

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

    // Kept for tests / older callers.
    fn reach_range_start(&self, index: u16, reach: u16) -> u16 {
        index.saturating_sub(reach)
    }
    fn reach_range_end(&self, index: u16, reach: u16) -> u16 {
        index.saturating_add(reach).min(self.size - 1)
    }
    fn reach_range(&self, index: u16, reach: u16) -> (u16, u16) {
        (
            self.reach_range_start(index, reach),
            self.reach_range_end(index, reach),
        )
    }
    fn neighbours(&self, index: u16, reach: u16) -> Vec<(u16, u16)> {
        let mut out = Vec::new();
        let (col, row) = self.index_to_col_row(index);
        let (col_start, col_end) = self.reach_range(col, reach);
        let (row_start, row_end) = self.reach_range(row, reach);
        for r in row_start..=row_end {
            for c in col_start..=col_end {
                if (c, r) != (col, row) {
                    out.push((c, r));
                }
            }
        }
        out
    }
    fn get_type_index(&self, mass: u16) -> u8 {
        if mass < self.min_star_mass {
            Galaxy::TYPE_INDEX_GAS
        } else {
            Galaxy::TYPE_INDEX_STAR
        }
    }
    fn distance(&self, index: u16, neighbour_coords: (u16, u16)) -> f32 {
        let (cx, cy) = self.index_to_col_row(index);
        let dx = (cx as i32 - neighbour_coords.0 as i32) as f32;
        let dy = (cy as i32 - neighbour_coords.1 as i32) as f32;
        (dx * dx + dy * dy).sqrt()
    }
    fn degrees(&self, index: u16, neighbour_coords: (u16, u16)) -> f32 {
        let (cx, cy) = self.index_to_col_row(index);
        let x = neighbour_coords.0 as i32 - cx as i32;
        let y = neighbour_coords.1 as i32 - cy as i32;
        (x as f32).atan2(y as f32).to_degrees()
    }
    fn neighbours_of_my_type(&self, index: u16) -> Vec<(u16, u16)> {
        let type_index = self.get_type_index(self.mass[index as usize]);
        let reach = match type_index {
            Galaxy::TYPE_INDEX_STAR => self.size,
            Galaxy::TYPE_INDEX_GAS => (self.size as f32).sqrt() as u16,
            _ => 0,
        };
        self.neighbours(index, reach)
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
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests_intial_generation {
    use super::*;
    #[test]
    fn test_inital_generation_no_panic() {
        Galaxy::new(10, 0, 1000);
    }
    #[test]
    fn test_seed_no_panic() {
        Galaxy::new(10, 0, 1000).seed(1);
    }
    #[test]
    fn test_seed_tick_no_panic() {
        Galaxy::new(10, 1, 1000).seed(1).tick(1.0);
    }
    #[test]
    fn test_seed_alters_data() {
        let g = Galaxy::new(10, 0, 1000);
        let before = g.mass.clone();
        let g = g.seed(1);
        assert_ne!(before, g.mass);
    }
    #[test]
    fn test_seed_doesnt_alter_when_zero() {
        let g = Galaxy::new(10, 0, 1000);
        let before = g.mass.clone();
        let g = g.seed(0);
        assert_eq!(before, g.mass);
    }
    #[test]
    fn test_seed_alters_data_twice() {
        let g = Galaxy::new(10, 0, 1000);
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
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.index_to_col_row(0), (0, 0));
    }
    #[test]
    fn test_col_row_to_index_start() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.col_row_to_index(0, 0), 0);
    }
    #[test]
    fn test_index_to_col_row_center() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.index_to_col_row(4), (1, 1));
    }
    #[test]
    fn test_col_row_to_index_center() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.col_row_to_index(1, 1), 4);
    }
    #[test]
    fn test_index_to_col_row_end() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.index_to_col_row(8), (2, 2));
    }
    #[test]
    fn test_col_row_to_index_end() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.col_row_to_index(2, 2), 8);
    }
    #[test]
    fn test_index_edge_transform_top_right() {
        let g = Galaxy::new(3, 0, 1000);
        let index = 2;
        let (x, y) = g.index_to_col_row(index);
        assert_eq!(g.col_row_to_index(x, y), index);
        assert_eq!((x, y), (2, 0));
    }
    #[test]
    fn test_index_edge_transform_bottom_left() {
        let g = Galaxy::new(3, 0, 1000);
        let index = 6;
        let (x, y) = g.index_to_col_row(index);
        assert_eq!(g.col_row_to_index(x, y), index);
        assert_eq!((x, y), (0, 2));
    }
}

#[cfg(test)]
mod tests_neighbors_and_reach {
    use super::*;
    #[test]
    fn test_mass() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.mass(), vec![0, 0, 0, 0, 0, 0, 0, 0, 0]);
    }
    #[test]
    fn test_x() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.x(), vec![0, 1, 2, 0, 1, 2, 0, 1, 2]);
    }
    #[test]
    fn test_y() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.y(), vec![0, 0, 0, 1, 1, 1, 2, 2, 2]);
    }
    #[test]
    fn test_reach_range_start_edge() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.reach_range_start(0, 99), 0);
    }
    #[test]
    fn test_reach_range_start_overflow() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.reach_range_start(1, 99), 0);
    }
    #[test]
    fn test_reach_range_start_contained() {
        let g = Galaxy::new(10, 0, 1000);
        assert_eq!(g.reach_range_start(4, 2), 2);
    }
    #[test]
    fn test_reach_range_end_edge() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.reach_range_end(2, 99), 2);
    }
    #[test]
    fn test_reach_range_end_overflow() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.reach_range_end(0, 99), 2);
    }
    #[test]
    fn test_reach_range_end_contained() {
        let g = Galaxy::new(10, 0, 1000);
        assert_eq!(g.reach_range_end(2, 2), 4);
    }
    #[test]
    fn test_neighbor_size() {
        let g = Galaxy::new(10, 0, 1000);
        assert_eq!(g.neighbours(0, 1).len(), 3);
    }
    #[test]
    fn test_neighbor_size_larger() {
        let g = Galaxy::new(10, 0, 1000);
        assert_eq!(g.neighbours(0, 2).len(), 8);
        assert_eq!(g.neighbours(0, u16::MAX).len(), 99);
    }
    #[test]
    fn test_neighbor_size_center() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.neighbours(4, 1).len(), 8);
        assert_eq!(g.neighbours(4, u16::MAX).len(), 8);
    }
    #[test]
    fn test_neighbor_size_differs_for_large_galaxy() {
        let mut g = Galaxy::new(100, 0, 1000);
        g.mass[0] = 1;
        let gas_neighbours = g.neighbours_of_my_type(0).len();
        g.mass[0] = 65535;
        let star_neighbours = g.neighbours_of_my_type(0).len();
        assert_ne!(gas_neighbours, star_neighbours);
    }
    #[test]
    fn test_neighbor_size_same_for_small_galaxy() {
        let mut g = Galaxy::new(1, 0, 1000);
        g.mass[0] = 1;
        let gas_neighbours = g.neighbours_of_my_type(0).len();
        g.mass[0] = 59999;
        let star_neighbours = g.neighbours_of_my_type(0).len();
        assert_eq!(gas_neighbours, star_neighbours);
    }
}

#[cfg(test)]
mod tests_distance {
    use super::*;
    #[test]
    fn test_distance_one() {
        let g = Galaxy::new(3, 0, 1000);
        let d = (g.distance(0, (1, 1)) * 100.0).round() / 100.0;
        assert_eq!(d, 1.41);
    }
    #[test]
    fn test_distance_two() {
        let g = Galaxy::new(3, 0, 1000);
        let d = (g.distance(0, (2, 2)) * 100.0).round() / 100.0;
        assert_eq!(d, 2.83);
    }
    #[test]
    fn test_distance_two_linear() {
        let g = Galaxy::new(3, 0, 1000);
        let d = (g.distance(0, (0, 2)) * 100.0).round() / 100.0;
        assert_eq!(d, 2.00);
    }
}

mod tests_degreess {
    use super::*;
    #[test]
    fn test_degreess_x() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.degrees(0, (2, 0)).round() as u16, 90);
    }
    #[test]
    fn test_degreess_y() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.degrees(0, (0, 2)).round() as u16, 0);
    }
    #[test]
    fn test_degreess_z_one() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.degrees(0, (2, 2)).round() as u16, 45);
    }
    #[test]
    fn test_degreess_z_two() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.degrees(0, (1, 2)).round() as u16, 27);
    }
    #[test]
    fn test_degreess_z_three() {
        let g = Galaxy::new(3, 0, 1000);
        assert_eq!(g.degrees(0, (2, 1)).round() as u16, 63);
    }
}

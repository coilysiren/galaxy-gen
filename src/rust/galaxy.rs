//! Galaxy simulation. See docs/galaxy-rust.md.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use wasm_bindgen::prelude::*;

use crate::events::{Event, EventQueue};
use crate::process;
use crate::stars::{Stars, NO_CLUSTER};

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
#[derive(Clone)]
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

    /// Ticks elapsed since seeding. Drives process cadence and the
    /// per-tick RNG stream derivation.
    tick_count: u64,
    /// Master seed for the RNG service. 0 until seeded.
    master_seed: u64,
    events: EventQueue,

    pub(crate) stars: Stars,
    /// Central black hole point mass, set at seed time and live
    /// thereafter: it grows by accreting core gas and capturing stars,
    /// and shrinks by (exaggerated) Hawking evaporation. Participates in
    /// the coarse gravity field (stars feel it); the gas force kernels
    /// predate it and stay untouched to preserve their tuning.
    bh_mass: f32,
    /// Seed-time black hole mass; the renderer scales the lens depth by
    /// sqrt(bh_mass / bh_mass_initial).
    bh_mass_initial: f32,
    /// Mass lost to Hawking radiation - a ledger sink like dissipation.
    radiated_total: f64,
    /// Coarse acceleration field (FIELD_RES x FIELD_RES over the world),
    /// rebuilt by process_gravity_field, bilinear-sampled by stars.
    field_ax: Vec<f32>,
    field_ay: Vec<f32>,
    /// Coarse radiation field (FIELD_RES x FIELD_RES): star luminosity
    /// deposits with decay. Hot gas resists collapse and dissipates.
    radiation: Vec<f32>,
    /// Per-cell sustained-density counter driving CloudCollapse. Bumped
    /// by collapse_watch when a cell stays dense, slow, and cool.
    collapse_heat: Vec<u8>,
    /// Mass destroyed by radiation dissipation - the one documented sink
    /// in the baryonic ledger (gas + stars + pending births + this).
    dissipated_total: u64,
    next_cluster_id: u32,
    next_star_id: u32,
    /// Causal attribution for shock-boosted collapse heat: the ShockWave
    /// event id that last boosted each cell, 0 = organic. Lets an induced
    /// CloudCollapse carry its true parent.
    heat_parent: Vec<u64>,
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
    /// Spring stiffness of the circular world boundary for GAS. Beyond
    /// the disk radius (size/2 - 1) cells feel a gentle inward pull
    /// proportional to overshoot. Gas is grid-bound anyway; stars use
    /// the two-tier halo confinement below instead.
    const CONFINE_STIFFNESS: f32 = 0.02;
    /// Stars: hard-clip radius as a multiple of the soft (disk) radius.
    /// 3x leaves a wide halo band now that the renderer shows past the
    /// disk edge.
    /// Between soft and hard lies the halo band with a repulsive
    /// gradient a = K (r - soft)/(hard - r) - gentle at the soft edge,
    /// divergent at the hard edge, so no finite speed reaches the hard
    /// clip. Replaces the old rim hard-stop that parked all ejecta in a
    /// ring at disk_r + 3.
    const HARD_CLIP_FACTOR: f32 = 3.0;
    /// Gradient scale for the halo repulsion.
    const HALO_STIFFNESS: f32 = 0.04;
    /// Acceleration ceiling for the halo gradient (the analytic form
    /// diverges at the hard clip; the clamp keeps integration sane).
    const HALO_ACCEL_MAX: f32 = 2.0;
    /// Velocity drag applied to stars only while in the halo band. The
    /// halo spring is conservative - without dissipation ejecta would
    /// oscillate through the halo forever instead of rejoining the disk.
    const STAR_HALO_DRAG: f32 = 0.04;
    /// Barnes-Hut opening angle, shared by the gas kernel and the coarse
    /// field builder.
    const THETA: f32 = 0.7;
    /// Coarse gravity-field resolution (per axis).
    const FIELD_RES: usize = 64;
    /// Softening for the coarse field build (separate from the gas
    /// kernel's SOFTENING_SQ). Large on purpose: the star field is a
    /// mean field. With point-scale softening, stars dive through steep
    /// cluster wells sampled from a 4-tick-stale field and the
    /// integration error pumps orbital energy until the disk evaporates
    /// into the halo.
    const FIELD_SOFTENING_SQ: f32 = 25.0;
    /// Central black hole mass as a fraction of total seeded mass.
    const BH_MASS_FRACTION: f32 = 0.05;
    // Stellar population. Births sample a Salpeter-flavored IMF
    // (dN/dm proportional to m^-2.35) between the mass bounds: many
    // faint red dwarfs, rare blue giants. Luminosity follows a
    // main-sequence-ish power law and lifetime falls steeply with mass,
    // so M-dwarfs outlive the session while O-stars die in minutes.
    const STAR_MASS_MIN: f32 = 3.0;
    const STAR_MASS_MAX: f32 = 120.0;
    const IMF_ALPHA: f32 = 2.35;
    /// Lifetime = COEFF x (30/m)^2 sim-time units.
    const STAR_LIFETIME_COEFF: f32 = 900.0;
    /// Max stars spawned per birth event (render + integration budget).
    const BIRTH_MAX_STARS: usize = 24;

    // Uniform-seed structure. Region-scale value noise (two octaves)
    // replaces per-cell white noise, and a two-arm logarithmic-spiral
    // overdensity seeds the density wave that differential rotation
    // shears into a pinwheel. ROTATION_BOOST spins the disk slightly
    // super-circular so the shear actually stretches the arms.
    const NOISE_COARSE_RES: usize = 7;
    const NOISE_MID_RES: usize = 17;
    const SPIRAL_AMP: f32 = 0.55;
    const SPIRAL_PITCH: f32 = 4.0;
    const ROTATION_BOOST: f32 = 1.1;

    // Cloud-collapse tuning. A cell must stay at or above the density
    // fraction of CELL_MASS_CAP and below the radiation resist level for
    // COLLAPSE_HEAT_TRIGGER consecutive scans (collapse_watch cadence 16)
    // before it can roll for collapse. No velocity gate: jammed cells
    // accumulate large STORED velocity while standing still, so a speed
    // limit anti-selects exactly the proto-cluster cells.
    const COLLAPSE_DENSITY_FRACTION: f32 = 0.75;
    const COLLAPSE_HEAT_TRIGGER: u8 = 6;
    const COLLAPSE_CHANCE: f32 = 0.35;
    const COLLAPSE_RADIATION_RESIST: f32 = 20.0;
    /// Fraction of the collapsing cell's gas consumed into the birth
    /// budget; neighbors contribute half this fraction.
    const COLLAPSE_CONSUME_FRACTION: f32 = 0.55;
    /// Minimum birth budget - collapses thinner than this fizzle.
    const BIRTH_MIN_BUDGET: f32 = 20.0;
    /// Cap on the gas velocity a newborn star inherits. Jammed cells
    /// accumulate unbounded STORED velocity while barely moving (the
    /// movement cap bounds real motion to ~1 cell per time unit) -
    /// inheriting the raw value launches newborns straight into the
    /// halo and empties the visible disk.
    const BIRTH_GAS_VEL_CAP: f32 = 1.0;
    /// Cap on the orbital-support speed given to newborns. In a clumpy
    /// field the local sample points at the nearest cluster, not the
    /// center - sqrt(|a| r) from a mis-aimed sample slingshots newborns
    /// outward.
    const BIRTH_VCIRC_CAP: f32 = 1.5;

    // Radiation tuning. Deposits scale luminosity into the coarse field
    // with a 3x3 splat; the field decays every rebuild.
    const RAD_DEPOSIT_SCALE: f32 = 0.01;
    const RAD_DECAY: f32 = 0.85;
    /// Above this radiation level gas dissipates (mass -> dissipated
    /// ledger), emitting CloudDissipate when a cell empties.
    const RAD_DISSIPATE_THRESHOLD: f32 = 60.0;

    // Supernova tuning. Main-sequence stars past their lifetime and at
    // or above the mass threshold detonate; lighter ones fade to
    // remnants. A supernova returns most of the star's mass to nearby
    // gas with an outward kick and leaves a dim compact remnant.
    const SN_MASS_THRESHOLD: f32 = 30.0;
    const SN_GAS_RETURN: f32 = 0.8;
    const SN_KICK: f32 = 1.2;
    const SN_RADIUS: i32 = 2;
    /// ShockWave heat boost applied to cells within SHOCK_RADIUS - the
    /// induced-collapse coupling that closes the loop.
    const SHOCK_HEAT_BOOST: u8 = 3;
    const SHOCK_RADIUS: i32 = 3;
    /// Renderer transient window (ticks) for executed-event flashes.
    const TRANSIENT_WINDOW: u64 = 90;

    // Black hole lifecycle. Accretion eats a fraction of the gas within
    // BH_ACCRETION_RADIUS of the center each run; stars inside
    // BH_CAPTURE_RADIUS are swallowed via BlackHoleCapture events.
    // Hawking evaporation follows the physically-shaped dM/dt =
    // -HAWKING_COEFF / M^2 (small holes evaporate in a runaway, big
    // ones barely leak), with the coefficient exaggerated enormously -
    // a real stellar-mass hole radiates nanokelvins and would outlive
    // 10^60 sessions.
    const BH_ACCRETION_RADIUS: i32 = 2;
    const BH_ACCRETION_FRACTION: f32 = 0.01;
    /// Capture needs BOTH deep proximity and low speed - fast stars
    /// slingshot through the center, only slow ones fall in. Without the
    /// speed gate every orbit through the core re-rolls the capture dice
    /// and the hole eats the galaxy.
    const BH_CAPTURE_RADIUS: f32 = 0.5;
    const BH_CAPTURE_MAX_SPEED: f32 = 0.8;
    const HAWKING_COEFF: f32 = 12_000.0;

    // RNG stream ids (see rng_stream).
    const RNG_COLLAPSE_WATCH: u64 = 1;
    const RNG_STAR_BIRTH: u64 = 2;
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
            tick_count: 0,
            master_seed: 0,
            events: EventQueue::new(),
            stars: Stars::new(),
            bh_mass: 0.0,
            bh_mass_initial: 0.0,
            radiated_total: 0.0,
            field_ax: vec![0.0; Galaxy::FIELD_RES * Galaxy::FIELD_RES],
            field_ay: vec![0.0; Galaxy::FIELD_RES * Galaxy::FIELD_RES],
            radiation: vec![0.0; Galaxy::FIELD_RES * Galaxy::FIELD_RES],
            collapse_heat: vec![0; n],
            dissipated_total: 0,
            next_cluster_id: 0,
            next_star_id: 1,
            heat_parent: vec![0; n],
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
        let seed: u64 = rand::rng().random();
        self.seed_with_mode_seeded(additional, mode, seed)
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
        self.seed_mode_kernel(additional, mode, seed, &mut rng)
    }

    // Private, so wasm-bindgen skips it; `dyn` because bindgen impls
    // cannot hold generics.
    fn seed_mode_kernel(
        &self,
        additional: u16,
        mode: InitialCondition,
        master_seed: u64,
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
                    // Region noise: coarse cloud/void structure times a
                    // finer texture octave, sampled bilinearly.
                    let n_coarse = Galaxy::NOISE_COARSE_RES;
                    let n_mid = Galaxy::NOISE_MID_RES;
                    let coarse: Vec<f32> = (0..n_coarse * n_coarse)
                        .map(|_| rng.random_range(0.25f32..1.75))
                        .collect();
                    let mid: Vec<f32> = (0..n_mid * n_mid)
                        .map(|_| rng.random_range(0.55f32..1.45))
                        .collect();
                    let spiral_phase = rng.random_range(0.0f32..std::f32::consts::TAU);
                    let bilinear = |grid: &[f32], res: usize, u: f32, v: f32| -> f32 {
                        let fu = (u * (res - 1) as f32).clamp(0.0, (res - 1) as f32);
                        let fv = (v * (res - 1) as f32).clamp(0.0, (res - 1) as f32);
                        let x0 = fu as usize;
                        let y0 = fv as usize;
                        let x1 = (x0 + 1).min(res - 1);
                        let y1 = (y0 + 1).min(res - 1);
                        let tx = fu - x0 as f32;
                        let ty = fv - y0 as f32;
                        let a = grid[y0 * res + x0] * (1.0 - tx) + grid[y0 * res + x1] * tx;
                        let b = grid[y1 * res + x0] * (1.0 - tx) + grid[y1 * res + x1] * tx;
                        a * (1.0 - ty) + b * ty
                    };
                    for i in 0..self.n {
                        let x = self.xs_i[i] as f32 - cx;
                        let y = self.ys_i[i] as f32 - cy;
                        if x * x + y * y > disk_r2 {
                            continue;
                        }
                        let u = (x / size + 0.5).clamp(0.0, 1.0);
                        let v = (y / size + 0.5).clamp(0.0, 1.0);
                        let region = bilinear(&coarse, n_coarse, u, v)
                            * bilinear(&mid, n_mid, u, v);
                        let r = (x * x + y * y).sqrt().max(1.0);
                        let theta = y.atan2(x);
                        // Two-arm density wave: cos(2 theta - pitch ln r).
                        let arm = 1.0
                            + Galaxy::SPIRAL_AMP
                                * (2.0 * theta - Galaxy::SPIRAL_PITCH * r.ln()
                                    + spiral_phase)
                                    .cos();
                        let m = additional as f32 * 0.5
                            * region
                            * arm
                            * rng.random_range(0.85f32..1.15);
                        mass[i] = mass[i]
                            .saturating_add(m.round().clamp(0.0, u16::MAX as f32) as u16);
                    }
                }
            }
            InitialCondition::Bang => {
                for m in mass.iter_mut() {
                    *m = 0;
                }
                let core_radius = (size * 0.15).max(2.0);
                let core_r2 = core_radius * core_radius;
                // `additional` is the intensity knob (fixed SEED_MASS
                // constant on the JS side; the URL knob is retired).
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
            let v = (Galaxy::GRAVATIONAL_CONSTANT * m_enc as f32 / r).sqrt()
                * Galaxy::ROTATION_BOOST;
            vel_x[i] += -y / r * v;
            vel_y[i] += x / r * v;
        }

        // Central black hole anchors the nucleus, scaled to seeded mass.
        let total_mass: f64 = mass.iter().map(|&m| m as f64).sum();

        let mut g = self.clone();
        g.mass = mass;
        g.vel_x = vel_x;
        g.vel_y = vel_y;
        g.acc_x = vec![0.0; self.n];
        g.acc_y = vec![0.0; self.n];
        g.frac_x = vec![0.0; self.n];
        g.frac_y = vec![0.0; self.n];
        g.scratch_mass = vec![0; self.n];
        g.tick_count = 0;
        g.master_seed = master_seed;
        g.events = EventQueue::new();
        g.stars = Stars::new();
        g.bh_mass = total_mass as f32 * Galaxy::BH_MASS_FRACTION;
        g.bh_mass_initial = g.bh_mass;
        g.radiated_total = 0.0;
        g.radiation = vec![0.0; Galaxy::FIELD_RES * Galaxy::FIELD_RES];
        g.collapse_heat = vec![0; self.n];
        g.dissipated_total = 0;
        g.next_cluster_id = 0;
        g.next_star_id = 1;
        g.heat_parent = vec![0; self.n];
        g
    }

    /// Reproducible [`seed`] variant. Same `(additional, seed)` gives
    /// byte-identical state, enabling `?seed=...` URL sharing.
    pub fn seed_with(&self, additional: u16, seed: u64) -> Galaxy {
        self.seed_with_mode_seeded(additional, InitialCondition::Uniform, seed)
    }

    /// One simulation step: run every due process in registry order, then
    /// execute the events scheduled for this tick (emitted last tick).
    pub fn tick(&self, time: f32) -> Galaxy {
        let mut next = self.clone();
        next.tick_count += 1;
        for p in process::registry() {
            if process::is_due(p, next.tick_count) {
                (p.run)(&mut next, time);
            }
        }
        let due = next.events.take_due(next.tick_count);
        next.execute_events(due, time);
        next
    }

    /// Tick using externally-computed forces (e.g. WebGPU compute shader).
    /// Mismatched slice lengths default to zero-force.
    pub fn tick_with_accel(&self, time: f32, acc_x: &[f32], acc_y: &[f32]) -> Galaxy {
        let n = self.n;
        let mut next = self.clone();
        next.tick_count += 1;
        next.acc_x = if acc_x.len() == n {
            acc_x.to_vec()
        } else {
            vec![0.0; n]
        };
        next.acc_y = if acc_y.len() == n {
            acc_y.to_vec()
        } else {
            vec![0.0; n]
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

    // --- Star population surface -----------------------------------

    pub fn star_count(&self) -> usize {
        self.stars.len()
    }

    /// Renderer packing: [x, y, luminosity, color_index] per star.
    pub fn star_render_data(&self) -> Vec<f32> {
        self.stars.render_data()
    }

    /// Spawn one star directly. Debug/test path - production stars are
    /// born from CloudCollapse -> StarBirth events. Derived attributes
    /// (lifetime, luminosity, color) come from mass.
    pub fn spawn_star(&mut self, x: f32, y: f32, vx: f32, vy: f32, mass: f32) -> usize {
        let m = mass.max(1.0);
        let (lifetime, luminosity, class_index) = Galaxy::star_attrs(m);
        let id = self.next_star_id;
        self.next_star_id += 1;
        self.stars
            .spawn(x, y, vx, vy, m, lifetime, luminosity, class_index, NO_CLUSTER, id)
    }

    /// Renderer transients: [kind, x, y, ticks_ago, magnitude] per recent
    /// executed event within the transient window (Supernova and
    /// StarBirth). Magnitude is the event payload - progenitor mass for
    /// a supernova, birth budget for a star birth - so blasts scale with
    /// stellar class. Render-only.
    pub fn render_transients(&self) -> Vec<f32> {
        let size = self.size as i32;
        let mut out = Vec::new();
        for ev in self.events.recent() {
            let age = self.tick_count.saturating_sub(ev.tick);
            if age > Galaxy::TRANSIENT_WINDOW {
                continue;
            }
            let (kind, cell) = match ev.kind {
                crate::events::EventKind::Supernova => (2.0f32, ev.target),
                crate::events::EventKind::StarBirth => (1.0f32, ev.target),
                _ => continue,
            };
            let cell = cell as i32;
            if cell < 0 || cell >= size * size {
                continue;
            }
            out.push(kind);
            out.push((cell % size) as f32);
            out.push((cell / size) as f32);
            out.push(age as f32);
            out.push(ev.payload);
        }
        out
    }

    pub fn bh_mass_value(&self) -> f32 {
        self.bh_mass
    }

    /// Authoritative simulation tick, continuous across worker
    /// pause/resume (it rides the meta state). f64 because wasm-bindgen
    /// maps u64 to BigInt and every consumer wants a plain number.
    pub fn sim_tick(&self) -> f64 {
        self.tick_count as f64
    }

    /// Coarse radiation field for the renderer's gas temperature tiers.
    pub fn radiation_field(&self) -> Vec<f32> {
        self.radiation.clone()
    }

    pub fn radiation_res() -> usize {
        Galaxy::FIELD_RES
    }

    /// Lens-depth scale for the renderer: sqrt of the black hole's mass
    /// relative to its seeded mass. 0 once the hole has evaporated.
    pub fn bh_lens_scale(&self) -> f32 {
        if self.bh_mass_initial <= 0.0 {
            return 0.0;
        }
        (self.bh_mass / self.bh_mass_initial).max(0.0).sqrt()
    }

    /// Executed-event count by kind index (EventKind discriminant).
    /// Instrumentation surface for debug_sim and the UI stats row.
    pub fn events_executed(&self, kind: u32) -> u64 {
        use crate::events::EventKind::*;
        let k = match kind {
            0 => CloudCollapse,
            1 => StarBirth,
            2 => Supernova,
            3 => ShockWave,
            5 => BlackHoleCapture,
            _ => CloudDissipate,
        };
        self.events.executed_count(k)
    }

    // --- Worker state round-trip (opaque to JS) ---------------------

    /// Full star state, STAR_FLOATS per star. Opaque to JS: hold it and
    /// hand it back to `restore_sim_state_stars`.
    pub fn sim_state_stars(&self) -> Vec<f32> {
        self.stars.to_flat()
    }

    pub fn restore_sim_state_stars(&mut self, data: &[f32]) {
        self.stars = Stars::from_flat(data);
    }

    /// Coarse-field state: [field_ax..., field_ay..., radiation...]. The
    /// fields are mid-tick derived state - rebuilding after restore would
    /// use post-tick inputs and fork the trajectory. Opaque to JS.
    pub fn sim_state_field(&self) -> Vec<f32> {
        let mut out = self.field_ax.clone();
        out.extend_from_slice(&self.field_ay);
        out.extend_from_slice(&self.radiation);
        out
    }

    pub fn restore_sim_state_field(&mut self, data: &[f32]) {
        let res = Galaxy::FIELD_RES * Galaxy::FIELD_RES;
        if data.len() != res * 3 {
            return;
        }
        self.field_ax.copy_from_slice(&data[..res]);
        self.field_ay.copy_from_slice(&data[res..res * 2]);
        self.radiation.copy_from_slice(&data[res * 2..]);
    }

    /// Versioned scheduler/event/RNG state: [version=4, tick lo/hi, seed
    /// lo/hi, bh_mass bits, bh_initial bits, radiated f64 bits lo/hi,
    /// dissipated lo/hi, next_cluster, next_star, n_cells, heat bytes
    /// packed 4-per-u32, heat_parent lo/hi per cell, then the
    /// event-queue flat form]. Opaque to JS.
    pub fn sim_state_meta(&self) -> Vec<u32> {
        let heat_words = self.n.div_ceil(4);
        let mut out = Vec::with_capacity(14 + heat_words + self.n * 2 + 6);
        out.push(4u32);
        out.push(self.tick_count as u32);
        out.push((self.tick_count >> 32) as u32);
        out.push(self.master_seed as u32);
        out.push((self.master_seed >> 32) as u32);
        out.push(self.bh_mass.to_bits());
        out.push(self.bh_mass_initial.to_bits());
        let rad_bits = self.radiated_total.to_bits();
        out.push(rad_bits as u32);
        out.push((rad_bits >> 32) as u32);
        out.push(self.dissipated_total as u32);
        out.push((self.dissipated_total >> 32) as u32);
        out.push(self.next_cluster_id);
        out.push(self.next_star_id);
        out.push(self.n as u32);
        for chunk in self.collapse_heat.chunks(4) {
            let mut w = 0u32;
            for (k, &b) in chunk.iter().enumerate() {
                w |= (b as u32) << (8 * k);
            }
            out.push(w);
        }
        for &hp in &self.heat_parent {
            out.push(hp as u32);
            out.push((hp >> 32) as u32);
        }
        out.extend(self.events.to_flat());
        out
    }

    pub fn restore_sim_state_meta(&mut self, data: &[u32]) {
        if data.len() < 14 || data[0] != 4 {
            return;
        }
        self.tick_count = data[1] as u64 | ((data[2] as u64) << 32);
        self.master_seed = data[3] as u64 | ((data[4] as u64) << 32);
        self.bh_mass = f32::from_bits(data[5]);
        self.bh_mass_initial = f32::from_bits(data[6]);
        self.radiated_total =
            f64::from_bits(data[7] as u64 | ((data[8] as u64) << 32));
        self.dissipated_total = data[9] as u64 | ((data[10] as u64) << 32);
        self.next_cluster_id = data[11];
        self.next_star_id = data[12];
        let n_cells = data[13] as usize;
        if n_cells != self.n {
            return;
        }
        let heat_words = n_cells.div_ceil(4);
        let parents_at = 14 + heat_words;
        let events_at = parents_at + n_cells * 2;
        if data.len() < events_at {
            return;
        }
        for i in 0..n_cells {
            let w = data[14 + i / 4];
            self.collapse_heat[i] = ((w >> (8 * (i % 4))) & 0xFF) as u8;
        }
        for i in 0..n_cells {
            self.heat_parent[i] =
                data[parents_at + i * 2] as u64 | ((data[parents_at + i * 2 + 1] as u64) << 32);
        }
        self.events = EventQueue::from_flat(&data[events_at..]);
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
        let mut g = base;
        g.mass = mass;
        g.vel_x = vel_x;
        g.vel_y = vel_y;
        g.frac_x = frac_x;
        g.frac_y = frac_y;
        g
    }
}

impl Galaxy {
    // Process entry points, referenced by name in process::REGISTRY.

    pub(crate) fn process_gravity(&mut self, _time: f32) {
        self.gravitate_all();
    }

    pub(crate) fn process_integrate_gas(&mut self, time: f32) {
        self.apply_acceleration(time);
    }

    /// Rebuild the coarse acceleration field from gas + stars + the
    /// central black hole. Stars read this field (never pairwise forces),
    /// so the star population adds O(N), not O(N^2).
    pub(crate) fn process_gravity_field(&mut self, _time: f32) {
        let size_f = self.size as f32;
        let active_est = self.stars.len() + 64;
        let mut px: Vec<f32> = Vec::with_capacity(active_est);
        let mut py: Vec<f32> = Vec::with_capacity(active_est);
        let mut pm: Vec<f32> = Vec::with_capacity(active_est);
        for i in 0..self.n {
            if self.mass[i] != 0 {
                px.push(self.xs_i[i] as f32);
                py.push(self.ys_i[i] as f32);
                pm.push(self.mass[i] as f32);
            }
        }
        for i in 0..self.stars.len() {
            px.push(self.stars.pos_x[i].clamp(0.0, size_f - 1e-3));
            py.push(self.stars.pos_y[i].clamp(0.0, size_f - 1e-3));
            pm.push(self.stars.mass[i]);
        }
        if self.bh_mass > 0.0 {
            px.push(size_f * 0.5);
            py.push(size_f * 0.5);
            pm.push(self.bh_mass);
        }
        let tree = build_quadtree(&px, &py, &pm, 0.0, 0.0, size_f);
        let res = Galaxy::FIELD_RES;
        let cell = size_f / res as f32;
        let theta_sq = Galaxy::THETA * Galaxy::THETA;
        for fy in 0..res {
            for fx in 0..res {
                let wx = (fx as f32 + 0.5) * cell;
                let wy = (fy as f32 + 0.5) * cell;
                let (ax, ay) = tree.force(
                    wx,
                    wy,
                    theta_sq,
                    Galaxy::FIELD_SOFTENING_SQ,
                    Galaxy::GRAVATIONAL_CONSTANT,
                );
                self.field_ax[fy * res + fx] = ax;
                self.field_ay[fy * res + fx] = ay;
            }
        }
    }

    /// Bilinear sample of the coarse acceleration field at world (x, y).
    pub(crate) fn sample_field(&self, x: f32, y: f32) -> (f32, f32) {
        let res = Galaxy::FIELD_RES;
        let size_f = self.size as f32;
        let cell = size_f / res as f32;
        // Field values sit at cell centers; shift into field space.
        let fx = (x / cell - 0.5).clamp(0.0, (res - 1) as f32);
        let fy = (y / cell - 0.5).clamp(0.0, (res - 1) as f32);
        let x0 = fx as usize;
        let y0 = fy as usize;
        let x1 = (x0 + 1).min(res - 1);
        let y1 = (y0 + 1).min(res - 1);
        let tx = fx - x0 as f32;
        let ty = fy - y0 as f32;
        let idx = |xx: usize, yy: usize| yy * res + xx;
        let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let ax = lerp(
            lerp(self.field_ax[idx(x0, y0)], self.field_ax[idx(x1, y0)], tx),
            lerp(self.field_ax[idx(x0, y1)], self.field_ax[idx(x1, y1)], tx),
            ty,
        );
        let ay = lerp(
            lerp(self.field_ay[idx(x0, y0)], self.field_ay[idx(x1, y0)], tx),
            lerp(self.field_ay[idx(x0, y1)], self.field_ay[idx(x1, y1)], tx),
            ty,
        );
        (ax, ay)
    }

    /// Integrate the star population: field gravity + two-tier halo
    /// confinement, semi-implicit Euler, no movement cap (stars cannot
    /// jam). Positions may leave the grid into the halo band; field
    /// sampling clamps internally.
    pub(crate) fn process_integrate_stars(&mut self, time: f32) {
        let size_f = self.size as f32;
        let center = size_f * 0.5;
        let soft_r = self.disk_radius();
        let hard_r = soft_r * Galaxy::HARD_CLIP_FACTOR;
        let halo_drag = (-Galaxy::STAR_HALO_DRAG * time).exp();
        for i in 0..self.stars.len() {
            let px = self.stars.pos_x[i];
            let py = self.stars.pos_y[i];
            let (mut ax, mut ay) = self.sample_field(px, py);
            let dx = px - center;
            let dy = py - center;
            let r = (dx * dx + dy * dy).sqrt();
            let in_halo = r > soft_r && r > 1e-3;
            if in_halo {
                let grad = (Galaxy::HALO_STIFFNESS * (r - soft_r) / (hard_r - r).max(1e-3))
                    .min(Galaxy::HALO_ACCEL_MAX);
                ax -= grad * dx / r;
                ay -= grad * dy / r;
            }
            let mut vx = self.stars.vel_x[i] + ax * time;
            let mut vy = self.stars.vel_y[i] + ay * time;
            if in_halo {
                vx *= halo_drag;
                vy *= halo_drag;
            }
            self.stars.vel_x[i] = vx;
            self.stars.vel_y[i] = vy;
            let mut nx = px + vx * time;
            let mut ny = py + vy * time;
            // Numerical backstop just inside the hard clip - the gradient
            // makes this effectively unreachable.
            let hx = nx - center;
            let hy = ny - center;
            let hr = (hx * hx + hy * hy).sqrt();
            let max_r = hard_r - 1.0;
            if hr > max_r {
                nx = center + hx / hr * max_r;
                ny = center + hy / hr * max_r;
            }
            self.stars.pos_x[i] = nx;
            self.stars.pos_y[i] = ny;
        }
    }

    /// Deposit star luminosity into the coarse radiation field (3x3
    /// splat) and decay the whole field.
    pub(crate) fn process_radiation_field(&mut self, _time: f32) {
        let res = Galaxy::FIELD_RES;
        for r in self.radiation.iter_mut() {
            *r *= Galaxy::RAD_DECAY;
        }
        let size_f = self.size as f32;
        let cell = size_f / res as f32;
        for i in 0..self.stars.len() {
            if self.stars.stage[i] != crate::stars::Stage::MainSequence as u8 {
                continue;
            }
            let fx = (self.stars.pos_x[i] / cell) as usize;
            let fy = (self.stars.pos_y[i] / cell) as usize;
            let deposit = self.stars.luminosity[i] * Galaxy::RAD_DEPOSIT_SCALE;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let x = fx as i32 + dx;
                    let y = fy as i32 + dy;
                    if x < 0 || y < 0 || x >= res as i32 || y >= res as i32 {
                        continue;
                    }
                    let w = if dx == 0 && dy == 0 { 0.4 } else { 0.075 };
                    self.radiation[y as usize * res + x as usize] += deposit * w;
                }
            }
        }
    }

    /// Radiation level at a gas cell's position.
    fn radiation_at_cell(&self, cell: usize) -> f32 {
        let res = Galaxy::FIELD_RES;
        let scale = res as f32 / self.size as f32;
        let fx = ((self.xs_i[cell] as f32 * scale) as usize).min(res - 1);
        let fy = ((self.ys_i[cell] as f32 * scale) as usize).min(res - 1);
        self.radiation[fy * res + fx]
    }

    /// Scan for cells that have stayed dense, slow, and cool; sustained
    /// qualification plus an RNG roll emits CloudCollapse.
    pub(crate) fn process_collapse_watch(&mut self, _time: f32) {
        let density_floor =
            (Galaxy::CELL_MASS_CAP as f32 * Galaxy::COLLAPSE_DENSITY_FRACTION) as u16;
        let mut rng = self.rng_stream(Galaxy::RNG_COLLAPSE_WATCH);
        let tick = self.tick_count;
        for i in 0..self.n {
            let m = self.mass[i];
            let qualifies = m >= density_floor
                && self.radiation_at_cell(i) < Galaxy::COLLAPSE_RADIATION_RESIST;
            if !qualifies {
                self.collapse_heat[i] = 0;
                self.heat_parent[i] = 0;
                continue;
            }
            self.collapse_heat[i] = self.collapse_heat[i].saturating_add(1);
            if self.collapse_heat[i] >= Galaxy::COLLAPSE_HEAT_TRIGGER
                && rng.random_range(0.0f32..1.0) < Galaxy::COLLAPSE_CHANCE
            {
                self.collapse_heat[i] = 0;
                let parent = self.heat_parent[i];
                self.heat_parent[i] = 0;
                self.events.emit(
                    tick,
                    crate::events::EventKind::CloudCollapse,
                    i as u32,
                    i as u32,
                    0.0,
                    parent,
                );
            }
        }
    }

    /// Irradiated gas evaporates into the dissipated ledger.
    pub(crate) fn process_gas_dissipation(&mut self, _time: f32) {
        let tick = self.tick_count;
        let mut dissipate_events = 0u32;
        for i in 0..self.n {
            let m = self.mass[i];
            if m == 0 {
                continue;
            }
            if self.radiation_at_cell(i) < Galaxy::RAD_DISSIPATE_THRESHOLD {
                continue;
            }
            let lose = (m / 50).max(1).min(m);
            self.mass[i] = m - lose;
            self.dissipated_total += lose as u64;
            if self.mass[i] == 0 && dissipate_events < 32 {
                dissipate_events += 1;
                self.events.emit(
                    tick,
                    crate::events::EventKind::CloudDissipate,
                    i as u32,
                    i as u32,
                    lose as f32,
                    crate::events::NO_PARENT,
                );
            }
        }
    }

    /// Advance stellar ages by the sim time elapsed since the last run
    /// (dt x cadence, assuming dt is stable between runs - dt changes
    /// mid-run smear ages slightly, which is acceptable). Deaths: heavy
    /// main-sequence stars past their lifetime emit Supernova; light ones
    /// quietly fade to remnants.
    pub(crate) fn process_stellar_aging(&mut self, time: f32) {
        let elapsed = time * 8.0;
        let tick = self.tick_count;
        for i in 0..self.stars.len() {
            if self.stars.stage[i] != crate::stars::Stage::MainSequence as u8 {
                continue;
            }
            self.stars.age[i] += elapsed;
            if self.stars.age[i] < self.stars.lifetime[i] {
                continue;
            }
            if self.stars.mass[i] >= Galaxy::SN_MASS_THRESHOLD {
                // Target carries the nearest cell index so renderer
                // transients and the shock handler know where it happened
                // even after the star is gone.
                let cell = self.cell_index_at(self.stars.pos_x[i], self.stars.pos_y[i]);
                self.events.emit(
                    tick,
                    crate::events::EventKind::Supernova,
                    self.stars.id[i],
                    cell as u32,
                    self.stars.mass[i],
                    crate::events::NO_PARENT,
                );
                // Mark so it cannot re-emit while the event is in flight.
                self.stars.stage[i] = crate::stars::Stage::Remnant as u8;
            } else {
                self.stars.stage[i] = crate::stars::Stage::Remnant as u8;
                self.stars.luminosity[i] *= 0.05;
            }
        }
    }

    fn cell_index_at(&self, x: f32, y: f32) -> usize {
        let size = self.size as i32;
        let col = (x as i32).clamp(0, size - 1);
        let row = (y as i32).clamp(0, size - 1);
        (row * size + col) as usize
    }

    /// Supernova: return most of the star's mass to nearby gas with an
    /// outward kick, keep a dim compact remnant, and emit ShockWave.
    fn handle_supernova(&mut self, ev: &Event) {
        let Some(i) = self.stars.index_of_id(ev.source) else {
            return;
        };
        let cell = ev.target as usize;
        if cell >= self.n {
            return;
        }
        let star_mass = self.stars.mass[i];
        let ejected = star_mass * Galaxy::SN_GAS_RETURN;
        // Distribute ejecta over the disc around the cell with an
        // outward momentum kick, mass-weighted into cell velocity.
        let size = self.size as i32;
        let (c_col, c_row) = (cell as i32 % size, cell as i32 / size);
        let mut targets: Vec<usize> = Vec::new();
        for dr in -Galaxy::SN_RADIUS..=Galaxy::SN_RADIUS {
            for dc in -Galaxy::SN_RADIUS..=Galaxy::SN_RADIUS {
                if dc * dc + dr * dr > Galaxy::SN_RADIUS * Galaxy::SN_RADIUS {
                    continue;
                }
                let nc = wrap(c_col + dc, size) as u16;
                let nr = wrap(c_row + dr, size) as u16;
                targets.push(self.col_row_to_index(nc, nr) as usize);
            }
        }
        let share = (ejected / targets.len() as f32).max(0.0);
        let mut distributed = 0.0f32;
        for &t in &targets {
            let add = share as u16;
            if add == 0 {
                continue;
            }
            let old_m = self.mass[t] as f32;
            let (t_col, t_row) = (t as i32 % size, t as i32 / size);
            let mut dx = (t_col - c_col) as f32;
            let mut dy = (t_row - c_row) as f32;
            let r = (dx * dx + dy * dy).sqrt();
            if r < 1e-3 {
                dx = 0.0;
                dy = 0.0;
            } else {
                dx /= r;
                dy /= r;
            }
            let new_m = old_m + add as f32;
            self.vel_x[t] = (self.vel_x[t] * old_m + Galaxy::SN_KICK * dx * add as f32) / new_m;
            self.vel_y[t] = (self.vel_y[t] * old_m + Galaxy::SN_KICK * dy * add as f32) / new_m;
            self.mass[t] = self.mass[t].saturating_add(add);
            distributed += add as f32;
        }
        // Remnant keeps whatever the integer distribution left behind, so
        // the baryonic ledger stays closed exactly.
        self.stars.mass[i] = star_mass - distributed;
        self.stars.stage[i] = crate::stars::Stage::Remnant as u8;
        self.stars.luminosity[i] = (star_mass.powf(1.5)) * 0.02;
        let tick = self.tick_count;
        self.events.emit(
            tick,
            crate::events::EventKind::ShockWave,
            ev.source,
            ev.target,
            Galaxy::SN_KICK,
            ev.id,
        );
    }

    /// ShockWave: boost collapse heat around the blast so swept gas is
    /// likelier to collapse - and record parentage so induced collapses
    /// carry the causal chain.
    fn handle_shock_wave(&mut self, ev: &Event) {
        let cell = ev.target as usize;
        if cell >= self.n {
            return;
        }
        let size = self.size as i32;
        let (c_col, c_row) = (cell as i32 % size, cell as i32 / size);
        for dr in -Galaxy::SHOCK_RADIUS..=Galaxy::SHOCK_RADIUS {
            for dc in -Galaxy::SHOCK_RADIUS..=Galaxy::SHOCK_RADIUS {
                if dc * dc + dr * dr > Galaxy::SHOCK_RADIUS * Galaxy::SHOCK_RADIUS {
                    continue;
                }
                let nc = wrap(c_col + dc, size) as u16;
                let nr = wrap(c_row + dr, size) as u16;
                let ni = self.col_row_to_index(nc, nr) as usize;
                self.collapse_heat[ni] =
                    self.collapse_heat[ni].saturating_add(Galaxy::SHOCK_HEAT_BOOST);
                self.heat_parent[ni] = ev.id;
            }
        }
    }

    /// The black hole feeds: a fraction of nearby core gas accretes each
    /// run, and stars inside the capture radius are marked for capture
    /// (the swallow itself is a BlackHoleCapture event next tick).
    pub(crate) fn process_bh_accretion(&mut self, _time: f32) {
        if self.bh_mass <= 0.0 {
            return;
        }
        let size = self.size as i32;
        let c = size / 2;
        for dr in -Galaxy::BH_ACCRETION_RADIUS..=Galaxy::BH_ACCRETION_RADIUS {
            for dc in -Galaxy::BH_ACCRETION_RADIUS..=Galaxy::BH_ACCRETION_RADIUS {
                if dc * dc + dr * dr > Galaxy::BH_ACCRETION_RADIUS * Galaxy::BH_ACCRETION_RADIUS
                {
                    continue;
                }
                let i = self.col_row_to_index(
                    wrap(c + dc, size) as u16,
                    wrap(c + dr, size) as u16,
                ) as usize;
                let m = self.mass[i];
                if m == 0 {
                    continue;
                }
                let take = ((m as f32 * Galaxy::BH_ACCRETION_FRACTION) as u16).min(m);
                if take == 0 {
                    continue;
                }
                self.mass[i] -= take;
                self.bh_mass += take as f32;
            }
        }
        let center = self.size as f32 * 0.5;
        let cap_sq = Galaxy::BH_CAPTURE_RADIUS * Galaxy::BH_CAPTURE_RADIUS;
        let tick = self.tick_count;
        let speed_sq = Galaxy::BH_CAPTURE_MAX_SPEED * Galaxy::BH_CAPTURE_MAX_SPEED;
        for i in 0..self.stars.len() {
            let dx = self.stars.pos_x[i] - center;
            let dy = self.stars.pos_y[i] - center;
            let vx = self.stars.vel_x[i];
            let vy = self.stars.vel_y[i];
            if dx * dx + dy * dy <= cap_sq && vx * vx + vy * vy <= speed_sq {
                self.events.emit(
                    tick,
                    crate::events::EventKind::BlackHoleCapture,
                    self.stars.id[i],
                    crate::events::NO_REF,
                    self.stars.mass[i],
                    crate::events::NO_PARENT,
                );
            }
        }
    }

    /// Hawking evaporation: dM = -HAWKING_COEFF / M^2 per sim-time unit.
    /// Negligible while the hole is fat, runaway once it gets small -
    /// the shape of the real thing, at a wildly exaggerated rate. The
    /// radiated mass leaves the baryonic ledger through radiated_total
    /// and heats the core radiation field on its way out.
    pub(crate) fn process_bh_evaporation(&mut self, time: f32) {
        if self.bh_mass <= 0.0 {
            return;
        }
        let elapsed = time * 8.0;
        let loss = (Galaxy::HAWKING_COEFF / (self.bh_mass * self.bh_mass) * elapsed)
            .min(self.bh_mass);
        self.bh_mass -= loss;
        self.radiated_total += loss as f64;
        if self.bh_mass < 1.0 {
            // Final flash: the last scrap evaporates entirely.
            self.radiated_total += self.bh_mass as f64;
            self.bh_mass = 0.0;
        }
        let res = Galaxy::FIELD_RES;
        self.radiation[(res / 2) * res + res / 2] += loss * 0.5;
    }

    /// Swallow a captured star, re-checked loosely against the capture
    /// radius since a tick passed between emission and execution.
    fn handle_bh_capture(&mut self, ev: &Event) {
        let Some(i) = self.stars.index_of_id(ev.source) else {
            return;
        };
        let center = self.size as f32 * 0.5;
        let dx = self.stars.pos_x[i] - center;
        let dy = self.stars.pos_y[i] - center;
        let slack = Galaxy::BH_CAPTURE_RADIUS * 2.0;
        if dx * dx + dy * dy > slack * slack {
            return;
        }
        self.bh_mass += self.stars.mass[i];
        self.stars.swap_remove(i);
    }

    /// Execute this tick's due events in stable order. Handlers may emit
    /// follow-up events, which land next tick by construction.
    fn execute_events(&mut self, due: Vec<Event>, _time: f32) {
        for ev in due {
            match ev.kind {
                crate::events::EventKind::CloudCollapse => self.handle_cloud_collapse(&ev),
                crate::events::EventKind::StarBirth => self.handle_star_birth(&ev),
                crate::events::EventKind::Supernova => self.handle_supernova(&ev),
                crate::events::EventKind::ShockWave => self.handle_shock_wave(&ev),
                crate::events::EventKind::CloudDissipate => {}
                crate::events::EventKind::BlackHoleCapture => self.handle_bh_capture(&ev),
            }
            self.events.record_executed(ev);
        }
    }

    /// Consume gas around the collapsing cell into a birth budget carried
    /// on the follow-up StarBirth event. The budget mass is in flight for
    /// exactly one tick (see pending_birth_mass and the ledger test).
    fn handle_cloud_collapse(&mut self, ev: &Event) {
        let i = ev.source as usize;
        if i >= self.n {
            return;
        }
        let size = self.size as i32;
        let mut budget = 0.0f32;
        let take = |m: u16, frac: f32| -> u16 { (m as f32 * frac) as u16 };
        let own = take(self.mass[i], Galaxy::COLLAPSE_CONSUME_FRACTION);
        self.mass[i] -= own;
        budget += own as f32;
        let (col, row) = (i as i32 % size, i as i32 / size);
        for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nc = wrap(col + dc, size) as u16;
            let nr = wrap(row + dr, size) as u16;
            let ni = self.col_row_to_index(nc, nr) as usize;
            let part = take(self.mass[ni], Galaxy::COLLAPSE_CONSUME_FRACTION * 0.5);
            self.mass[ni] -= part;
            budget += part as f32;
        }
        if budget < Galaxy::BIRTH_MIN_BUDGET {
            // Fizzle: return the gas where it came from.
            self.mass[i] = self.mass[i].saturating_add(budget as u16);
            return;
        }
        let tick = self.tick_count;
        self.events.emit(
            tick,
            crate::events::EventKind::StarBirth,
            ev.source,
            ev.source,
            budget,
            ev.id,
        );
    }

    /// Spawn a cluster of stars from the budget, masses drawn from the
    /// IMF (mostly red dwarfs, occasionally a giant), leftover folded
    /// into the heaviest draw so the masses sum to the budget exactly
    /// and the baryonic ledger stays closed. Velocities = capped local
    /// gas velocity + prograde circular orbit component from the field.
    fn handle_star_birth(&mut self, ev: &Event) {
        let i = ev.target as usize;
        if i >= self.n {
            return;
        }
        let budget = ev.payload;
        let mut rng = self.rng_stream(Galaxy::RNG_STAR_BIRTH);
        // Draw IMF masses until the budget runs out.
        let mut masses: Vec<f32> = Vec::new();
        let mut remaining = budget;
        while remaining >= Galaxy::STAR_MASS_MIN && masses.len() < Galaxy::BIRTH_MAX_STARS {
            let m = Galaxy::imf_sample(rng.random_range(0.0f32..1.0)).min(remaining);
            remaining -= m;
            masses.push(m);
        }
        if masses.is_empty() {
            masses.push(budget);
            remaining = 0.0;
        }
        if remaining > 0.0 {
            // Fold the leftover into the heaviest star.
            let mut hi = 0;
            for (k, &m) in masses.iter().enumerate() {
                if m > masses[hi] {
                    hi = k;
                }
            }
            masses[hi] += remaining;
        }
        let n_stars = masses.len();
        let cluster = self.next_cluster_id;
        self.next_cluster_id += 1;

        let cx = self.xs_i[i] as f32;
        let cy = self.ys_i[i] as f32;
        let center = self.size as f32 * 0.5;
        let mut gas_vx = self.vel_x[i];
        let mut gas_vy = self.vel_y[i];
        let gas_speed = (gas_vx * gas_vx + gas_vy * gas_vy).sqrt();
        if gas_speed > Galaxy::BIRTH_GAS_VEL_CAP {
            let scale = Galaxy::BIRTH_GAS_VEL_CAP / gas_speed;
            gas_vx *= scale;
            gas_vy *= scale;
        }

        for k in 0..n_stars {
            let mass = masses[k];
            let px = (cx + rng.random_range(-1.8f32..1.8)).clamp(0.0, self.size as f32 - 1e-3);
            let py = (cy + rng.random_range(-1.8f32..1.8)).clamp(0.0, self.size as f32 - 1e-3);
            // Prograde circular support from the INWARD RADIAL component
            // of the field only - the raw magnitude is dominated by
            // whatever clump is nearest and mis-aims newborns.
            let rx = px - center;
            let ry = py - center;
            let r = (rx * rx + ry * ry).sqrt().max(1e-3);
            let (ax, ay) = self.sample_field(px, py);
            let a_rad = (-(ax * rx + ay * ry) / r).max(0.0);
            let v_circ = (a_rad * r).sqrt().min(Galaxy::BIRTH_VCIRC_CAP);
            let vx = gas_vx + (-ry / r) * v_circ;
            let vy = gas_vy + (rx / r) * v_circ;
            let (lifetime, luminosity, class_index) = Galaxy::star_attrs(mass);
            let star_id = self.next_star_id;
            self.next_star_id += 1;
            self.stars.spawn(
                px,
                py,
                vx,
                vy,
                mass,
                lifetime,
                luminosity,
                class_index,
                cluster,
                star_id,
            );
        }
    }

    /// Birth budgets currently in flight on pending StarBirth events.
    /// Part of the baryonic ledger between collapse and birth.
    pub(crate) fn pending_birth_mass(&self) -> f64 {
        self.events
            .pending()
            .filter(|e| e.kind == crate::events::EventKind::StarBirth)
            .map(|e| e.payload as f64)
            .sum()
    }

    /// Baryonic ledger: gas + stars + in-flight births + the black hole
    /// + the dissipated and radiated sinks.
    pub(crate) fn baryonic_total(&self) -> f64 {
        let gas: f64 = self.mass.iter().map(|&m| m as f64).sum();
        let stars: f64 = self.stars.mass.iter().map(|&m| m as f64).sum();
        gas + stars
            + self.pending_birth_mass()
            + self.dissipated_total as f64
            + self.bh_mass as f64
            + self.radiated_total
    }

    /// Stateless per-(process, tick) RNG stream. Derivation depends only
    /// on (master_seed, process_id, tick_count), so streams are
    /// independent per process, reproducible after a state round-trip,
    /// and adding a process never shifts another's draw sequence.
    pub(crate) fn rng_stream(&self, process_id: u64) -> StdRng {
        let mixed = splitmix64(
            self.master_seed
                ^ splitmix64(process_id ^ self.tick_count.wrapping_mul(0x9E37_79B9_7F4A_7C15)),
        );
        StdRng::seed_from_u64(mixed)
    }

    /// Derived main-sequence attributes: (lifetime, luminosity,
    /// class_index). class_index is log-mass normalized 0..1, M -> O;
    /// the renderer maps it through the stellar-classification colors.
    fn star_attrs(mass: f32) -> (f32, f32, f32) {
        let m = mass.max(1.0);
        let lifetime = Galaxy::STAR_LIFETIME_COEFF * (30.0 / m).powi(2);
        let luminosity = m.powf(2.0);
        let class_index = ((m / Galaxy::STAR_MASS_MIN).ln()
            / (Galaxy::STAR_MASS_MAX / Galaxy::STAR_MASS_MIN).ln())
        .clamp(0.0, 1.0);
        (lifetime, luminosity, class_index)
    }

    /// Inverse-transform sample of the truncated power-law IMF.
    fn imf_sample(u: f32) -> f32 {
        let a = 1.0 - Galaxy::IMF_ALPHA;
        let lo = Galaxy::STAR_MASS_MIN.powf(a);
        let hi = Galaxy::STAR_MASS_MAX.powf(a);
        (lo + u * (hi - lo)).powf(1.0 / a)
    }

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

/// splitmix64 finalizer - cheap, well-mixed u64 -> u64 for RNG stream
/// derivation.
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
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
mod tests_golden {
    use super::*;

    fn mass_hash(g: &Galaxy) -> u64 {
        let mut h: u64 = 1469598103934665603;
        for &m in g.mass.iter() {
            h ^= m as u64;
            h = h.wrapping_mul(1099511628211);
        }
        h
    }

    /// Golden values pin the mass field after 100 ticks (uniform seed 42
    /// / bang seed 7, size 50, dt 0.5). Last recaptured for the
    /// region-noise + spiral-density-wave seeding and the rotation
    /// boost (both modes share the boosted orbital step). If another
    /// deliberate change lands, recapture and say so in the commit.
    #[test]
    fn test_golden_uniform_mass_field() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Uniform, 42);
        for _ in 0..100 {
            g = g.tick(0.5);
        }
        assert_eq!(mass_hash(&g), 14143635165160636807);
    }

    #[test]
    fn test_golden_bang_mass_field() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Bang, 7);
        for _ in 0..100 {
            g = g.tick(0.5);
        }
        assert_eq!(mass_hash(&g), 200863505778242815);
    }

    #[test]
    fn test_rng_streams_are_reproducible_and_independent() {
        let g = Galaxy::new(10, 0).seed_with_mode_seeded(5, InitialCondition::Uniform, 99);
        let mut a1 = g.rng_stream(1);
        let mut a2 = g.rng_stream(1);
        let mut b = g.rng_stream(2);
        let draws1: Vec<u32> = (0..8).map(|_| a1.random()).collect();
        let draws2: Vec<u32> = (0..8).map(|_| a2.random()).collect();
        let draws_b: Vec<u32> = (0..8).map(|_| b.random()).collect();
        assert_eq!(draws1, draws2, "same (seed, process, tick) must repeat");
        assert_ne!(draws1, draws_b, "different processes must not share a stream");
        let g2 = g.tick(0.5);
        let mut a_next = g2.rng_stream(1);
        let draws_next: Vec<u32> = (0..8).map(|_| a_next.random()).collect();
        assert_ne!(draws1, draws_next, "streams must advance across ticks");
    }
}

#[cfg(test)]
mod tests_stars_dynamics {
    use super::*;

    #[test]
    fn test_star_at_rest_falls_toward_the_disk_center() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Uniform, 42);
        g.spawn_star(35.0, 25.0, 0.0, 0.0, 10.0);
        let r0 = (g.stars.pos_x[0] - 25.0).hypot(g.stars.pos_y[0] - 25.0);
        for _ in 0..40 {
            g = g.tick(0.5);
        }
        let r1 = (g.stars.pos_x[0] - 25.0).hypot(g.stars.pos_y[0] - 25.0);
        assert!(
            r1 < r0,
            "field gravity must pull a resting star inward: r {r0:.2} -> {r1:.2}"
        );
    }

    #[test]
    fn test_ejected_star_stays_inside_hard_clip_and_rejoins_disk() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Uniform, 42);
        g.spawn_star(25.0, 25.0, 6.0, 0.0, 10.0);
        let soft = 24.0f32;
        let hard = soft * Galaxy::HARD_CLIP_FACTOR;
        let mut max_r = 0.0f32;
        let mut g = g;
        for _ in 0..4000 {
            g = g.tick(0.5);
            if g.star_count() == 0 {
                break;
            }
            let r = (g.stars.pos_x[0] - 25.0).hypot(g.stars.pos_y[0] - 25.0);
            if r > max_r {
                max_r = r;
            }
        }
        assert!(
            max_r < hard,
            "halo gradient must stop ejecta before the hard clip: max r {max_r:.1}"
        );
        assert!(
            max_r > soft,
            "test must actually exercise the halo band: max r {max_r:.1}"
        );
        // Halo drag decays the excursion into a skim orbit at the disk
        // edge - "returned" means out of the deep halo, not parked at a
        // fixed rim like the old hard-stop.
        let r_final = (g.stars.pos_x[0] - 25.0).hypot(g.stars.pos_y[0] - 25.0);
        assert!(
            r_final < soft * 1.2,
            "halo drag must decay ejecta back to the disk edge: final r {r_final:.1}"
        );
    }

    #[test]
    fn test_star_render_data_shape() {
        let mut g = Galaxy::new(20, 0).seed_with_mode_seeded(5, InitialCondition::Uniform, 1);
        g.spawn_star(10.0, 10.0, 0.1, 0.0, 42.0);
        g.spawn_star(5.0, 5.0, 0.0, 0.1, 7.0);
        assert_eq!(g.star_count(), 2);
        let rd = g.star_render_data();
        assert_eq!(rd.len(), 2 * crate::stars::RENDER_FLOATS);
        assert_eq!(rd[0], 10.0);
        assert!(rd[2] > rd[6], "heavier star must be more luminous");
    }

    #[test]
    fn test_sim_state_round_trip_preserves_star_evolution() {
        // The worker boundary contract: exporting gas + star + meta state
        // and rehydrating must continue the exact same trajectory.
        let mut a = Galaxy::new(30, 0).seed_with_mode_seeded(10, InitialCondition::Uniform, 9);
        a.spawn_star(20.0, 15.0, 0.0, 0.4, 30.0);
        a.spawn_star(10.0, 15.0, 0.0, -0.4, 60.0);
        for _ in 0..5 {
            a = a.tick(0.5);
        }
        let mut b = Galaxy::from_state(
            30,
            a.mass(),
            a.vel_x(),
            a.vel_y(),
            a.frac_x(),
            a.frac_y(),
        );
        b.restore_sim_state_stars(&a.sim_state_stars());
        b.restore_sim_state_field(&a.sim_state_field());
        b.restore_sim_state_meta(&a.sim_state_meta());
        for _ in 0..20 {
            a = a.tick(0.5);
            b = b.tick(0.5);
        }
        assert_eq!(a.stars.pos_x, b.stars.pos_x, "star x trajectories must match");
        assert_eq!(a.stars.pos_y, b.stars.pos_y, "star y trajectories must match");
        assert_eq!(a.mass, b.mass, "gas must match");
        assert_eq!(a.tick_count, b.tick_count);
    }
}

#[cfg(test)]
mod tests_causal_loop {
    use super::*;
    use crate::events::EventKind;

    #[test]
    fn test_stars_form_unattended_from_cold_gas() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Uniform, 42);
        let mut formed_at = None;
        for t in 0..4000 {
            g = g.tick(0.5);
            if g.star_count() > 0 {
                formed_at = Some(t);
                break;
            }
        }
        assert!(
            formed_at.is_some(),
            "cold uniform gas must form stars unattended within 4000 ticks"
        );
        assert!(g.events.executed_count(EventKind::CloudCollapse) > 0);
        assert!(g.events.executed_count(EventKind::StarBirth) > 0);
    }

    #[test]
    fn test_baryonic_ledger_is_conserved_through_star_formation() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Uniform, 42);
        let initial = g.baryonic_total();
        for _ in 0..3000 {
            g = g.tick(0.5);
            let now = g.baryonic_total();
            assert!(
                (now - initial).abs() < 1.0,
                "ledger drifted at tick {}: {initial} -> {now}",
                g.tick_count
            );
        }
        assert!(g.star_count() > 0, "run must include actual star formation");
        assert!(
            g.events.executed_count(EventKind::Supernova) > 0,
            "3000 ticks must include supernovae so the ledger covers ejecta"
        );
    }

    #[test]
    fn test_determinism_same_seed_same_trajectory_at_two_depths() {
        // Same seed + same dt sequence -> identical star arrays and
        // event log, checked at two depths to catch cadence-boundary
        // nondeterminism. Both depths reach into the star-formation era.
        fn run(n: usize) -> (Vec<f32>, Vec<f32>, u64, [u64; 5]) {
            let mut g =
                Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Uniform, 42);
            for _ in 0..n {
                g = g.tick(0.5);
            }
            let counts = [
                g.events.executed_count(EventKind::CloudCollapse),
                g.events.executed_count(EventKind::StarBirth),
                g.events.executed_count(EventKind::Supernova),
                g.events.executed_count(EventKind::ShockWave),
                g.events.executed_count(EventKind::CloudDissipate),
            ];
            (
                g.stars.pos_x.clone(),
                g.stars.vel_y.clone(),
                g.stars.len() as u64,
                counts,
            )
        }
        for n in [900usize, 1800] {
            let a = run(n);
            let b = run(n);
            assert_eq!(a.2, b.2, "star count must be deterministic at n={n}");
            assert!(a.2 > 0, "depth n={n} must include star formation");
            assert_eq!(a.0, b.0, "star positions must be deterministic at n={n}");
            assert_eq!(a.1, b.1, "star velocities must be deterministic at n={n}");
            assert_eq!(a.3, b.3, "event log must be deterministic at n={n}");
        }
    }

    #[test]
    fn test_full_causal_chain_supernova_induces_star_birth() {
        // The loop's acceptance scenario: a StarBirth whose ancestry runs
        // birth -> CloudCollapse -> ShockWave -> Supernova -> root.
        use std::collections::HashMap;
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Uniform, 42);
        // (id -> (kind, parent)) log collected from the bounded ring
        // every tick so eviction cannot lose links.
        let mut log: HashMap<u64, (EventKind, u64)> = HashMap::new();
        let mut found = false;
        for _ in 0..8000 {
            g = g.tick(0.5);
            for ev in g.events.recent() {
                log.insert(ev.id, (ev.kind, ev.parent));
            }
            found = log.iter().any(|(_, &(kind, parent))| {
                kind == EventKind::StarBirth
                    && matches!(log.get(&parent), Some(&(EventKind::CloudCollapse, gp))
                        if matches!(log.get(&gp), Some(&(EventKind::ShockWave, ggp))
                            if matches!(log.get(&ggp), Some(&(EventKind::Supernova, _)))))
            });
            if found {
                break;
            }
        }
        assert!(
            found,
            "no supernova-induced star birth chain within 8000 ticks              (events: col={} birth={} sn={} shock={})",
            g.events.executed_count(EventKind::CloudCollapse),
            g.events.executed_count(EventKind::StarBirth),
            g.events.executed_count(EventKind::Supernova),
            g.events.executed_count(EventKind::ShockWave),
        );
    }
}

#[cfg(test)]
mod tests_black_hole {
    use super::*;
    use crate::events::EventKind;

    #[test]
    fn test_capture_swallows_a_central_star_and_ledger_holds() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, InitialCondition::Uniform, 42);
        let initial = g.baryonic_total();
        let bh0 = g.bh_mass_value();
        g.spawn_star(25.2, 25.2, 0.0, 0.0, 40.0);
        for _ in 0..40 {
            g = g.tick(0.5);
        }
        assert_eq!(g.star_count(), 0, "central star must be swallowed");
        assert!(g.events.executed_count(EventKind::BlackHoleCapture) > 0);
        assert!(g.bh_mass_value() > bh0, "the hole must grow from the meal");
        assert!(
            (g.baryonic_total() - (initial + 40.0)).abs() < 1.0,
            "ledger must account the swallowed star"
        );
    }

    #[test]
    fn test_hawking_evaporation_runs_away_for_a_small_hole() {
        // Small seeded mass -> small hole. dM/dt = -H/M^2 barely leaks
        // while fat and runs away once small - it must fully evaporate,
        // land in the radiated sink, and take the lens with it.
        let mut g = Galaxy::new(20, 0).seed_with_mode_seeded(2, InitialCondition::Uniform, 7);
        let bh0 = g.bh_mass_value();
        assert!(bh0 > 0.0 && bh0 < 100.0, "test wants a small hole, got {bh0}");
        let initial = g.baryonic_total();
        for _ in 0..2000 {
            g = g.tick(0.5);
            if g.bh_mass_value() == 0.0 {
                break;
            }
        }
        assert_eq!(g.bh_mass_value(), 0.0, "small hole must evaporate away");
        assert_eq!(g.bh_lens_scale(), 0.0, "no hole, no lens");
        assert!(
            (g.baryonic_total() - initial).abs() < 2.0,
            "radiated sink must close the ledger"
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

//! Galaxy simulation. See docs/galaxy-rust.md.

use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use wasm_bindgen::prelude::*;

use crate::events::{Event, EventQueue};
use crate::process;
use crate::stars::{Stage, Stars, NO_BINARY, NO_CLUSTER};

/// Scenario presets: a hardcoded `start => end-shape` pair. The name is
/// the promise - "bang => ring" seeds a central explosion whose physics
/// parameters are tuned so the gas vaguely resembles a ring at t ~= 1000
/// for most seeds. See `seed_with_mode` and `ScenarioParams`.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scenario {
    /// Central explosion; ejecta circularize near their turnaround
    /// radius into a rotating ring with a hollow core.
    BangRing = 0,
    /// Central explosion with a two-lobed ejection; a rotating density
    /// wave gathers its cooling gas into sustained spiral arms.
    BangSpiral = 1,
    /// Clumpy smoke-noise disk that settles into a rotating, star-forming
    /// two-arm density wave.
    IrregularSpiral = 2,
    /// Clumpy smoke-noise cloud with weak rotation and high dispersion
    /// that relaxes into a smooth centrally concentrated spheroid.
    IrregularElliptical = 3,
}

/// Per-scenario physics targets. The start half of a scenario picks the
/// seeder (`bang`); the end half is carried by the rotation curve and
/// relaxation constants that steer 1000 ticks of evolution. Values are
/// hardcoded per variant - noise only textures a scenario, so the end
/// shape is sturdy across seeds.
pub struct ScenarioParams {
    /// Bang core seeder (true) vs irregular smoke seeder (false).
    pub bang: bool,
    /// Flat-rotation-curve speed of the static halo potential. The halo
    /// stands in for dark matter: gas self-gravity alone cannot hold a
    /// flat curve, and without one the disk either freezes or falls in.
    pub v_flat: f32,
    /// Rotation-curve turnover radius as a fraction of the disk radius:
    /// v_c(r) = v_flat * r / sqrt(r^2 + rc^2).
    pub halo_core_frac: f32,
    /// Relaxation rate of gas velocity toward the local circular flow.
    /// This replaces plain drag: dissipation circularizes instead of
    /// stopping, which is what keeps the big clouds rotating at t=1000.
    pub flow_drag: f32,
    /// Fraction of v_c the flow target carries. 1.0 holds orbits where
    /// they are; below 1.0 the gas is chronically under-supported and
    /// inspirals while rotating - the elliptical's concentration knob.
    pub flow_support: f32,
    /// Seed-time spin multiplier on the self-gravity circular velocity.
    pub rotation_boost: f32,
    /// Bang: ejection speed as a multiple of the speed needed to climb
    /// from the core to `eject_target_frac` against self-gravity AND the
    /// halo potential (the halo well is deep - naive v_esc stalls).
    pub eject_factor: f32,
    /// Bang: intended turnaround radius as a fraction of disk_r. The
    /// flow drag then circularizes ejecta near it - the ring-radius knob.
    pub eject_target_frac: f32,
    /// Bang: core radius as a fraction of world size.
    pub core_radius_frac: f32,
    /// Bang: per-cell core fill as a multiple of the seed-mass knob.
    /// Deliberately UNDER the collapse density threshold: a dense core
    /// converts to stars before its ejecta travel anywhere, so the bang
    /// seeds a wide thin core whose cells only reach star-forming
    /// density where they pile up at the target radius.
    pub core_fill_scale: f32,
    /// Bang: m=2 azimuthal modulation depth on the ejection speed - the
    /// two fast lobes become the spiral arms.
    pub eject_lobes: f32,
    /// Bang: tangential tilt of the ejection direction, radians from
    /// radial. Direction is immune to the per-axis movement clamp, so
    /// this curls arms even while speed is capped - the spiral's
    /// signature. Also smears the diagonal grid artifact.
    pub eject_swirl: f32,
    /// Irregular: amplitude of the seeded two-arm density wave.
    pub spiral_amp: f32,
    /// Ongoing logarithmic density-wave potential strength. Gas responds
    /// to the wave; collisionless stars do not.
    pub spiral_wave_strength: f32,
    /// Isothermal pressure response to neighboring gas-density gradients
    /// in scenarios that sustain resolved gas structures.
    pub gas_pressure: f32,
    /// Cold-gas transport rate down the arm potential. The conservative
    /// drift models gas cooling into the compression lane.
    pub spiral_arm_transport: f32,
    /// Phase advance per tick in m-theta space. Dividing by two gives the
    /// visible two-arm pattern speed.
    pub spiral_pattern_step: f32,
    /// Axisymmetric radial restoring force for a sustained gas ring.
    pub ring_wave_strength: f32,
    /// Radius of the annular potential minimum as a fraction of disk_r.
    pub ring_radius_frac: f32,
    /// Conservative cold-gas transport rate toward the annular minimum.
    pub ring_transport: f32,
    /// Scenario density threshold for sustained cloud collapse, expressed
    /// as a fraction of the per-cell mass cap.
    pub collapse_density_fraction: f32,
    /// Per-scan collapse probability after the sustained-density trigger.
    pub collapse_chance: f32,
    /// Irregular: power-law contrast of the smoke field (clumpiness).
    pub smoke_contrast: f32,
    /// Irregular: exponential radial density envelope scale as a
    /// fraction of disk_r; 0 = flat disk. The elliptical seeds its
    /// central concentration here - real ellipticals are light-profile
    /// concentrated, not dynamically collapsed on this timescale.
    pub radial_scale_frac: f32,
    /// Irregular: seeder mass multiplier - rebalances scenarios whose
    /// envelope would otherwise seed a dim galaxy.
    pub seed_gain: f32,
    /// Isotropic velocity jitter at seed time (pressure support for the
    /// elliptical - it puffs the cloud instead of letting it pancake).
    pub vel_dispersion: f32,
    /// In-disk star velocity drag. Stars are collisionless, so a young
    /// swarm slowly evaporates outward; the elliptical uses a whisper of
    /// drag to settle its swarm into the central glow instead. Zero for
    /// disk scenarios - their stars must keep orbiting forever.
    pub star_drag: f32,
}

impl Scenario {
    pub fn params(self) -> ScenarioParams {
        match self {
            Scenario::BangRing => ScenarioParams {
                bang: true,
                v_flat: 1.4,
                halo_core_frac: 0.3,
                flow_drag: 0.03,
                flow_support: 1.02,
                rotation_boost: 0.8,
                eject_factor: 1.45,
                eject_target_frac: 0.62,
                core_radius_frac: 0.24,
                core_fill_scale: 3.0,
                eject_lobes: 0.0,
                eject_swirl: 0.15,
                spiral_amp: 0.0,
                spiral_wave_strength: 0.0,
                gas_pressure: 0.18,
                spiral_arm_transport: 0.0,
                spiral_pattern_step: 0.0,
                ring_wave_strength: 0.1,
                ring_radius_frac: 0.58,
                ring_transport: 0.08,
                collapse_density_fraction: 0.24,
                collapse_chance: 0.08,
                smoke_contrast: 1.8,
                radial_scale_frac: 0.0,
                seed_gain: 1.0,
                vel_dispersion: 0.0,
                star_drag: 0.0,
            },
            Scenario::BangSpiral => ScenarioParams {
                bang: true,
                v_flat: 1.4,
                halo_core_frac: 0.2,
                flow_drag: 0.02,
                flow_support: 1.05,
                rotation_boost: 1.2,
                eject_factor: 1.45,
                eject_target_frac: 0.7,
                core_radius_frac: 0.2,
                core_fill_scale: 3.0,
                eject_lobes: 0.35,
                eject_swirl: 0.6,
                spiral_amp: 0.0,
                spiral_wave_strength: 0.65,
                gas_pressure: 0.4,
                spiral_arm_transport: 0.12,
                spiral_pattern_step: 0.1,
                ring_wave_strength: 0.0,
                ring_radius_frac: 0.0,
                ring_transport: 0.0,
                collapse_density_fraction: 0.75,
                collapse_chance: 0.35,
                smoke_contrast: 1.8,
                radial_scale_frac: 0.0,
                seed_gain: 1.0,
                vel_dispersion: 0.0,
                star_drag: 0.0,
            },
            Scenario::IrregularSpiral => ScenarioParams {
                bang: false,
                v_flat: 1.4,
                halo_core_frac: 0.2,
                flow_drag: 0.015,
                flow_support: 1.0,
                rotation_boost: 1.1,
                eject_factor: 0.0,
                eject_target_frac: 0.0,
                core_radius_frac: 0.0,
                core_fill_scale: 0.0,
                eject_lobes: 0.0,
                eject_swirl: 0.0,
                spiral_amp: 0.7,
                spiral_wave_strength: 0.7,
                gas_pressure: 0.5,
                spiral_arm_transport: 0.14,
                spiral_pattern_step: 0.1,
                ring_wave_strength: 0.0,
                ring_radius_frac: 0.0,
                ring_transport: 0.0,
                collapse_density_fraction: 0.75,
                collapse_chance: 0.35,
                smoke_contrast: 1.8,
                radial_scale_frac: 0.0,
                seed_gain: 1.5,
                vel_dispersion: 0.0,
                star_drag: 0.0,
            },
            Scenario::IrregularElliptical => ScenarioParams {
                bang: false,
                v_flat: 1.0,
                halo_core_frac: 0.15,
                flow_drag: 0.035,
                flow_support: 0.85,
                rotation_boost: 0.7,
                eject_factor: 0.0,
                eject_target_frac: 0.0,
                core_radius_frac: 0.0,
                core_fill_scale: 0.0,
                eject_lobes: 0.0,
                eject_swirl: 0.0,
                spiral_amp: 0.0,
                spiral_wave_strength: 0.0,
                gas_pressure: 0.35,
                spiral_arm_transport: 0.0,
                spiral_pattern_step: 0.0,
                ring_wave_strength: 0.0,
                ring_radius_frac: 0.0,
                ring_transport: 0.0,
                collapse_density_fraction: 0.4,
                collapse_chance: 0.2,
                smoke_contrast: 0.9,
                radial_scale_frac: 0.28,
                seed_gain: 1.5,
                vel_dispersion: 0.5,
                star_drag: 0.0015,
            },
        }
    }

    pub fn from_u32(v: u32) -> Scenario {
        match v {
            0 => Scenario::BangRing,
            1 => Scenario::BangSpiral,
            3 => Scenario::IrregularElliptical,
            _ => Scenario::IrregularSpiral,
        }
    }
}

/// Per-tick reduction of one stellar association. The authoritative state
/// remains on each star through `cluster_id`, so worker snapshots need no
/// second cluster object graph. Rebuilding this compact view keeps removal
/// and phase mixing compatible with the star SoA's swap-remove semantics.
#[derive(Clone, Copy, Default)]
struct AssociationAggregate {
    mass: f32,
    weighted_x: f32,
    weighted_y: f32,
    weighted_vx: f32,
    weighted_vy: f32,
    oldest_age: f32,
    members: u32,
}

impl AssociationAggregate {
    fn center(self) -> (f32, f32) {
        if self.mass <= 0.0 {
            return (0.0, 0.0);
        }
        (self.weighted_x / self.mass, self.weighted_y / self.mass)
    }

    fn velocity(self) -> (f32, f32) {
        if self.mass <= 0.0 {
            return (0.0, 0.0);
        }
        (self.weighted_vx / self.mass, self.weighted_vy / self.mass)
    }
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
    /// Heavy-element mass in each cold-gas cell. Always bounded by mass.
    metal_mass: Vec<f32>,
    scratch_metal_mass: Vec<f32>,

    /// Ticks elapsed since seeding. Drives process cadence and the
    /// per-tick RNG stream derivation.
    tick_count: u64,
    /// Master seed for the RNG service. 0 until seeded.
    master_seed: u64,
    events: EventQueue,

    pub(crate) stars: Stars,
    /// Central black hole point mass, set at seed time and live
    /// thereafter: it grows by accreting core gas and capturing stars,
    /// and shrinks by (exaggerated) Hawking evaporation. It participates
    /// in both the stellar field and the shared gas integration step.
    bh_mass: f32,
    /// Seed-time black hole mass; the renderer scales the lens depth by
    /// sqrt(bh_mass / bh_mass_initial).
    bh_mass_initial: f32,
    /// Mass lost to Hawking radiation - the irreversible ledger sink.
    radiated_total: f64,
    /// Heavy elements carried into the central black hole.
    bh_metal_mass: f64,
    /// Heavy elements carried out in the radiated sink.
    radiated_metal_mass: f64,
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
    /// Hot gas lifted out of the visible disk by feedback. The galactic
    /// fountain cools it back, making this a reservoir rather than a sink.
    halo_gas_mass: u64,
    /// Heavy elements carried by the hot circumgalactic gas.
    halo_metal_mass: f64,
    /// Resolved stars and compact remnants phase-mixed into a diffuse,
    /// unresolved stellar halo. It remains in the baryonic ledger.
    stellar_halo_mass: f64,
    /// Heavy elements retired with unresolved stellar-halo mass.
    stellar_halo_metal_mass: f64,
    /// New heavy elements synthesized by stellar feedback.
    metal_produced_total: f64,
    /// Number of resolved particles retired into `stellar_halo_mass`.
    phase_mixed_count: u64,
    next_cluster_id: u32,
    next_star_id: u32,
    next_binary_id: u32,
    /// Causal attribution for shock-boosted collapse heat: the ShockWave
    /// event id that last boosted each cell, 0 = organic. Lets an induced
    /// CloudCollapse carry its true parent.
    heat_parent: Vec<u64>,
    /// Active scenario - fixes the halo rotation curve and flow-drag
    /// constants every tick, not just at seed time.
    scenario: Scenario,
}

impl Galaxy {
    // See docs/galaxy-rust.md for constant rationale.
    pub const GRAVATIONAL_CONSTANT: f32 = 5.0e-4;
    const SOFTENING_SQ: f32 = 1.0;
    /// 1.0 = one cell per tick, the transfer scheme's hard ceiling
    /// (one cell-hop per axis per tick). Doubled from 0.5 so the gas
    /// visibly flows - the rotation curve runs right at this cap.
    const MAX_SUBGRID_STEP: f32 = 1.0;
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
    /// every tick, so any per-block bleed spins it down fast. The
    /// per-scenario flow relaxation is the energy sink instead.
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
    /// Stars orbit at half the gas pace: the coarse field they read is
    /// built at quarter strength (v ~ sqrt(a r)), with its halo term
    /// using the pre-doubled gas curve. Fast pink rivers of gas around
    /// a slow drifting star population is the intended look.
    const STAR_FIELD_SCALE: f32 = 0.25;
    /// Softening for the coarse field build (separate from the gas
    /// kernel's SOFTENING_SQ). Large on purpose: the star field is a
    /// mean field. With point-scale softening, stars dive through steep
    /// cluster wells sampled from a 4-tick-stale field and the
    /// integration error pumps orbital energy until the disk evaporates
    /// into the halo.
    const FIELD_SOFTENING_SQ: f32 = 25.0;
    /// Central black hole mass as a fraction of total seeded mass.
    const BH_MASS_FRACTION: f32 = 0.05;
    /// Primordial gas begins weakly enriched so dust remains visible before
    /// the first stellar generation completes.
    const INITIAL_METALLICITY: f32 = 0.004;
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
    /// Radius beyond which a sustained orbit becomes unresolved halo light.
    const STELLAR_HALO_MIX_RADIUS: f32 = 1.18;
    /// Eight cadence scans = 64 ticks outside the luminous disk.
    const STELLAR_HALO_DWELL: u16 = 8;
    /// Dim remnants retire from the point population after this sim age.
    const REMNANT_RESOLVED_AGE: f32 = 360.0;
    /// Unpaired neutron stars stay resolved longer than ordinary remnants.
    const NEUTRON_STAR_RESOLVED_AGE: f32 = 1_200.0;
    /// Brief post-main-sequence expansion before envelope loss.
    const RED_GIANT_LIFETIME: f32 = 160.0;
    /// Single white dwarfs remain visible before joining the stellar halo.
    const WHITE_DWARF_RESOLVED_AGE: f32 = 1_200.0;

    // Uniform-seed structure: domain-warped fractal noise shaped into
    // smoke. Four octaves of smoothstep value noise (fBm) give the
    // power-law texture, a second fBm field warps the sampling
    // coordinates so blobs curl into wisps and billows, and a power-law
    // contrast exponent carves thin filaments and true voids out of
    // what would otherwise read as smooth milk. A two-arm
    // logarithmic-spiral overdensity still seeds the density wave that
    // differential rotation shears into a pinwheel; ROTATION_BOOST
    // spins the disk slightly super-circular so the shear works.
    pub(crate) const SMOKE_OCTAVE_RES: [usize; 4] = [6, 12, 24, 48];
    const SMOKE_WARP: f32 = 0.11;
    /// Normalized fBm clusters tightly around its mean - stretch it
    /// (about a slightly dark center) so voids reach true zero and
    /// billows saturate, THEN shape with the power law.
    const SMOKE_STRETCH: f32 = 3.2;
    const SMOKE_CENTER: f32 = 0.44;
    const SMOKE_GAIN: f32 = 2.6;
    /// Radial fraction where the seed density starts feathering toward
    /// zero at the disk rim - no cookie-cutter edge on the clouds.
    const EDGE_FEATHER_START: f32 = 0.55;
    const SPIRAL_AMP: f32 = 0.55;
    const SPIRAL_PITCH: f32 = 4.0;

    // Cloud-collapse tuning. A cell must stay above its scenario-owned
    // density fraction and below the radiation resist level for
    // COLLAPSE_HEAT_TRIGGER consecutive scans (collapse_watch cadence 16)
    // before it can roll for collapse. No velocity gate: jammed cells
    // accumulate large STORED velocity while standing still, so a speed
    // limit anti-selects exactly the proto-cluster cells.
    const COLLAPSE_HEAT_TRIGGER: u8 = 6;
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

    // Stellar associations. Nearby collapse events belong to the same
    // star-forming complex, receive one center-of-mass orbit, and retain a
    // temporary softened binding potential. The potential fades with age
    // and the local galactic tide releases exterior members into streams.
    const ASSOCIATION_JOIN_RADIUS: f32 = 3.2;
    const ASSOCIATION_JOIN_MAX_AGE: f32 = 32.0;
    const ASSOCIATION_BIRTH_RADIUS: f32 = 0.9;
    const ASSOCIATION_RADIAL_INHERITANCE: f32 = 0.12;
    const ASSOCIATION_ORBIT_SUPPORT: f32 = 1.08;
    const ASSOCIATION_HALO_ORBIT_FLOOR: f32 = 0.88;
    // Retain most of the pre-association gas-plus-circular-support envelope
    // while leaving headroom below the high-speed halo escape regime.
    const ASSOCIATION_ORBIT_SPEED_CAP: f32 = 2.1;
    const ASSOCIATION_EXISTING_VELOCITY_WEIGHT: f32 = 0.70;
    const ASSOCIATION_INTERNAL_SPEED_SCALE: f32 = 0.42;
    const ASSOCIATION_INTERNAL_SPEED_CAP: f32 = 0.24;
    const ASSOCIATION_BINDING_G: f32 = 7.5e-4;
    const ASSOCIATION_BINDING_SOFTENING_SQ: f32 = 0.64;
    const ASSOCIATION_ACCEL_MAX: f32 = 0.08;
    const ASSOCIATION_BINDING_LIFETIME: f32 = 620.0;
    const ASSOCIATION_TIDAL_GRACE: f32 = 56.0;
    const ASSOCIATION_TIDAL_RADIUS_MIN: f32 = 1.4;
    const ASSOCIATION_TIDAL_RADIUS_MAX: f32 = 6.5;
    const ASSOCIATION_MIN_MEMBERS: u32 = 3;

    // Radiation tuning. Deposits scale luminosity into the coarse field
    // with a 3x3 splat; the field decays every rebuild.
    const RAD_DEPOSIT_SCALE: f32 = 0.01;
    const RAD_DECAY: f32 = 0.85;
    /// Above this radiation level gas rises into the hot halo,
    /// emitting CloudDissipate when a cell empties.
    const RAD_DISSIPATE_THRESHOLD: f32 = 60.0;

    // Observable closed fountain cycle, standing in for delayed feedback
    // and cooling without adding baryons. See docs/galaxy-rust.md.
    const FOUNTAIN_PERIOD: u64 = 480;
    const FOUNTAIN_COLD_MIDPOINT: f32 = 0.50;
    const FOUNTAIN_COLD_AMPLITUDE: f32 = 0.10;
    /// Maximum share of active gas exchanged per fountain cadence.
    const FOUNTAIN_MAX_EXCHANGE: f32 = 0.02;
    /// Cooled halo gas rains back in small parcels, avoiding new square
    /// mega-clouds while pressure overflow connects neighboring cells.
    const FOUNTAIN_PACKET_MASS: u16 = 4;

    // Supernova tuning. Main-sequence stars past their lifetime and at
    // or above the mass threshold detonate; lighter ones fade to
    // remnants. A supernova returns most of the star's mass to nearby
    // gas with an outward kick and leaves a neutron star.
    const SN_MASS_THRESHOLD: f32 = 30.0;
    const SN_GAS_RETURN: f32 = 0.8;
    /// Newly synthesized heavy elements as a share of progenitor mass.
    const SN_METAL_YIELD_FRACTION: f32 = 0.02;
    const SN_KICK: f32 = 1.2;
    const SN_RADIUS: i32 = 2;
    /// Intermediate-mass birth draws split into delayed white-dwarf pairs.
    const WD_BINARY_SPLIT_MASS: f32 = 12.0;
    const WD_RETAINED_FRACTION: f32 = 0.2;
    const WD_MERGER_DELAY_MIN: f32 = 240.0;
    const WD_MERGER_DELAY_SPAN: f32 = 640.0;
    const WD_LUMINOSITY: f32 = 2.0;
    const PLANETARY_NEBULA_RADIUS: i32 = 2;
    const PLANETARY_NEBULA_KICK: f32 = 0.22;
    const TYPE_IA_RADIUS: i32 = 3;
    const TYPE_IA_KICK: f32 = 1.55;
    /// Thermonuclear burning converts a large share of the binary to metals.
    const TYPE_IA_METAL_YIELD_FRACTION: f32 = 0.35;
    /// Core-collapse-scale birth draws split into a close pair. The pair
    /// retains the system's original lifetime and fate without creating mass.
    const NS_BINARY_SPLIT_MASS: f32 = 30.0;
    const NS_MERGER_DELAY_MIN: f32 = 160.0;
    const NS_MERGER_DELAY_SPAN: f32 = 480.0;
    const NS_LUMINOSITY: f32 = 8.0;
    const GRB_RADIATED_FRACTION: f32 = 0.01;
    const GRB_RADIATION_BOOST: f32 = 220.0;
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
    /// Inner disk over which weak viscosity bleeds angular momentum. The
    /// sink remains two cells wide, so nuclear rings persist while leaking.
    const BH_INFLOW_RADIUS: f32 = 7.0;
    const BH_NUCLEAR_VISCOSITY: f32 = 0.0015;
    const BH_GAS_SOFTENING_SQ: f32 = 9.0;
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
    const RNG_BH_ACCRETION: u64 = 3;
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
            metal_mass: vec![cell_initial_mass as f32 * Galaxy::INITIAL_METALLICITY; n],
            scratch_metal_mass: vec![0.0; n],
            tick_count: 0,
            master_seed: 0,
            events: EventQueue::new(),
            stars: Stars::new(),
            bh_mass: 0.0,
            bh_mass_initial: 0.0,
            radiated_total: 0.0,
            bh_metal_mass: 0.0,
            radiated_metal_mass: 0.0,
            field_ax: vec![0.0; Galaxy::FIELD_RES * Galaxy::FIELD_RES],
            field_ay: vec![0.0; Galaxy::FIELD_RES * Galaxy::FIELD_RES],
            radiation: vec![0.0; Galaxy::FIELD_RES * Galaxy::FIELD_RES],
            collapse_heat: vec![0; n],
            halo_gas_mass: 0,
            halo_metal_mass: 0.0,
            stellar_halo_mass: 0.0,
            stellar_halo_metal_mass: 0.0,
            metal_produced_total: 0.0,
            phase_mixed_count: 0,
            next_cluster_id: 0,
            next_star_id: 1,
            next_binary_id: 1,
            heat_parent: vec![0; n],
            scenario: Scenario::IrregularSpiral,
        }
    }

    /// Default-scenario seed. Preserved for backwards-compatibility with
    /// the JS `Frontend.seed(mass)` call.
    pub fn seed(&self, additional: u16) -> Galaxy {
        self.seed_with_mode(additional, Scenario::IrregularSpiral)
    }

    /// Seed with a named scenario. Tuning constants assume default UI
    /// params (size=250, seed_mass=25).
    pub fn seed_with_mode(&self, additional: u16, mode: Scenario) -> Galaxy {
        let seed: u64 = rand::rng().random();
        self.seed_with_mode_seeded(additional, mode, seed)
    }

    /// Reproducible [`seed_with_mode`]: same `(additional, mode, seed)`
    /// gives byte-identical state, enabling `?seed=...` URL sharing for
    /// every scenario.
    pub fn seed_with_mode_seeded(&self, additional: u16, mode: Scenario, seed: u64) -> Galaxy {
        let mut rng = StdRng::seed_from_u64(seed);
        self.seed_mode_kernel(additional, mode, seed, &mut rng)
    }

    // Private, so wasm-bindgen skips it; `dyn` because bindgen impls
    // cannot hold generics.
    fn seed_mode_kernel(
        &self,
        additional: u16,
        mode: Scenario,
        master_seed: u64,
        rng: &mut dyn rand::Rng,
    ) -> Galaxy {
        let p = mode.params();
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

        match p.bang {
            false => {
                if additional > 0 {
                    // Smoke field: three fBm stacks (density + two warp
                    // components), each four octaves of smoothstep value
                    // noise drawn from the seeded RNG.
                    let make_octaves = |rng: &mut dyn rand::Rng| -> Vec<Vec<f32>> {
                        Galaxy::SMOKE_OCTAVE_RES
                            .iter()
                            .map(|&res| {
                                (0..res * res)
                                    .map(|_| rng.random_range(0.0f32..1.0))
                                    .collect()
                            })
                            .collect()
                    };
                    let density_oct = make_octaves(rng);
                    let warp_x_oct = make_octaves(rng);
                    let warp_y_oct = make_octaves(rng);
                    let spiral_phase = rng.random_range(0.0f32..std::f32::consts::TAU);

                    let vnoise = |grid: &[f32], res: usize, u: f32, v: f32| -> f32 {
                        let fu = (u.rem_euclid(1.0)) * (res - 1) as f32;
                        let fv = (v.rem_euclid(1.0)) * (res - 1) as f32;
                        let x0 = fu as usize;
                        let y0 = fv as usize;
                        let x1 = (x0 + 1).min(res - 1);
                        let y1 = (y0 + 1).min(res - 1);
                        // Smoothstep fade - bilinear alone leaves diamond
                        // artifacts.
                        let tx = fu - x0 as f32;
                        let ty = fv - y0 as f32;
                        let tx = tx * tx * (3.0 - 2.0 * tx);
                        let ty = ty * ty * (3.0 - 2.0 * ty);
                        let a = grid[y0 * res + x0] * (1.0 - tx) + grid[y0 * res + x1] * tx;
                        let b = grid[y1 * res + x0] * (1.0 - tx) + grid[y1 * res + x1] * tx;
                        a * (1.0 - ty) + b * ty
                    };
                    let fbm = |octaves: &[Vec<f32>], u: f32, v: f32| -> f32 {
                        let mut sum = 0.0f32;
                        let mut amp = 1.0f32;
                        let mut norm = 0.0f32;
                        for (grid, &res) in octaves.iter().zip(Galaxy::SMOKE_OCTAVE_RES.iter()) {
                            sum += amp * vnoise(grid, res, u, v);
                            norm += amp;
                            amp *= 0.5;
                        }
                        sum / norm
                    };

                    let mut weights: Vec<(usize, f32)> = Vec::with_capacity(self.n / 3);
                    for i in 0..self.n {
                        let x = self.xs_i[i] as f32 - cx;
                        let y = self.ys_i[i] as f32 - cy;
                        if x * x + y * y > disk_r2 {
                            continue;
                        }
                        // Feather to zero approaching the rim - the
                        // smoke trails off instead of hitting a wall.
                        let r_frac = (x * x + y * y).sqrt() / disk_r;
                        let edge = if r_frac > Galaxy::EDGE_FEATHER_START {
                            let t = ((1.0 - r_frac) / (1.0 - Galaxy::EDGE_FEATHER_START))
                                .clamp(0.0, 1.0);
                            t * t * (3.0 - 2.0 * t)
                        } else {
                            1.0
                        };
                        // Exponential radial envelope (0 = flat disk).
                        let envelope = if p.radial_scale_frac > 0.0 {
                            (-r_frac / p.radial_scale_frac).exp()
                        } else {
                            1.0
                        };
                        let u = (x / size + 0.5).clamp(0.0, 1.0);
                        let v = (y / size + 0.5).clamp(0.0, 1.0);
                        // Domain warp: sample density through a noise
                        // displacement so structure curls instead of
                        // pooling.
                        let wu = u + Galaxy::SMOKE_WARP * (fbm(&warp_x_oct, u, v) - 0.5);
                        let wv = v + Galaxy::SMOKE_WARP * (fbm(&warp_y_oct, u, v) - 0.5);
                        let d = fbm(&density_oct, wu, wv);
                        // Stretch, then power-law: thin bright filaments,
                        // voids that reach clean through the disk.
                        let stretched = ((d - 0.5) * Galaxy::SMOKE_STRETCH + Galaxy::SMOKE_CENTER)
                            .clamp(0.0, 1.0);
                        let smoke = stretched.powf(p.smoke_contrast) * Galaxy::SMOKE_GAIN;
                        let r = (x * x + y * y).sqrt().max(1.0);
                        let theta = y.atan2(x);
                        // Two-arm density wave: cos(2 theta - pitch ln r).
                        let arm = 1.0
                            + p.spiral_amp
                                * (2.0 * theta - Galaxy::SPIRAL_PITCH * r.ln() + spiral_phase)
                                    .cos();
                        let w = smoke * arm * edge * envelope * rng.random_range(0.85f32..1.15);
                        if w > 0.0 {
                            weights.push((i, w));
                        }
                    }
                    // Normalize to a deterministic total: the fBm draw's
                    // mean varies +-35% seed to seed, and thin draws lose
                    // their seeded structure to dissipation long before
                    // t=1000. Fixing the budget is what makes the end
                    // shape sturdy across seeds - noise only textures.
                    let w_sum: f64 = weights.iter().map(|&(_, w)| w as f64).sum();
                    let target = additional as f64
                        * 0.5
                        * p.seed_gain as f64
                        * 0.53
                        * (std::f64::consts::PI * (disk_r as f64) * (disk_r as f64));
                    if w_sum > 0.0 {
                        let scale = (target / w_sum) as f32;
                        for &(i, w) in &weights {
                            let m = w * scale;
                            mass[i] = mass[i]
                                .saturating_add(m.round().clamp(0.0, u16::MAX as f32) as u16);
                        }
                    }
                }
            }
            true => {
                for m in mass.iter_mut() {
                    *m = 0;
                }
                let core_radius = (size * p.core_radius_frac).max(2.0);
                let core_r2 = core_radius * core_radius;
                // `additional` is the intensity knob (fixed SEED_MASS
                // constant on the JS side; the URL knob is retired).
                let core_fill = ((additional as f32 * p.core_fill_scale) as u16).max(40);
                for i in 0..self.n {
                    let x = self.xs_i[i] as f32 - cx;
                    let y = self.ys_i[i] as f32 - cy;
                    let r2 = x * x + y * y;
                    if r2 > core_r2 {
                        continue;
                    }
                    // Feathered core edge, same rationale as uniform.
                    let r_frac = (r2.sqrt() / core_radius).clamp(0.0, 1.0);
                    let edge = if r_frac > 0.6 {
                        let t = ((1.0 - r_frac) / 0.4).clamp(0.0, 1.0);
                        t * t * (3.0 - 2.0 * t)
                    } else {
                        1.0
                    };
                    let fill = core_fill.saturating_add(rng.random_range(0..=core_fill / 4));
                    mass[i] = (fill as f32 * edge) as u16;
                }
                // Ejection speed keyed to the seeded core's own escape
                // velocity - a fixed speed stops scaling once core mass
                // grows with size² and the "explosion" jams into a ball.
                let m_core: f64 = mass.iter().map(|&m| m as f64).sum();
                // Climb energy to the target radius: self-gravity escape
                // plus the halo potential difference 2 dPhi = v_flat^2 *
                // ln((rt^2 + rc^2) / rc^2). Without the halo term the
                // ejecta stall far short of the intended ring.
                let v_esc_sq = 2.0 * Galaxy::GRAVATIONAL_CONSTANT * m_core as f32 / core_radius;
                let rc_b = p.halo_core_frac * disk_r;
                let rt = p.eject_target_frac * disk_r;
                let halo_climb_sq =
                    p.v_flat * p.v_flat * ((rt * rt + rc_b * rc_b) / (rc_b * rc_b)).ln();
                let v_eject = p.eject_factor * (v_esc_sq + halo_climb_sq).sqrt();
                // Two-lobed ejection (bang => spiral): the fast lobes
                // race ahead and differential rotation winds them into
                // arms. Zero depth gives the symmetric shell the ring
                // scenario circularizes.
                let lobe_phase = rng.random_range(0.0f32..std::f32::consts::TAU);
                let (swirl_cos, swirl_sin) = (p.eject_swirl.cos(), p.eject_swirl.sin());
                for i in 0..self.n {
                    if mass[i] == 0 {
                        continue;
                    }
                    let x = self.xs_i[i] as f32 - cx;
                    let y = self.ys_i[i] as f32 - cy;
                    let r = (x * x + y * y).sqrt().max(1e-3);
                    let theta = y.atan2(x);
                    let lobes = 1.0 + p.eject_lobes * (2.0 * theta + lobe_phase).cos();
                    // Ejection direction: radial tilted prograde by the
                    // swirl angle. Generous speed jitter breaks up the
                    // diagonal-travel grid artifact.
                    let jitter = rng.random_range(-0.2f32..=0.2f32);
                    let speed = v_eject * lobes * (1.0 + jitter);
                    let (rx, ry) = (x / r, y / r);
                    let (tx, ty) = (-y / r, x / r);
                    vel_x[i] = (rx * swirl_cos + tx * swirl_sin) * speed;
                    vel_y[i] = (ry * swirl_cos + ty * swirl_sin) * speed;
                }
            }
        }

        // Every scenario gets orbital support on top of its seeder
        // velocities: v = boost * sqrt(G·M_enc/r + v_c(r)^2) tangentially,
        // with M_enc prefix-summed over cells sorted by radius and v_c
        // the halo rotation curve - seeding at the combined equilibrium
        // speed, not just self-gravity's. A hand-tuned linear ramp
        // under-spins the disk and it free-falls to the center within a
        // few hundred ticks.
        let mut order: Vec<usize> = (0..self.n).collect();
        let r2_of = |i: usize, xs: &[i16], ys: &[i16]| {
            let x = xs[i] as f32 - cx;
            let y = ys[i] as f32 - cy;
            x * x + y * y
        };
        order.sort_by(|&a, &b| {
            r2_of(a, &self.xs_i, &self.ys_i).total_cmp(&r2_of(b, &self.xs_i, &self.ys_i))
        });
        let rc = p.halo_core_frac * disk_r;
        let rc2 = rc * rc;
        let mut m_enc: f64 = 0.0;
        for &i in &order {
            m_enc += mass[i] as f64;
            if mass[i] == 0 {
                continue;
            }
            let x = self.xs_i[i] as f32 - cx;
            let y = self.ys_i[i] as f32 - cy;
            let r2 = x * x + y * y;
            let r = r2.sqrt();
            if r < 1e-3 {
                continue;
            }
            let vc2_halo = p.v_flat * p.v_flat * r2 / (r2 + rc2);
            let v = (Galaxy::GRAVATIONAL_CONSTANT * m_enc as f32 / r + vc2_halo).sqrt()
                * p.rotation_boost;
            vel_x[i] += -y / r * v;
            vel_y[i] += x / r * v;
        }

        // Isotropic velocity jitter - pressure support that keeps the
        // elliptical scenario puffed instead of pancaked.
        if p.vel_dispersion > 0.0 {
            for i in 0..self.n {
                if mass[i] == 0 {
                    continue;
                }
                vel_x[i] += rng.random_range(-p.vel_dispersion..=p.vel_dispersion);
                vel_y[i] += rng.random_range(-p.vel_dispersion..=p.vel_dispersion);
            }
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
        g.metal_mass = g
            .mass
            .iter()
            .map(|&mass| mass as f32 * Galaxy::INITIAL_METALLICITY)
            .collect();
        g.scratch_metal_mass = vec![0.0; self.n];
        g.tick_count = 0;
        g.master_seed = master_seed;
        g.events = EventQueue::new();
        g.stars = Stars::new();
        g.bh_mass = total_mass as f32 * Galaxy::BH_MASS_FRACTION;
        g.bh_mass_initial = g.bh_mass;
        g.radiated_total = 0.0;
        g.bh_metal_mass = g.bh_mass as f64 * Galaxy::INITIAL_METALLICITY as f64;
        g.radiated_metal_mass = 0.0;
        g.radiation = vec![0.0; Galaxy::FIELD_RES * Galaxy::FIELD_RES];
        g.collapse_heat = vec![0; self.n];
        g.halo_gas_mass = 0;
        g.halo_metal_mass = 0.0;
        g.stellar_halo_mass = 0.0;
        g.stellar_halo_metal_mass = 0.0;
        g.metal_produced_total = 0.0;
        g.phase_mixed_count = 0;
        g.next_cluster_id = 0;
        g.next_star_id = 1;
        g.next_binary_id = 1;
        g.heat_parent = vec![0; self.n];
        g.scenario = mode;
        g
    }

    /// Reproducible [`seed`] variant. Same `(additional, seed)` gives
    /// byte-identical state, enabling `?seed=...` URL sharing.
    pub fn seed_with(&self, additional: u16, seed: u64) -> Galaxy {
        self.seed_with_mode_seeded(additional, Scenario::IrregularSpiral, seed)
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
        // External backends replace gravity only. Scenario-owned gas
        // forces still run in Rust so CPU and WebGPU share one model.
        next.process_spiral_density_wave(time);
        next.process_ring_density_wave(time);
        next.process_gas_pressure(time);
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

    /// Renderer packing:
    /// [x, y, luminosity, color_index, stage, cluster_id, age] per star.
    pub fn star_render_data(&self) -> Vec<f32> {
        self.stars.render_data()
    }

    /// Cumulative number of distinct stellar associations formed since seeding.
    pub fn stellar_association_count(&self) -> u32 {
        self.next_cluster_id
    }

    pub fn neutron_star_count(&self) -> usize {
        self.stars
            .stage
            .iter()
            .filter(|&&stage| stage == Stage::NeutronStar as u8)
            .count()
    }

    pub fn red_giant_count(&self) -> usize {
        self.stars
            .stage
            .iter()
            .filter(|&&stage| stage == Stage::RedGiant as u8)
            .count()
    }

    pub fn white_dwarf_count(&self) -> usize {
        self.stars
            .stage
            .iter()
            .filter(|&&stage| stage == Stage::WhiteDwarf as u8)
            .count()
    }

    pub fn stellar_halo_mass_value(&self) -> f64 {
        self.stellar_halo_mass
    }

    pub fn phase_mixed_count(&self) -> u64 {
        self.phase_mixed_count
    }

    /// Spawn one star directly. Debug/test path - production stars are
    /// born from CloudCollapse -> StarBirth events. Derived attributes
    /// (lifetime, luminosity, color) come from mass.
    pub fn spawn_star(&mut self, x: f32, y: f32, vx: f32, vy: f32, mass: f32) -> usize {
        let m = mass.max(1.0);
        let (lifetime, luminosity, class_index) = Galaxy::star_attrs(m);
        let id = self.next_star_id;
        self.next_star_id += 1;
        self.stars.spawn(
            x,
            y,
            vx,
            vy,
            m,
            0.0,
            lifetime,
            luminosity,
            class_index,
            NO_CLUSTER,
            NO_BINARY,
            id,
        )
    }

    /// Renderer transients: [kind, x, y, ticks_ago, magnitude] per recent
    /// executed event within the transient window. Magnitude is
    /// kind-specific physical mass or budget, used only to scale the
    /// renderer's transient.
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
                crate::events::EventKind::GammaRayBurst => (3.0f32, ev.target),
                crate::events::EventKind::PlanetaryNebula => (4.0f32, ev.target),
                crate::events::EventKind::TypeIaSupernova => (5.0f32, ev.target),
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

    /// Per-cell cold-gas metallicity for dust and emission-line rendering.
    pub fn gas_metallicity(&self) -> Vec<f32> {
        self.mass
            .iter()
            .zip(self.metal_mass.iter())
            .map(|(&mass, &metals)| {
                if mass == 0 {
                    0.0
                } else {
                    (metals / mass as f32).clamp(0.0, 1.0)
                }
            })
            .collect()
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

    /// Cold disk share of the active gas reservoir. The galactic fountain
    /// moves it between roughly 0.4 and 0.6 after feedback starts.
    pub fn gas_cold_fraction(&self) -> f32 {
        let cold: u64 = self.mass.iter().map(|&m| m as u64).sum();
        let active = cold + self.halo_gas_mass;
        if active == 0 {
            0.0
        } else {
            cold as f32 / active as f32
        }
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
            6 => NeutronStarMerger,
            7 => GammaRayBurst,
            8 => PlanetaryNebula,
            9 => TypeIaSupernova,
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

    /// Coarse-field state: [field_ax..., field_ay..., radiation...,
    /// metal_mass...]. The
    /// fields are mid-tick derived state - rebuilding after restore would
    /// use post-tick inputs and fork the trajectory. Opaque to JS.
    pub fn sim_state_field(&self) -> Vec<f32> {
        let mut out = self.field_ax.clone();
        out.extend_from_slice(&self.field_ay);
        out.extend_from_slice(&self.radiation);
        out.extend_from_slice(&self.metal_mass);
        out
    }

    pub fn restore_sim_state_field(&mut self, data: &[f32]) {
        let res = Galaxy::FIELD_RES * Galaxy::FIELD_RES;
        if data.len() != res * 3 + self.n {
            return;
        }
        self.field_ax.copy_from_slice(&data[..res]);
        self.field_ay.copy_from_slice(&data[res..res * 2]);
        self.radiation.copy_from_slice(&data[res * 2..res * 3]);
        for (i, &metals) in data[res * 3..].iter().enumerate() {
            self.metal_mass[i] = metals.clamp(0.0, self.mass[i] as f32);
        }
    }

    /// Versioned scheduler/event/RNG state: [version=7, tick lo/hi, seed
    /// lo/hi, bh_mass bits, bh_initial bits, radiated f64 bits lo/hi,
    /// halo-gas lo/hi, stellar-halo f64 lo/hi, phase-mixed lo/hi,
    /// next_cluster, next_star, next_binary, scenario, five composition
    /// ledger f64 values, n_cells, heat bytes
    /// packed 4-per-u32, heat_parent lo/hi per cell, then events].
    pub fn sim_state_meta(&self) -> Vec<u32> {
        let heat_words = self.n.div_ceil(4);
        let mut out = Vec::with_capacity(30 + heat_words + self.n * 2 + 6);
        out.push(7u32);
        out.push(self.tick_count as u32);
        out.push((self.tick_count >> 32) as u32);
        out.push(self.master_seed as u32);
        out.push((self.master_seed >> 32) as u32);
        out.push(self.bh_mass.to_bits());
        out.push(self.bh_mass_initial.to_bits());
        let rad_bits = self.radiated_total.to_bits();
        out.push(rad_bits as u32);
        out.push((rad_bits >> 32) as u32);
        out.push(self.halo_gas_mass as u32);
        out.push((self.halo_gas_mass >> 32) as u32);
        let stellar_halo_bits = self.stellar_halo_mass.to_bits();
        out.push(stellar_halo_bits as u32);
        out.push((stellar_halo_bits >> 32) as u32);
        out.push(self.phase_mixed_count as u32);
        out.push((self.phase_mixed_count >> 32) as u32);
        out.push(self.next_cluster_id);
        out.push(self.next_star_id);
        out.push(self.next_binary_id);
        out.push(self.scenario as u32);
        for value in [
            self.halo_metal_mass,
            self.stellar_halo_metal_mass,
            self.bh_metal_mass,
            self.radiated_metal_mass,
            self.metal_produced_total,
        ] {
            let bits = value.to_bits();
            out.push(bits as u32);
            out.push((bits >> 32) as u32);
        }
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
        if data.len() < 30 || data[0] != 7 {
            return;
        }
        self.tick_count = data[1] as u64 | ((data[2] as u64) << 32);
        self.master_seed = data[3] as u64 | ((data[4] as u64) << 32);
        self.bh_mass = f32::from_bits(data[5]);
        self.bh_mass_initial = f32::from_bits(data[6]);
        self.radiated_total = f64::from_bits(data[7] as u64 | ((data[8] as u64) << 32));
        self.halo_gas_mass = data[9] as u64 | ((data[10] as u64) << 32);
        self.stellar_halo_mass = f64::from_bits(data[11] as u64 | ((data[12] as u64) << 32));
        self.phase_mixed_count = data[13] as u64 | ((data[14] as u64) << 32);
        self.next_cluster_id = data[15];
        self.next_star_id = data[16];
        self.next_binary_id = data[17];
        self.scenario = Scenario::from_u32(data[18]);
        self.halo_metal_mass = f64::from_bits(data[19] as u64 | ((data[20] as u64) << 32));
        self.stellar_halo_metal_mass = f64::from_bits(data[21] as u64 | ((data[22] as u64) << 32));
        self.bh_metal_mass = f64::from_bits(data[23] as u64 | ((data[24] as u64) << 32));
        self.radiated_metal_mass = f64::from_bits(data[25] as u64 | ((data[26] as u64) << 32));
        self.metal_produced_total = f64::from_bits(data[27] as u64 | ((data[28] as u64) << 32));
        let n_cells = data[29] as usize;
        if n_cells != self.n {
            return;
        }
        let heat_words = n_cells.div_ceil(4);
        let parents_at = 30 + heat_words;
        let events_at = parents_at + n_cells * 2;
        if data.len() < events_at {
            return;
        }
        for i in 0..n_cells {
            let w = data[30 + i / 4];
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

    /// Pitch-aware logarithmic m=2 amplitude of the visible gas disk.
    /// Unlike a plain azimuthal m=2 score, this rejects bars and opposite
    /// clumps that do not follow the configured spiral pitch.
    pub fn spiral_coherence(&self) -> f32 {
        self.spiral_structure().0
    }

    /// Fraction of radial disk bands that carry the same pitched arm phase.
    /// Sparse clumps can score well globally, so visible morphology requires
    /// both coherence and radial coverage.
    pub fn spiral_coverage(&self) -> f32 {
        self.spiral_structure().1
    }

    /// Fraction of visible gas mass inside the scenario's target annulus.
    pub fn ring_concentration(&self) -> f32 {
        self.ring_structure().0
    }

    /// Fraction of visible gas mass outside the ring's hollow core.
    pub fn ring_core_depletion(&self) -> f32 {
        self.ring_structure().1
    }

    /// Fraction of azimuthal sectors populated across the target annulus.
    pub fn ring_coverage(&self) -> f32 {
        self.ring_structure().2
    }

    /// Mass-weighted radial RMS distance from the target ring, in disk radii.
    /// Lower values describe a narrower annulus.
    pub fn ring_width(&self) -> f32 {
        self.ring_structure().3
    }

    /// Fraction of resolved stellar mass inside 0.35 disk radii.
    pub fn spheroid_concentration(&self) -> f32 {
        self.spheroid_structure().0
    }

    /// One minus the strongest low-order angular Fourier mode. Higher values
    /// distinguish a smooth spheroid from surviving clumps, bars, and arms.
    pub fn spheroid_smoothness(&self) -> f32 {
        self.spheroid_structure().1
    }

    /// Projected minor-to-major axis ratio from the stellar inertia tensor.
    pub fn spheroid_axis_ratio(&self) -> f32 {
        self.spheroid_structure().2
    }

    /// Mass-weighted stellar RMS radius in units of the disk radius.
    pub fn spheroid_extent(&self) -> f32 {
        self.spheroid_structure().3
    }

    /// Net stellar rotation divided by RMS speed. Pressure-supported systems
    /// approach zero, while a cold rotating disk approaches one.
    pub fn spheroid_rotational_support(&self) -> f32 {
        self.spheroid_structure().4
    }

    fn spiral_structure(&self) -> (f32, f32) {
        const BINS: usize = 8;
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let mut re = 0.0f64;
        let mut im = 0.0f64;
        let mut total = 0.0f64;
        let mut bin_re = [0.0f64; BINS];
        let mut bin_im = [0.0f64; BINS];
        let mut bin_mass = [0.0f64; BINS];
        let mut bin_occupied = [0usize; BINS];
        for i in 0..self.n {
            let mass = self.mass[i] as f64;
            if mass == 0.0 {
                continue;
            }
            let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
            let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
            let r = (x * x + y * y).sqrt();
            if r < disk_r * 0.12 || r > disk_r * 0.94 {
                continue;
            }
            let phase = 2.0 * y.atan2(x) - Galaxy::SPIRAL_PITCH * r.max(1.0).ln();
            re += mass * phase.cos() as f64;
            im += mass * phase.sin() as f64;
            total += mass;
            let radial_fraction = r / disk_r;
            let bin = (((radial_fraction - 0.12) / 0.82) * BINS as f32)
                .floor()
                .clamp(0.0, (BINS - 1) as f32) as usize;
            bin_re[bin] += mass * phase.cos() as f64;
            bin_im[bin] += mass * phase.sin() as f64;
            bin_mass[bin] += mass;
            bin_occupied[bin] += 1;
        }
        if total == 0.0 {
            return (0.0, 0.0);
        }
        let amplitude = (re * re + im * im).sqrt();
        let coherence = (amplitude / total) as f32;
        if amplitude == 0.0 {
            return (coherence, 0.0);
        }
        let mut covered = 0usize;
        for bin in 0..BINS {
            let bin_amplitude = (bin_re[bin] * bin_re[bin] + bin_im[bin] * bin_im[bin]).sqrt();
            let bin_coherence = if bin_mass[bin] > 0.0 {
                bin_amplitude / bin_mass[bin]
            } else {
                0.0
            };
            let alignment = if bin_amplitude > 0.0 {
                (bin_re[bin] * re + bin_im[bin] * im) / (bin_amplitude * amplitude)
            } else {
                0.0
            };
            if bin_occupied[bin] >= 4
                && bin_mass[bin] >= total * 0.02
                && bin_coherence >= 0.25
                && alignment >= std::f64::consts::FRAC_1_SQRT_2
            {
                covered += 1;
            }
        }
        (coherence, covered as f32 / BINS as f32)
    }

    fn ring_structure(&self) -> (f32, f32, f32, f32) {
        const SECTORS: usize = 12;
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let configured = self.scenario.params().ring_radius_frac;
        let target_fraction = if configured > 0.0 { configured } else { 0.58 };
        let target = disk_r * target_fraction;
        let half_width = disk_r * 0.12;
        let core_radius = disk_r * (target_fraction - 0.2).max(0.12);
        let mut total = 0.0f64;
        let mut annular = 0.0f64;
        let mut core = 0.0f64;
        let mut radial_variance = 0.0f64;
        let mut sector_mass = [0.0f64; SECTORS];
        let mut sector_occupied = [0usize; SECTORS];
        for i in 0..self.n {
            let mass = self.mass[i] as f64;
            if mass == 0.0 {
                continue;
            }
            let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
            let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
            let r = (x * x + y * y).sqrt();
            if r > disk_r * 0.94 {
                continue;
            }
            total += mass;
            radial_variance += mass * ((r - target) / disk_r).powi(2) as f64;
            if r < core_radius {
                core += mass;
            }
            if (r - target).abs() <= half_width {
                annular += mass;
                let theta = y.atan2(x).rem_euclid(std::f32::consts::TAU);
                let sector = ((theta / std::f32::consts::TAU) * SECTORS as f32)
                    .floor()
                    .clamp(0.0, (SECTORS - 1) as f32) as usize;
                sector_mass[sector] += mass;
                sector_occupied[sector] += 1;
            }
        }
        if total == 0.0 {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let covered = if annular > 0.0 {
            (0..SECTORS)
                .filter(|&sector| {
                    sector_occupied[sector] >= 2 && sector_mass[sector] >= annular * 0.03
                })
                .count()
        } else {
            0
        };
        (
            (annular / total) as f32,
            (1.0 - core / total) as f32,
            covered as f32 / SECTORS as f32,
            (radial_variance / total).sqrt() as f32,
        )
    }

    fn spheroid_structure(&self) -> (f32, f32, f32, f32, f32) {
        const HARMONICS: usize = 4;
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let mut total = 0.0f64;
        let mut central = 0.0f64;
        let mut weighted_x = 0.0f64;
        let mut weighted_y = 0.0f64;
        let mut radius_sq = 0.0f64;
        let mut tangential = 0.0f64;
        let mut speed_sq = 0.0f64;
        let mut harmonic_re = [0.0f64; HARMONICS];
        let mut harmonic_im = [0.0f64; HARMONICS];
        for i in 0..self.stars.len() {
            let mass = self.stars.mass[i] as f64;
            if mass <= 0.0 {
                continue;
            }
            let x = self.stars.pos_x[i] - center;
            let y = self.stars.pos_y[i] - center;
            let r = (x * x + y * y).sqrt();
            if r > disk_r {
                continue;
            }
            total += mass;
            weighted_x += mass * x as f64;
            weighted_y += mass * y as f64;
            radius_sq += mass * (r * r) as f64;
            if r <= disk_r * 0.35 {
                central += mass;
            }
            if r > 1e-3 {
                let theta = y.atan2(x);
                for harmonic in 1..=HARMONICS {
                    let phase = theta * harmonic as f32;
                    harmonic_re[harmonic - 1] += mass * phase.cos() as f64;
                    harmonic_im[harmonic - 1] += mass * phase.sin() as f64;
                }
                tangential +=
                    mass * ((x * self.stars.vel_y[i] - y * self.stars.vel_x[i]) / r) as f64;
            }
            speed_sq += mass
                * (self.stars.vel_x[i] * self.stars.vel_x[i]
                    + self.stars.vel_y[i] * self.stars.vel_y[i]) as f64;
        }
        if total <= 0.0 {
            return (0.0, 0.0, 0.0, 0.0, 0.0);
        }

        let centroid_x = weighted_x / total;
        let centroid_y = weighted_y / total;
        let mut moment_xx = 0.0f64;
        let mut moment_yy = 0.0f64;
        let mut moment_xy = 0.0f64;
        for i in 0..self.stars.len() {
            let mass = self.stars.mass[i] as f64;
            let center_x = self.stars.pos_x[i] as f64 - center as f64;
            let center_y = self.stars.pos_y[i] as f64 - center as f64;
            if mass <= 0.0 || center_x * center_x + center_y * center_y > (disk_r * disk_r) as f64 {
                continue;
            }
            let x = center_x - centroid_x;
            let y = center_y - centroid_y;
            moment_xx += mass * x * x;
            moment_yy += mass * y * y;
            moment_xy += mass * x * y;
        }
        let trace = moment_xx + moment_yy;
        let discriminant = ((moment_xx - moment_yy).powi(2) + 4.0 * moment_xy.powi(2)).sqrt();
        let major = ((trace + discriminant) * 0.5).max(0.0);
        let minor = ((trace - discriminant) * 0.5).max(0.0);
        let axis_ratio = if major > 0.0 {
            (minor / major).sqrt()
        } else {
            0.0
        };
        let strongest_mode = harmonic_re
            .iter()
            .zip(harmonic_im.iter())
            .map(|(&re, &im)| (re * re + im * im).sqrt() / total)
            .fold(0.0f64, f64::max);
        let mean_tangential = (tangential / total).abs();
        let rms_speed = (speed_sq / total).sqrt();
        (
            (central / total) as f32,
            (1.0 - strongest_mode).clamp(0.0, 1.0) as f32,
            axis_ratio as f32,
            ((radius_sq / total).sqrt() / disk_r as f64) as f32,
            if rms_speed > 0.0 {
                (mean_tangential / rms_speed).clamp(0.0, 1.0) as f32
            } else {
                0.0
            },
        )
    }

    pub(crate) fn process_gravity(&mut self, _time: f32) {
        self.gravitate_all();
    }

    /// Apply a rotating logarithmic density-wave potential to gas. The
    /// force is normal to the configured arm phase, so orbiting gas crosses
    /// the wave, compresses along it, and can collapse into stars there.
    pub(crate) fn process_spiral_density_wave(&mut self, _time: f32) {
        let p = self.scenario.params();
        if p.spiral_wave_strength <= 0.0 {
            return;
        }
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let inner = disk_r * 0.12;
        let outer = disk_r * 0.94;
        let taper_width = disk_r * 0.12;
        let pattern_phase = p.spiral_pattern_step * self.tick_count as f32;
        for i in 0..self.n {
            if self.mass[i] == 0 {
                continue;
            }
            let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
            let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
            let r2 = x * x + y * y;
            let r = r2.sqrt();
            if r <= inner || r >= outer {
                continue;
            }
            let inner_t = ((r - inner) / taper_width).clamp(0.0, 1.0);
            let outer_t = ((outer - r) / taper_width).clamp(0.0, 1.0);
            let taper = inner_t
                * inner_t
                * (3.0 - 2.0 * inner_t)
                * outer_t
                * outer_t
                * (3.0 - 2.0 * outer_t);
            let phase = 2.0 * y.atan2(x) - Galaxy::SPIRAL_PITCH * r.ln() - pattern_phase;
            // grad(2 theta - pitch ln r).
            let inv_r2 = 1.0 / r2;
            let grad_x = (-2.0 * y - Galaxy::SPIRAL_PITCH * x) * inv_r2;
            let grad_y = (2.0 * x - Galaxy::SPIRAL_PITCH * y) * inv_r2;
            let force = -p.spiral_wave_strength * taper * phase.sin();
            self.acc_x[i] += force * grad_x;
            self.acc_y[i] += force * grad_y;
        }
    }

    /// Apply an axisymmetric annular potential to gas. Ejecta cross and
    /// settle into the minimum while newborn collisionless stars retain
    /// the orbital velocity inherited from their natal gas.
    pub(crate) fn process_ring_density_wave(&mut self, _time: f32) {
        let p = self.scenario.params();
        if p.ring_wave_strength <= 0.0 {
            return;
        }
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let target = disk_r * p.ring_radius_frac;
        let scale = (disk_r * 0.14).max(1.0);
        for i in 0..self.n {
            if self.mass[i] == 0 {
                continue;
            }
            let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
            let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
            let r = (x * x + y * y).sqrt();
            if r <= disk_r * 0.06 || r >= disk_r * 0.96 {
                continue;
            }
            let radial_force = -p.ring_wave_strength * ((r - target) / scale).tanh();
            self.acc_x[i] += radial_force * x / r;
            self.acc_y[i] += radial_force * y / r;
        }
    }

    /// Resolve the local isothermal pressure gradient before advection.
    /// The conservative post-advection flux below carries the same model
    /// through parcel collisions without inventing mass or momentum.
    pub(crate) fn process_gas_pressure(&mut self, _time: f32) {
        let p = self.scenario.params();
        if p.gas_pressure <= 0.0 {
            return;
        }
        let size = self.size as i32;
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let pressure_scale = p.gas_pressure / (2.0 * Galaxy::CELL_MASS_CAP as f32);
        for i in 0..self.n {
            if self.mass[i] == 0 {
                continue;
            }
            if p.spiral_wave_strength > 0.0 {
                let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
                let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
                let r = (x * x + y * y).sqrt();
                if r <= disk_r * 0.12 || r >= disk_r * 0.94 {
                    continue;
                }
            }
            let col = i as i32 % size;
            let row = i as i32 / size;
            let left = self.mass
                [self.col_row_to_index(wrap(col - 1, size) as u16, row as u16) as usize]
                as f32;
            let right = self.mass
                [self.col_row_to_index(wrap(col + 1, size) as u16, row as u16) as usize]
                as f32;
            let up = self.mass
                [self.col_row_to_index(col as u16, wrap(row - 1, size) as u16) as usize]
                as f32;
            let down = self.mass
                [self.col_row_to_index(col as u16, wrap(row + 1, size) as u16) as usize]
                as f32;
            self.acc_x[i] += pressure_scale * (left - right);
            self.acc_y[i] += pressure_scale * (up - down);
        }
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
        // Halo centripetal term baked into the field so stars (and star
        // births, which sample the field for orbital support) feel the
        // halo too - but at half the gas curve and quarter total
        // strength (STAR_FIELD_SCALE), so star orbits run at half the
        // gas pace.
        let p = self.scenario.params();
        let center = size_f * 0.5;
        let rc = p.halo_core_frac * self.disk_radius();
        let rc2 = rc * rc;
        let v_flat_star = p.v_flat * 0.5;
        let v_flat2 = v_flat_star * v_flat_star;
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
                let hx = wx - center;
                let hy = wy - center;
                let ah = v_flat2 / (hx * hx + hy * hy + rc2);
                self.field_ax[fy * res + fx] = (ax - ah * hx) * Galaxy::STAR_FIELD_SCALE;
                self.field_ay[fy * res + fx] = (ay - ah * hy) * Galaxy::STAR_FIELD_SCALE;
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

    /// Reduce star-local cluster ids into deterministic center-of-mass
    /// aggregates. Cluster ids are allocated densely, so a Vec keeps this
    /// hot path cheaper and more reproducible than a hash table.
    fn association_aggregates(&self) -> Vec<AssociationAggregate> {
        let mut associations = vec![AssociationAggregate::default(); self.next_cluster_id as usize];
        for i in 0..self.stars.len() {
            let cluster = self.stars.cluster_id[i];
            if cluster == NO_CLUSTER || cluster as usize >= associations.len() {
                continue;
            }
            let mass = self.stars.mass[i];
            let a = &mut associations[cluster as usize];
            a.mass += mass;
            a.weighted_x += mass * self.stars.pos_x[i];
            a.weighted_y += mass * self.stars.pos_y[i];
            a.weighted_vx += mass * self.stars.vel_x[i];
            a.weighted_vy += mass * self.stars.vel_y[i];
            a.oldest_age = a.oldest_age.max(self.stars.age[i]);
            a.members += 1;
        }
        associations
    }

    /// Find a still-forming association close enough to share the same
    /// molecular complex. Distance is measured to young main-sequence
    /// members rather than an ever-expanding bounding box, preventing a
    /// long tidal stream from chaining unrelated births across the disk.
    fn nearby_young_association(&self, x: f32, y: f32) -> Option<u32> {
        let join_r2 = Galaxy::ASSOCIATION_JOIN_RADIUS * Galaxy::ASSOCIATION_JOIN_RADIUS;
        let mut best: Option<(f32, u32)> = None;
        for i in 0..self.stars.len() {
            let cluster = self.stars.cluster_id[i];
            if cluster == NO_CLUSTER
                || self.stars.stage[i] != Stage::MainSequence as u8
                || self.stars.age[i] > Galaxy::ASSOCIATION_JOIN_MAX_AGE
            {
                continue;
            }
            let dx = self.stars.pos_x[i] - x;
            let dy = self.stars.pos_y[i] - y;
            let d2 = dx * dx + dy * dy;
            if d2 > join_r2 {
                continue;
            }
            if best.is_none_or(|(best_d2, best_cluster)| {
                d2 < best_d2 || (d2 == best_d2 && cluster < best_cluster)
            }) {
                best = Some((d2, cluster));
            }
        }
        best.map(|(_, cluster)| cluster)
    }

    /// Circular support from an azimuthal average of the live stellar
    /// field. Sampling around the same radius removes the nearest-clump
    /// direction that makes a local field sample unsuitable for choosing
    /// an association's galactic orbit, while retaining the actual gas,
    /// halo, stellar, and black-hole mass already encoded in the field.
    fn association_circular_speed(&self, radius: f32) -> f32 {
        if radius < 1e-3 {
            return 0.0;
        }
        const SAMPLES: usize = 8;
        let center = self.size as f32 * 0.5;
        let mut inward = 0.0f32;
        for sample in 0..SAMPLES {
            let angle = std::f32::consts::TAU * sample as f32 / SAMPLES as f32;
            let (rx, ry) = (angle.cos(), angle.sin());
            let (ax, ay) = self.sample_field(center + rx * radius, center + ry * radius);
            inward += (-(ax * rx + ay * ry)).max(0.0);
        }
        let mean_inward = inward / SAMPLES as f32;
        (mean_inward * radius).sqrt().min(Galaxy::BIRTH_VCIRC_CAP)
    }

    /// Smooth axisymmetric support not captured robustly by a local sample
    /// of the coarse, clump-dominated field. The weights keep associations
    /// coupled to the intentionally slower stellar rotation curve.
    fn association_background_speed(&self, radius: f32) -> f32 {
        let params = self.scenario.params();
        let halo_core = params.halo_core_frac * self.disk_radius();
        let halo_support = params.v_flat * radius
            / (radius * radius + halo_core * halo_core).sqrt()
            * Galaxy::ASSOCIATION_HALO_ORBIT_FLOOR;
        let bh_support = (Galaxy::GRAVATIONAL_CONSTANT * self.bh_mass * radius * radius
            / (radius * radius + Galaxy::BH_GAS_SOFTENING_SQ).powf(1.5))
        .sqrt();
        0.55 * halo_support + 0.65 * bh_support
    }

    fn association_tidal_radius(&self, association: AssociationAggregate) -> f32 {
        let (cx, cy) = association.center();
        let center = self.size as f32 * 0.5;
        let (rx, ry) = (cx - center, cy - center);
        let radius = (rx * rx + ry * ry).sqrt().max(1.0);
        let (ax, ay) = self.sample_field(cx, cy);
        let inward = (-(ax * rx + ay * ry) / radius).max(1e-5);
        let omega_sq = inward / radius;
        let age_fraction =
            (1.0 - association.oldest_age / Galaxy::ASSOCIATION_BINDING_LIFETIME).clamp(0.0, 1.0);
        let bound_mass = association.mass * (0.35 + 0.65 * age_fraction);
        (Galaxy::ASSOCIATION_BINDING_G * bound_mass / (3.0 * omega_sq))
            .cbrt()
            .clamp(
                Galaxy::ASSOCIATION_TIDAL_RADIUS_MIN,
                Galaxy::ASSOCIATION_TIDAL_RADIUS_MAX,
            )
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
        let disk_drag = (-self.scenario.params().star_drag * time).exp();

        // Associations are real but temporary. Once the oldest member has
        // exhausted the binding lifetime, or a member crosses the local
        // tidal radius after the embedded-cluster grace period, that star
        // keeps its exact phase-space state and simply becomes unbound.
        // The resulting differential galactic acceleration makes a stream.
        let mut associations = self.association_aggregates();
        let tidal_radii: Vec<f32> = associations
            .iter()
            .map(|&association| self.association_tidal_radius(association))
            .collect();
        for i in 0..self.stars.len() {
            let cluster = self.stars.cluster_id[i];
            if cluster == NO_CLUSTER || cluster as usize >= associations.len() {
                continue;
            }
            let association = associations[cluster as usize];
            if association.members < Galaxy::ASSOCIATION_MIN_MEMBERS
                || association.oldest_age >= Galaxy::ASSOCIATION_BINDING_LIFETIME
            {
                self.stars.cluster_id[i] = NO_CLUSTER;
                continue;
            }
            if association.oldest_age < Galaxy::ASSOCIATION_TIDAL_GRACE {
                continue;
            }
            let (cx, cy) = association.center();
            let distance = (self.stars.pos_x[i] - cx).hypot(self.stars.pos_y[i] - cy);
            if distance > tidal_radii[cluster as usize] {
                self.stars.cluster_id[i] = NO_CLUSTER;
            }
        }

        // Rebuild after releases, then calculate a softened association
        // acceleration for every remaining member. Subtracting each
        // cluster's mass-weighted mean acceleration makes the internal
        // force momentum-neutral, so binding cannot propel the cluster's
        // center of mass or manufacture orbital angular momentum.
        associations = self.association_aggregates();
        let mut binding_ax = vec![0.0f32; self.stars.len()];
        let mut binding_ay = vec![0.0f32; self.stars.len()];
        let mut recoil_x = vec![0.0f32; associations.len()];
        let mut recoil_y = vec![0.0f32; associations.len()];
        for i in 0..self.stars.len() {
            let cluster = self.stars.cluster_id[i];
            if cluster == NO_CLUSTER || cluster as usize >= associations.len() {
                continue;
            }
            let association = associations[cluster as usize];
            if association.members < Galaxy::ASSOCIATION_MIN_MEMBERS || association.mass <= 0.0 {
                continue;
            }
            let age_fraction = (1.0
                - association.oldest_age / Galaxy::ASSOCIATION_BINDING_LIFETIME)
                .clamp(0.0, 1.0);
            let (cx, cy) = association.center();
            let dx = cx - self.stars.pos_x[i];
            let dy = cy - self.stars.pos_y[i];
            let softened = dx * dx + dy * dy + Galaxy::ASSOCIATION_BINDING_SOFTENING_SQ;
            let mut scale = Galaxy::ASSOCIATION_BINDING_G * association.mass * age_fraction.powi(2)
                / (softened * softened.sqrt());
            let raw_magnitude = scale * (dx * dx + dy * dy).sqrt();
            if raw_magnitude > Galaxy::ASSOCIATION_ACCEL_MAX {
                scale *= Galaxy::ASSOCIATION_ACCEL_MAX / raw_magnitude;
            }
            let ax = dx * scale;
            let ay = dy * scale;
            binding_ax[i] = ax;
            binding_ay[i] = ay;
            recoil_x[cluster as usize] += self.stars.mass[i] * ax;
            recoil_y[cluster as usize] += self.stars.mass[i] * ay;
        }
        for i in 0..self.stars.len() {
            let cluster = self.stars.cluster_id[i];
            if cluster == NO_CLUSTER || cluster as usize >= associations.len() {
                continue;
            }
            let mass = associations[cluster as usize].mass;
            if mass > 0.0 {
                binding_ax[i] -= recoil_x[cluster as usize] / mass;
                binding_ay[i] -= recoil_y[cluster as usize] / mass;
            }
        }

        for i in 0..self.stars.len() {
            let px = self.stars.pos_x[i];
            let py = self.stars.pos_y[i];
            let (mut ax, mut ay) = self.sample_field(px, py);
            ax += binding_ax[i];
            ay += binding_ay[i];
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
            } else {
                vx *= disk_drag;
                vy *= disk_drag;
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
        let density_floor = (Galaxy::CELL_MASS_CAP as f32
            * self.scenario.params().collapse_density_fraction) as u16;
        let mut rng = self.rng_stream(Galaxy::RNG_COLLAPSE_WATCH);
        let tick = self.tick_count;
        let p = self.scenario.params();
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        for i in 0..self.n {
            let m = self.mass[i];
            if m < density_floor {
                // Only density failure resets - dispersal undoes the
                // compression. Radiation merely DELAYS ignition: a
                // shock-compressed cell near a luminous region holds its
                // heat (and causal parent) and waits to cool, otherwise
                // every dead giant's afterglow would erase the very
                // trigger its supernova just planted.
                self.collapse_heat[i] = 0;
                self.heat_parent[i] = 0;
                continue;
            }
            if p.ring_wave_strength > 0.0 {
                let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
                let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
                let ring_radius = disk_r * p.ring_radius_frac;
                if ((x * x + y * y).sqrt() - ring_radius).abs() > disk_r * 0.14 {
                    self.collapse_heat[i] = 0;
                    self.heat_parent[i] = 0;
                    continue;
                }
            }
            if self.radiation_at_cell(i) >= Galaxy::COLLAPSE_RADIATION_RESIST {
                continue;
            }
            self.collapse_heat[i] = self.collapse_heat[i].saturating_add(1);
            if self.collapse_heat[i] >= Galaxy::COLLAPSE_HEAT_TRIGGER
                && rng.random_range(0.0f32..1.0) < p.collapse_chance
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

    /// Irradiated gas evaporates into the hot circumgalactic reservoir.
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
            let metals = self.remove_cell_mass_with_metals(i, lose);
            self.halo_gas_mass += lose as u64;
            self.halo_metal_mass += metals as f64;
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

    /// Exchange visible disk gas with the hot halo on a feedback/cooling
    /// limit cycle. See docs/galaxy-rust.md.
    pub(crate) fn process_gas_fountain(&mut self, _time: f32) {
        if self.stars.len() == 0 && self.halo_gas_mass == 0 {
            return;
        }
        let cold: u64 = self.mass.iter().map(|&m| m as u64).sum();
        let active = cold + self.halo_gas_mass;
        if active == 0 {
            return;
        }

        let phase = std::f32::consts::TAU * (self.tick_count % Galaxy::FOUNTAIN_PERIOD) as f32
            / Galaxy::FOUNTAIN_PERIOD as f32;
        let target_fraction =
            Galaxy::FOUNTAIN_COLD_MIDPOINT + Galaxy::FOUNTAIN_COLD_AMPLITUDE * phase.sin();
        let target_cold = (active as f32 * target_fraction).round() as u64;
        let exchange_cap = ((active as f32 * Galaxy::FOUNTAIN_MAX_EXCHANGE).ceil() as u64).max(1);

        if cold > target_cold {
            let requested = (cold - target_cold).min(exchange_cap);
            let lifted = self.lift_gas_to_halo(requested);
            self.halo_gas_mass += lifted;
        } else if cold < target_cold && self.halo_gas_mass > 0 {
            let requested = (target_cold - cold)
                .min(exchange_cap)
                .min(self.halo_gas_mass);
            let cooled = self.cool_halo_gas(requested);
            self.halo_gas_mass -= cooled;
        }
    }

    /// Lift gas into the halo, preferring irradiated cells before a
    /// deterministic pass over the rest of the disk.
    fn lift_gas_to_halo(&mut self, requested: u64) -> u64 {
        let mut remaining = requested;
        let start = (splitmix64(self.master_seed ^ self.tick_count) as usize) % self.n;
        for hot_only in [true, false] {
            for offset in 0..self.n {
                if remaining == 0 {
                    break;
                }
                let i = (start + offset) % self.n;
                let m = self.mass[i];
                if m == 0 {
                    continue;
                }
                if hot_only && self.radiation_at_cell(i) < Galaxy::COLLAPSE_RADIATION_RESIST {
                    continue;
                }
                let per_cell = if hot_only {
                    (m / 4).max(1)
                } else {
                    (m / 16).max(1)
                };
                let take = (per_cell as u64).min(remaining) as u16;
                let metals = self.remove_cell_mass_with_metals(i, take);
                self.halo_metal_mass += metals as f64;
                remaining -= take as u64;
            }
            if remaining == 0 {
                break;
            }
        }
        requested - remaining
    }

    /// Cool halo gas into the evolved disk. Parcels inherit circular flow
    /// plus a slight inward drift so they visibly keep moving.
    fn cool_halo_gas(&mut self, requested: u64) -> u64 {
        let mut remaining = requested;
        let size = self.size as i32;
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let p = self.scenario.params();
        let rc = p.halo_core_frac * disk_r;
        let rc2 = rc * rc;
        let start = (splitmix64(self.master_seed ^ self.tick_count ^ 0xA076_1D64_78BD_642F)
            as usize)
            % self.n;
        let halo_metallicity = if self.halo_gas_mass == 0 {
            0.0
        } else {
            (self.halo_metal_mass / self.halo_gas_mass as f64).clamp(0.0, 1.0)
        };

        for seed_empty in [false, true] {
            for offset in 0..self.n {
                if remaining == 0 {
                    break;
                }
                let i = (start + offset) % self.n;
                let hash = splitmix64(i as u64 ^ self.tick_count);
                // Rain onto existing filaments first so the evolved shape
                // survives. A sparse fallback seeds empty annular cells.
                if seed_empty == (self.mass[i] > 0) {
                    continue;
                }
                if seed_empty && hash & 0b11 != 0 {
                    continue;
                }
                let col = i as i32 % size;
                let row = i as i32 / size;
                let x = col as f32 + 0.5 - center;
                let y = row as f32 + 0.5 - center;
                let r2 = x * x + y * y;
                let r = r2.sqrt();
                if r < disk_r * 0.25 || r > disk_r * 0.9 {
                    continue;
                }
                let capacity = Galaxy::CELL_MASS_CAP.saturating_sub(self.mass[i] as u32);
                if capacity == 0 {
                    continue;
                }
                let add = (Galaxy::FOUNTAIN_PACKET_MASS as u64)
                    .min(capacity as u64)
                    .min(remaining) as u16;
                if add == 0 {
                    continue;
                }

                let vc = p.flow_support * p.v_flat * r / (r2 + rc2).sqrt();
                let inward = 0.06;
                let rain_vx = -y / r * vc - x / r * inward;
                let rain_vy = x / r * vc - y / r * inward;
                let old = self.mass[i] as f32;
                let new = old + add as f32;
                self.vel_x[i] = (self.vel_x[i] * old + rain_vx * add as f32) / new;
                self.vel_y[i] = (self.vel_y[i] * old + rain_vy * add as f32) / new;
                if self.mass[i] == 0 {
                    let jx = ((hash & 0xFF) as f32 / 255.0 - 0.5) * 0.35;
                    let jy = (((hash >> 8) & 0xFF) as f32 / 255.0 - 0.5) * 0.35;
                    self.frac_x[i] = jx;
                    self.frac_y[i] = jy;
                }
                self.mass[i] += add;
                let metals = (add as f64 * halo_metallicity).min(self.halo_metal_mass);
                self.metal_mass[i] = (self.metal_mass[i] + metals as f32).min(self.mass[i] as f32);
                self.halo_metal_mass -= metals;
                remaining -= add as u64;
            }
            if remaining == 0 {
                break;
            }
        }
        requested - remaining
    }

    /// Remove whole gas mass while retaining the cell's composition ratio.
    fn remove_cell_mass_with_metals(&mut self, i: usize, requested: u16) -> f32 {
        let old_mass = self.mass[i];
        let take = requested.min(old_mass);
        if take == 0 || old_mass == 0 {
            return 0.0;
        }
        let fraction = take as f32 / old_mass as f32;
        let metals = (self.metal_mass[i] * fraction).min(take as f32);
        self.mass[i] -= take;
        self.metal_mass[i] = (self.metal_mass[i] - metals)
            .max(0.0)
            .min(self.mass[i] as f32);
        metals
    }

    /// Retire long-lived outer stars and old dim remnants from the resolved
    /// particle set into a diffuse stellar-halo reservoir. This is phase
    /// mixing, not destruction: mass stays in the baryonic ledger.
    pub(crate) fn process_stellar_halo(&mut self, _time: f32) {
        let center = self.size as f32 * 0.5;
        let mix_radius = self.disk_radius() * Galaxy::STELLAR_HALO_MIX_RADIUS;
        let mut i = 0;
        while i < self.stars.len() {
            let dx = self.stars.pos_x[i] - center;
            let dy = self.stars.pos_y[i] - center;
            let in_deep_halo = dx * dx + dy * dy > mix_radius * mix_radius;
            if in_deep_halo {
                self.stars.halo_dwell[i] = self.stars.halo_dwell[i].saturating_add(1);
            } else {
                self.stars.halo_dwell[i] = self.stars.halo_dwell[i].saturating_sub(1);
            }

            let stage = Stage::from_u8(self.stars.stage[i]);
            let spatially_mixed =
                stage != Stage::Merging && self.stars.halo_dwell[i] >= Galaxy::STELLAR_HALO_DWELL;
            let aged_remnant = matches!(stage, Stage::Remnant | Stage::MergedRemnant)
                && self.stars.age[i] >= Galaxy::REMNANT_RESOLVED_AGE;
            let aged_single_neutron_star = stage == Stage::NeutronStar
                && self.stars.binary_id[i] == NO_BINARY
                && self.stars.age[i] >= Galaxy::NEUTRON_STAR_RESOLVED_AGE;
            let aged_single_white_dwarf = stage == Stage::WhiteDwarf
                && self.stars.binary_id[i] == NO_BINARY
                && self.stars.age[i] >= Galaxy::WHITE_DWARF_RESOLVED_AGE;
            if !(spatially_mixed
                || aged_remnant
                || aged_single_neutron_star
                || aged_single_white_dwarf)
            {
                i += 1;
                continue;
            }

            let binary = self.stars.binary_id[i];
            if binary != NO_BINARY {
                for partner in &mut self.stars.binary_id {
                    if *partner == binary {
                        *partner = NO_BINARY;
                    }
                }
            }
            self.stellar_halo_mass += self.stars.mass[i] as f64;
            self.stellar_halo_metal_mass += self.stars.metal_mass[i] as f64;
            self.phase_mixed_count += 1;
            self.stars.swap_remove(i);
        }
    }

    /// Advance stellar ages by the sim time elapsed since the last run
    /// (dt x cadence, assuming dt is stable between runs - dt changes
    /// mid-run smear ages slightly, which is acceptable). Massive stars
    /// core-collapse. Lighter stars expand, shed envelopes, and leave white
    /// dwarfs that may later produce a delayed thermonuclear supernova.
    pub(crate) fn process_stellar_aging(&mut self, time: f32) {
        let elapsed = time * 8.0;
        let tick = self.tick_count;
        for i in 0..self.stars.len() {
            let stage = Stage::from_u8(self.stars.stage[i]);
            if stage == Stage::Merging {
                continue;
            }
            self.stars.age[i] += elapsed;
            if self.stars.age[i] < self.stars.lifetime[i]
                || !matches!(stage, Stage::MainSequence | Stage::RedGiant)
            {
                continue;
            }
            if stage == Stage::RedGiant {
                let cell = self.cell_index_at(self.stars.pos_x[i], self.stars.pos_y[i]);
                self.events.emit(
                    tick,
                    crate::events::EventKind::PlanetaryNebula,
                    self.stars.id[i],
                    cell as u32,
                    self.stars.mass[i],
                    crate::events::NO_PARENT,
                );
                self.stars.stage[i] = Stage::WhiteDwarf as u8;
                self.stars.age[i] = 0.0;
                self.stars.lifetime[i] = if self.stars.binary_id[i] == NO_BINARY {
                    Galaxy::WHITE_DWARF_RESOLVED_AGE
                } else {
                    self.white_dwarf_merger_delay(self.stars.binary_id[i])
                };
                self.stars.luminosity[i] = Galaxy::WD_LUMINOSITY;
                self.stars.color_index[i] = 0.72;
                continue;
            }

            let binary_core_collapse = self.stars.binary_id[i] != NO_BINARY
                && self.stars.mass[i] * 2.0 >= Galaxy::NS_BINARY_SPLIT_MASS;
            if self.stars.mass[i] >= Galaxy::SN_MASS_THRESHOLD || binary_core_collapse {
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
                self.stars.stage[i] = Stage::Remnant as u8;
            } else {
                self.stars.stage[i] = Stage::RedGiant as u8;
                self.stars.age[i] = 0.0;
                self.stars.lifetime[i] = Galaxy::RED_GIANT_LIFETIME;
                self.stars.luminosity[i] = (self.stars.luminosity[i] * 2.5).max(48.0);
                self.stars.color_index[i] = 0.02;
            }
        }

        // A compact pair merges only after both supernova handlers have
        // produced neutron stars and both deterministic delay clocks expire.
        let mut scheduled_binaries: Vec<u32> = Vec::new();
        for i in 0..self.stars.len() {
            if Stage::from_u8(self.stars.stage[i]) != Stage::NeutronStar
                || self.stars.binary_id[i] == NO_BINARY
                || self.stars.age[i] < self.stars.lifetime[i]
            {
                continue;
            }
            let binary = self.stars.binary_id[i];
            if scheduled_binaries.contains(&binary) {
                continue;
            }
            let Some(j) = (0..self.stars.len()).find(|&j| {
                j != i
                    && self.stars.binary_id[j] == binary
                    && Stage::from_u8(self.stars.stage[j]) == Stage::NeutronStar
                    && self.stars.age[j] >= self.stars.lifetime[j]
            }) else {
                continue;
            };
            self.events.emit(
                tick,
                crate::events::EventKind::NeutronStarMerger,
                self.stars.id[i],
                self.stars.id[j],
                self.stars.mass[i] + self.stars.mass[j],
                crate::events::NO_PARENT,
            );
            self.stars.stage[i] = Stage::Merging as u8;
            self.stars.stage[j] = Stage::Merging as u8;
            scheduled_binaries.push(binary);
        }

        // Intermediate-mass binaries follow the same deterministic pairing
        // contract, but merge as white dwarfs and disrupt completely.
        for i in 0..self.stars.len() {
            if Stage::from_u8(self.stars.stage[i]) != Stage::WhiteDwarf
                || self.stars.binary_id[i] == NO_BINARY
                || self.stars.age[i] < self.stars.lifetime[i]
            {
                continue;
            }
            let binary = self.stars.binary_id[i];
            if scheduled_binaries.contains(&binary) {
                continue;
            }
            let Some(j) = (0..self.stars.len()).find(|&j| {
                j != i
                    && self.stars.binary_id[j] == binary
                    && Stage::from_u8(self.stars.stage[j]) == Stage::WhiteDwarf
                    && self.stars.age[j] >= self.stars.lifetime[j]
            }) else {
                continue;
            };
            let combined_mass = self.stars.mass[i] + self.stars.mass[j];
            let x = (self.stars.pos_x[i] * self.stars.mass[i]
                + self.stars.pos_x[j] * self.stars.mass[j])
                / combined_mass;
            let y = (self.stars.pos_y[i] * self.stars.mass[i]
                + self.stars.pos_y[j] * self.stars.mass[j])
                / combined_mass;
            let cell = self.cell_index_at(x, y);
            self.events.emit_with_aux(
                tick,
                crate::events::EventKind::TypeIaSupernova,
                self.stars.id[i],
                cell as u32,
                combined_mass,
                self.stars.id[j] as f32,
                crate::events::NO_PARENT,
            );
            self.stars.stage[i] = Stage::Merging as u8;
            self.stars.stage[j] = Stage::Merging as u8;
            scheduled_binaries.push(binary);
        }
    }

    fn cell_index_at(&self, x: f32, y: f32) -> usize {
        let size = self.size as i32;
        let col = (x as i32).clamp(0, size - 1);
        let row = (y as i32).clamp(0, size - 1);
        (row * size + col) as usize
    }

    /// Deposit event ejecta into nearby cells, preserving momentum and
    /// composition even when a saturated destination accepts less mass.
    fn deposit_ejecta(
        &mut self,
        cell: usize,
        requested_mass: u16,
        requested_metals: f32,
        kick: f32,
        radius: i32,
    ) -> (f32, f32) {
        if cell >= self.n || requested_mass == 0 {
            return (0.0, 0.0);
        }
        let size = self.size as i32;
        let (center_col, center_row) = (cell as i32 % size, cell as i32 / size);
        let mut targets: Vec<usize> = Vec::new();
        for dr in -radius..=radius {
            for dc in -radius..=radius {
                if dc * dc + dr * dr > radius * radius {
                    continue;
                }
                let col = wrap(center_col + dc, size) as u16;
                let row = wrap(center_row + dr, size) as u16;
                targets.push(self.col_row_to_index(col, row) as usize);
            }
        }

        let share = requested_mass / targets.len() as u16;
        let remainder = requested_mass % targets.len() as u16;
        let mut deposits: Vec<(usize, u16)> = Vec::with_capacity(targets.len());
        let mut distributed = 0.0f32;
        for (order, target) in targets.into_iter().enumerate() {
            let requested = share + u16::from(order < remainder as usize);
            let old_mass = self.mass[target];
            let new_mass = old_mass.saturating_add(requested);
            let add = new_mass - old_mass;
            if add == 0 {
                continue;
            }
            let target_col = target as i32 % size;
            let target_row = target as i32 / size;
            let mut dx = (target_col - center_col) as f32;
            let mut dy = (target_row - center_row) as f32;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance > 1e-3 {
                dx /= distance;
                dy /= distance;
            } else {
                dx = 0.0;
                dy = 0.0;
            }
            let old = old_mass as f32;
            let new = new_mass as f32;
            self.vel_x[target] = (self.vel_x[target] * old + kick * dx * add as f32) / new;
            self.vel_y[target] = (self.vel_y[target] * old + kick * dy * add as f32) / new;
            self.mass[target] = new_mass;
            deposits.push((target, add));
            distributed += add as f32;
        }

        let deposited_metals = requested_metals.clamp(0.0, requested_mass as f32) * distributed
            / requested_mass as f32;
        if distributed > 0.0 {
            for (target, add) in deposits {
                self.metal_mass[target] = (self.metal_mass[target]
                    + deposited_metals * add as f32 / distributed)
                    .min(self.mass[target] as f32);
            }
        }
        (distributed, deposited_metals)
    }

    /// Supernova: return most of the star's mass to nearby gas with an
    /// outward kick, leave a neutron star, and emit ShockWave.
    fn handle_supernova(&mut self, ev: &Event) {
        let Some(i) = self.stars.index_of_id(ev.source) else {
            return;
        };
        let cell = ev.target as usize;
        if cell >= self.n {
            return;
        }
        let star_mass = self.stars.mass[i];
        let star_metals = self.stars.metal_mass[i];
        let ejected = (star_mass * Galaxy::SN_GAS_RETURN).floor() as u16;
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
        let share = ejected / targets.len() as u16;
        let remainder = ejected % targets.len() as u16;
        let mut distributed = 0.0f32;
        let mut deposits: Vec<(usize, u16)> = Vec::with_capacity(targets.len());
        for (order, &t) in targets.iter().enumerate() {
            let requested = share + u16::from(order < remainder as usize);
            if requested == 0 {
                continue;
            }
            let old_mass = self.mass[t];
            let new_mass = old_mass.saturating_add(requested);
            let add = new_mass - old_mass;
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
            self.mass[t] = new_mass;
            distributed += add as f32;
            deposits.push((t, add));
        }
        let inherited_ejecta_metals = if star_mass > 0.0 {
            star_metals * (distributed / star_mass).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let yield_metals = (star_mass * Galaxy::SN_METAL_YIELD_FRACTION)
            .min((distributed - inherited_ejecta_metals).max(0.0));
        let ejecta_metals = inherited_ejecta_metals + yield_metals;
        if distributed > 0.0 {
            for (target, add) in deposits {
                self.metal_mass[target] = (self.metal_mass[target]
                    + ejecta_metals * add as f32 / distributed)
                    .min(self.mass[target] as f32);
            }
        }
        self.metal_produced_total += yield_metals as f64;
        // Remnant keeps whatever the integer distribution left behind, so
        // the baryonic ledger stays closed exactly.
        self.stars.mass[i] = star_mass - distributed;
        self.stars.metal_mass[i] = (star_metals - inherited_ejecta_metals)
            .max(0.0)
            .min(self.stars.mass[i]);
        self.stars.stage[i] = Stage::NeutronStar as u8;
        self.stars.age[i] = 0.0;
        self.stars.lifetime[i] = if self.stars.binary_id[i] == NO_BINARY {
            Galaxy::NEUTRON_STAR_RESOLVED_AGE
        } else {
            self.neutron_star_merger_delay(self.stars.binary_id[i])
        };
        self.stars.luminosity[i] = Galaxy::NS_LUMINOSITY;
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

    /// A red giant returns its envelope gently and leaves a white dwarf.
    fn handle_planetary_nebula(&mut self, ev: &Event) {
        let Some(i) = self.stars.index_of_id(ev.source) else {
            return;
        };
        if Stage::from_u8(self.stars.stage[i]) != Stage::WhiteDwarf {
            return;
        }
        let star_mass = self.stars.mass[i];
        let star_metals = self.stars.metal_mass[i];
        let requested = (star_mass * (1.0 - Galaxy::WD_RETAINED_FRACTION)).floor() as u16;
        let inherited = if star_mass > 0.0 {
            star_metals * requested as f32 / star_mass
        } else {
            0.0
        };
        let (distributed, deposited_metals) = self.deposit_ejecta(
            ev.target as usize,
            requested,
            inherited,
            Galaxy::PLANETARY_NEBULA_KICK,
            Galaxy::PLANETARY_NEBULA_RADIUS,
        );
        self.stars.mass[i] = (star_mass - distributed).max(0.0);
        self.stars.metal_mass[i] = (star_metals - deposited_metals)
            .max(0.0)
            .min(self.stars.mass[i]);
    }

    /// A mature white-dwarf pair disrupts completely. Integer gas returns
    /// to the grid, any fractional or capacity-limited remainder enters the
    /// radiated sink, and thermonuclear burning adds an explicit metal yield.
    fn handle_type_ia_supernova(&mut self, ev: &Event) {
        let (Some(source), Some(target)) = (
            self.stars.index_of_id(ev.source),
            self.stars.index_of_id(ev.aux as u32),
        ) else {
            return;
        };
        if source == target
            || Stage::from_u8(self.stars.stage[source]) != Stage::Merging
            || Stage::from_u8(self.stars.stage[target]) != Stage::Merging
        {
            return;
        }

        let source_mass = self.stars.mass[source];
        let target_mass = self.stars.mass[target];
        let combined = source_mass + target_mass;
        if combined <= 0.0 {
            return;
        }
        let combined_metals = self.stars.metal_mass[source] + self.stars.metal_mass[target];
        let x = (self.stars.pos_x[source] * source_mass + self.stars.pos_x[target] * target_mass)
            / combined;
        let y = (self.stars.pos_y[source] * source_mass + self.stars.pos_y[target] * target_mass)
            / combined;
        let cell = self.cell_index_at(x, y);
        let requested = combined.floor().clamp(0.0, u16::MAX as f32) as u16;
        let inherited_ejecta = combined_metals * requested as f32 / combined;
        let yield_metals = (combined * Galaxy::TYPE_IA_METAL_YIELD_FRACTION)
            .min((requested as f32 - inherited_ejecta).max(0.0));
        let (distributed, deposited_metals) = self.deposit_ejecta(
            cell,
            requested,
            inherited_ejecta + yield_metals,
            Galaxy::TYPE_IA_KICK,
            Galaxy::TYPE_IA_RADIUS,
        );

        self.metal_produced_total += yield_metals as f64;
        self.radiated_total += (combined - distributed) as f64;
        self.radiated_metal_mass +=
            (combined_metals + yield_metals - deposited_metals).max(0.0) as f64;
        let hi = source.max(target);
        let lo = source.min(target);
        self.stars.swap_remove(hi);
        self.stars.swap_remove(lo);

        self.events.emit(
            self.tick_count,
            crate::events::EventKind::ShockWave,
            ev.source,
            cell as u32,
            Galaxy::TYPE_IA_KICK,
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

    /// Merge a resolved neutron-star pair into one compact remnant. A small
    /// fraction leaves the baryonic system as radiation, and the visible
    /// gamma-ray burst is emitted as a causal follow-up event.
    fn handle_neutron_star_merger(&mut self, ev: &Event) {
        let (Some(source), Some(target)) = (
            self.stars.index_of_id(ev.source),
            self.stars.index_of_id(ev.target),
        ) else {
            return;
        };
        if source == target
            || Stage::from_u8(self.stars.stage[source]) != Stage::Merging
            || Stage::from_u8(self.stars.stage[target]) != Stage::Merging
        {
            return;
        }

        let source_mass = self.stars.mass[source];
        let target_mass = self.stars.mass[target];
        let combined_metals = self.stars.metal_mass[source] + self.stars.metal_mass[target];
        let combined = source_mass + target_mass;
        if combined <= 0.0 {
            return;
        }
        let merged_x = (self.stars.pos_x[source] * source_mass
            + self.stars.pos_x[target] * target_mass)
            / combined;
        let merged_y = (self.stars.pos_y[source] * source_mass
            + self.stars.pos_y[target] * target_mass)
            / combined;
        let merged_vx = (self.stars.vel_x[source] * source_mass
            + self.stars.vel_x[target] * target_mass)
            / combined;
        let merged_vy = (self.stars.vel_y[source] * source_mass
            + self.stars.vel_y[target] * target_mass)
            / combined;
        let radiated = combined * Galaxy::GRB_RADIATED_FRACTION;
        let cell = self.cell_index_at(merged_x, merged_y) as u32;

        self.stars.pos_x[source] = merged_x;
        self.stars.pos_y[source] = merged_y;
        self.stars.vel_x[source] = merged_vx;
        self.stars.vel_y[source] = merged_vy;
        self.stars.mass[source] = combined - radiated;
        let radiated_metals = combined_metals * Galaxy::GRB_RADIATED_FRACTION;
        self.stars.metal_mass[source] =
            (combined_metals - radiated_metals).min(self.stars.mass[source]);
        self.stars.stage[source] = Stage::MergedRemnant as u8;
        self.stars.age[source] = 0.0;
        self.stars.lifetime[source] = Galaxy::REMNANT_RESOLVED_AGE;
        self.stars.luminosity[source] = Galaxy::NS_LUMINOSITY * 0.25;
        self.stars.binary_id[source] = NO_BINARY;
        self.stars.halo_dwell[source] = 0;
        self.radiated_total += radiated as f64;
        self.radiated_metal_mass += radiated_metals as f64;
        self.stars.swap_remove(target);

        self.events.emit(
            self.tick_count,
            crate::events::EventKind::GammaRayBurst,
            ev.source,
            cell,
            combined,
            ev.id,
        );
    }

    /// Deposit the short burst into the coarse radiation field. The event
    /// itself carries the renderer transient, while this splat briefly
    /// suppresses collapse in the surrounding gas.
    fn handle_gamma_ray_burst(&mut self, ev: &Event) {
        let cell = ev.target as usize;
        if cell >= self.n {
            return;
        }
        let res = Galaxy::FIELD_RES;
        let scale = res as f32 / self.size as f32;
        let fx = ((self.xs_i[cell] as f32 * scale) as usize).min(res - 1);
        let fy = ((self.ys_i[cell] as f32 * scale) as usize).min(res - 1);
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let x = fx as i32 + dx;
                let y = fy as i32 + dy;
                if x < 0 || y < 0 || x >= res as i32 || y >= res as i32 {
                    continue;
                }
                let weight = if dx == 0 && dy == 0 { 0.5 } else { 0.0625 };
                self.radiation[y as usize * res + x as usize] +=
                    Galaxy::GRB_RADIATION_BOOST * weight;
            }
        }
    }

    /// The black hole feeds: a fraction of nearby core gas accretes each
    /// run, and stars inside the capture radius are marked for capture
    /// (the swallow itself is a BlackHoleCapture event next tick).
    pub(crate) fn process_bh_accretion(&mut self, time: f32) {
        if self.bh_mass <= 0.0 {
            return;
        }
        let center = self.size as f32 * 0.5;
        let elapsed = time * 8.0;
        let p = self.scenario.params();
        let rc = p.halo_core_frac * self.disk_radius();
        let rc2 = rc * rc;
        let mut rng = self.rng_stream(Galaxy::RNG_BH_ACCRETION);
        for i in 0..self.n {
            let m = self.mass[i];
            if m == 0 {
                continue;
            }
            let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
            let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
            let r2 = x * x + y * y;
            let r = r2.sqrt();
            if r > Galaxy::BH_INFLOW_RADIUS {
                continue;
            }

            // Weak nuclear viscosity removes only tangential momentum.
            // The ring remains an orbiting structure while slowly leaking
            // low-angular-momentum gas into the two-cell sink.
            if r > 1e-3 {
                let radial_weight = 1.0 - r / Galaxy::BH_INFLOW_RADIUS;
                let damp = (-Galaxy::BH_NUCLEAR_VISCOSITY * elapsed * radial_weight).exp();
                let vr = (self.vel_x[i] * x + self.vel_y[i] * y) / r;
                let vt = (-self.vel_x[i] * y + self.vel_y[i] * x) / r * damp;
                self.vel_x[i] = vr * x / r - vt * y / r;
                self.vel_y[i] = vr * y / r + vt * x / r;
            }

            if r > Galaxy::BH_ACCRETION_RADIUS as f32 {
                continue;
            }
            let vc_halo = p.v_flat * r / (r2 + rc2).sqrt();
            let vc_bh = (Galaxy::GRAVATIONAL_CONSTANT * self.bh_mass / r.max(0.5)).sqrt();
            let circular_speed = (vc_halo * vc_halo + vc_bh * vc_bh).sqrt().max(0.1);
            let tangential_speed = if r > 1e-3 {
                (-self.vel_x[i] * y + self.vel_y[i] * x).abs() / r
            } else {
                0.0
            };
            let low_j = (1.0 - tangential_speed / circular_speed).clamp(0.1, 1.0);
            let expected = m as f32 * Galaxy::BH_ACCRETION_FRACTION * low_j;
            let whole = expected.floor() as u16;
            let fractional = expected - whole as f32;
            let stochastic = u16::from(rng.random_range(0.0f32..1.0) < fractional);
            let take = whole.saturating_add(stochastic).min(m);
            let metals = self.remove_cell_mass_with_metals(i, take);
            self.bh_mass += take as f32;
            self.bh_metal_mass += metals as f64;
        }

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
        let loss =
            (Galaxy::HAWKING_COEFF / (self.bh_mass * self.bh_mass) * elapsed).min(self.bh_mass);
        let metal_fraction = (self.bh_metal_mass / self.bh_mass as f64).clamp(0.0, 1.0);
        let lost_metals = (loss as f64 * metal_fraction).min(self.bh_metal_mass);
        self.bh_mass -= loss;
        self.radiated_total += loss as f64;
        self.bh_metal_mass -= lost_metals;
        self.radiated_metal_mass += lost_metals;
        if self.bh_mass < 1.0 {
            // Final flash: the last scrap evaporates entirely.
            self.radiated_total += self.bh_mass as f64;
            self.radiated_metal_mass += self.bh_metal_mass;
            self.bh_metal_mass = 0.0;
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
        self.bh_metal_mass += self.stars.metal_mass[i] as f64;
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
                crate::events::EventKind::NeutronStarMerger => self.handle_neutron_star_merger(&ev),
                crate::events::EventKind::GammaRayBurst => self.handle_gamma_ray_burst(&ev),
                crate::events::EventKind::PlanetaryNebula => self.handle_planetary_nebula(&ev),
                crate::events::EventKind::TypeIaSupernova => self.handle_type_ia_supernova(&ev),
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
        let mut metal_budget = 0.0f32;
        let take = |m: u16, frac: f32| -> u16 { (m as f32 * frac) as u16 };
        let own = take(self.mass[i], Galaxy::COLLAPSE_CONSUME_FRACTION);
        metal_budget += self.remove_cell_mass_with_metals(i, own);
        budget += own as f32;
        let (col, row) = (i as i32 % size, i as i32 / size);
        for (dc, dr) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let nc = wrap(col + dc, size) as u16;
            let nr = wrap(row + dr, size) as u16;
            let ni = self.col_row_to_index(nc, nr) as usize;
            let part = take(self.mass[ni], Galaxy::COLLAPSE_CONSUME_FRACTION * 0.5);
            metal_budget += self.remove_cell_mass_with_metals(ni, part);
            budget += part as f32;
        }
        if budget < Galaxy::BIRTH_MIN_BUDGET {
            // Fizzle: return the gas where it came from.
            self.mass[i] = self.mass[i].saturating_add(budget as u16);
            self.metal_mass[i] = (self.metal_mass[i] + metal_budget).min(self.mass[i] as f32);
            return;
        }
        let tick = self.tick_count;
        self.events.emit_with_aux(
            tick,
            crate::events::EventKind::StarBirth,
            ev.source,
            ev.source,
            budget,
            metal_budget,
            ev.id,
        );
    }

    /// Spawn a cluster of stars from the budget, masses drawn from the
    /// IMF (mostly red dwarfs, occasionally a giant), leftover folded
    /// into the heaviest draw so the masses sum to the budget exactly
    /// and the baryonic ledger stays closed. Nearby young births join one
    /// association. Each batch receives a shared galactic orbit plus a
    /// momentum-neutral internal spin, rather than independently adding
    /// circular speed to whatever radial motion the gas happened to carry.
    fn handle_star_birth(&mut self, ev: &Event) {
        let i = ev.target as usize;
        if i >= self.n {
            return;
        }
        let budget = ev.payload;
        let metallicity = if budget > 0.0 {
            (ev.aux / budget).clamp(0.0, 1.0)
        } else {
            0.0
        };
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
        // Split intermediate and core-collapse-scale births into equal
        // partners. Component mass distinguishes the later white-dwarf
        // and neutron-star channels without another per-star fate array.
        let mut expanded: Vec<f32> = Vec::with_capacity(Galaxy::BIRTH_MAX_STARS);
        let mut binary_ids: Vec<u32> = Vec::with_capacity(Galaxy::BIRTH_MAX_STARS);
        for mass in masses {
            if mass >= Galaxy::WD_BINARY_SPLIT_MASS && expanded.len() + 2 <= Galaxy::BIRTH_MAX_STARS
            {
                let binary = self.next_binary_id;
                self.next_binary_id = self.next_binary_id.wrapping_add(1);
                expanded.push(mass * 0.5);
                expanded.push(mass * 0.5);
                binary_ids.push(binary);
                binary_ids.push(binary);
            } else {
                expanded.push(mass);
                binary_ids.push(NO_BINARY);
            }
        }
        let masses = expanded;
        let n_stars = masses.len();

        let cx = self.xs_i[i] as f32;
        let cy = self.ys_i[i] as f32;
        let center = self.size as f32 * 0.5;
        let existing_associations = self.association_aggregates();
        let cluster = if let Some(cluster) = self.nearby_young_association(cx, cy) {
            cluster
        } else {
            let cluster = self.next_cluster_id;
            self.next_cluster_id = self.next_cluster_id.wrapping_add(1);
            cluster
        };
        let existing = existing_associations
            .get(cluster as usize)
            .copied()
            .filter(|association| association.mass > 0.0);

        let mut gas_vx = self.vel_x[i];
        let mut gas_vy = self.vel_y[i];
        let gas_speed = (gas_vx * gas_vx + gas_vy * gas_vy).sqrt();
        if gas_speed > Galaxy::BIRTH_GAS_VEL_CAP {
            let scale = Galaxy::BIRTH_GAS_VEL_CAP / gas_speed;
            gas_vx *= scale;
            gas_vy *= scale;
        }

        // Choose one center-of-mass orbit from the prograde gas flow plus
        // the azimuthally smoothed live stellar potential. Dense collapse
        // gas may be streaming inward, but only a small radial share
        // survives star formation. Separating the components preserves the
        // disk's real rotation without inheriting its radial plunge.
        let rx = cx - center;
        let ry = cy - center;
        let galactic_r = (rx * rx + ry * ry).sqrt().max(1e-3);
        let (radial_x, radial_y) = (rx / galactic_r, ry / galactic_r);
        let (tangent_x, tangent_y) = (-radial_y, radial_x);
        let gas_radial = gas_vx * radial_x + gas_vy * radial_y;
        let gas_tangential = gas_vx * tangent_x + gas_vy * tangent_y;
        let stellar_support =
            self.association_circular_speed(galactic_r) * Galaxy::ASSOCIATION_ORBIT_SUPPORT;
        let background_support = self.association_background_speed(galactic_r);
        // The smooth floor supplements the deliberately quarter-strength
        // live field. The cap keeps that compensation inside the old
        // gas-plus-circular-support birth envelope.
        let smooth_support = stellar_support + background_support;
        let target_tangential =
            (gas_tangential.max(0.0) + smooth_support).min(Galaxy::ASSOCIATION_ORBIT_SPEED_CAP);
        let orbital_radial = gas_radial * Galaxy::ASSOCIATION_RADIAL_INHERITANCE;
        let orbital_tangential = target_tangential;
        let orbital_vx = radial_x * orbital_radial + tangent_x * orbital_tangential;
        let orbital_vy = radial_y * orbital_radial + tangent_y * orbital_tangential;
        let (association_vx, association_vy) = if let Some(existing) = existing {
            let (old_vx, old_vy) = existing.velocity();
            let old = Galaxy::ASSOCIATION_EXISTING_VELOCITY_WEIGHT;
            (
                old_vx * old + orbital_vx * (1.0 - old),
                old_vy * old + orbital_vy * (1.0 - old),
            )
        } else {
            (orbital_vx, orbital_vy)
        };

        // A circular footprint reads as a cluster before the
        // binding process has integrated even one tick. Compact-binary
        // partners remain much closer than the broader association.
        let mut positions: Vec<(f32, f32)> = Vec::with_capacity(n_stars);
        for k in 0..n_stars {
            let paired_with_previous =
                k > 0 && binary_ids[k] != NO_BINARY && binary_ids[k] == binary_ids[k - 1];
            let (px, py) = if paired_with_previous {
                let previous = positions[k - 1];
                (
                    (previous.0 + rng.random_range(-0.18f32..0.18))
                        .clamp(0.0, self.size as f32 - 1e-3),
                    (previous.1 + rng.random_range(-0.18f32..0.18))
                        .clamp(0.0, self.size as f32 - 1e-3),
                )
            } else {
                let angle = rng.random_range(0.0f32..std::f32::consts::TAU);
                let radius =
                    Galaxy::ASSOCIATION_BIRTH_RADIUS * rng.random_range(0.0f32..1.0).sqrt();
                (
                    (cx + angle.cos() * radius).clamp(0.0, self.size as f32 - 1e-3),
                    (cy + angle.sin() * radius).clamp(0.0, self.size as f32 - 1e-3),
                )
            };
            positions.push((px, py));
        }

        let mut combined_mass = budget;
        let mut combined_x = positions
            .iter()
            .zip(masses.iter())
            .map(|(&(x, _), &mass)| x * mass)
            .sum::<f32>();
        let mut combined_y = positions
            .iter()
            .zip(masses.iter())
            .map(|(&(_, y), &mass)| y * mass)
            .sum::<f32>();
        if let Some(existing) = existing {
            combined_mass += existing.mass;
            combined_x += existing.weighted_x;
            combined_y += existing.weighted_y;
        }
        let association_x = combined_x / combined_mass.max(1e-3);
        let association_y = combined_y / combined_mass.max(1e-3);

        let mut internal_velocities: Vec<(f32, f32)> = Vec::with_capacity(n_stars);
        let mut internal_momentum_x = 0.0f32;
        let mut internal_momentum_y = 0.0f32;
        for (k, &(px, py)) in positions.iter().enumerate() {
            let dx = px - association_x;
            let dy = py - association_y;
            let local_r = (dx * dx + dy * dy).sqrt();
            let speed = if local_r > 1e-3 {
                (Galaxy::ASSOCIATION_BINDING_G * combined_mass / (local_r + 0.8)).sqrt()
                    * Galaxy::ASSOCIATION_INTERNAL_SPEED_SCALE
            } else {
                0.0
            }
            .min(Galaxy::ASSOCIATION_INTERNAL_SPEED_CAP);
            let (ivx, ivy) = if local_r > 1e-3 {
                (-dy / local_r * speed, dx / local_r * speed)
            } else {
                (0.0, 0.0)
            };
            internal_velocities.push((ivx, ivy));
            internal_momentum_x += masses[k] * ivx;
            internal_momentum_y += masses[k] * ivy;
        }
        let batch_mass = masses.iter().sum::<f32>().max(1e-3);
        let mean_internal_vx = internal_momentum_x / batch_mass;
        let mean_internal_vy = internal_momentum_y / batch_mass;

        for k in 0..n_stars {
            let mass = masses[k];
            let (px, py) = positions[k];
            let vx = association_vx + internal_velocities[k].0 - mean_internal_vx;
            let vy = association_vy + internal_velocities[k].1 - mean_internal_vy;
            let (mut lifetime, luminosity, class_index) = Galaxy::star_attrs(mass);
            if binary_ids[k] != NO_BINARY {
                lifetime = Galaxy::star_attrs(mass * 2.0).0;
            }
            let star_id = self.next_star_id;
            self.next_star_id += 1;
            self.stars.spawn(
                px,
                py,
                vx,
                vy,
                mass,
                mass * metallicity,
                lifetime,
                luminosity,
                class_index,
                cluster,
                binary_ids[k],
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

    /// Heavy elements carried by in-flight StarBirth events.
    pub(crate) fn pending_birth_metals(&self) -> f64 {
        self.events
            .pending()
            .filter(|e| e.kind == crate::events::EventKind::StarBirth)
            .map(|e| e.aux as f64)
            .sum()
    }

    /// Baryonic ledger: gas + resolved stars + in-flight births + the black
    /// hole + both halo reservoirs and the radiated sink.
    pub(crate) fn baryonic_total(&self) -> f64 {
        let gas: f64 = self.mass.iter().map(|&m| m as f64).sum();
        let stars: f64 = self.stars.mass.iter().map(|&m| m as f64).sum();
        gas + stars
            + self.pending_birth_mass()
            + self.halo_gas_mass as f64
            + self.stellar_halo_mass
            + self.bh_mass as f64
            + self.radiated_total
    }

    /// Composition ledger across every carrier and in-flight birth event.
    pub(crate) fn tracked_metal_total(&self) -> f64 {
        let gas: f64 = self.metal_mass.iter().map(|&m| m as f64).sum();
        let stars: f64 = self.stars.metal_mass.iter().map(|&m| m as f64).sum();
        gas + stars
            + self.pending_birth_metals()
            + self.halo_metal_mass
            + self.stellar_halo_metal_mass
            + self.bh_metal_mass
            + self.radiated_metal_mass
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

    fn neutron_star_merger_delay(&self, binary_id: u32) -> f32 {
        let hash = splitmix64(
            self.master_seed
                ^ (binary_id as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93)
                ^ 0xA409_3822_299F_31D0,
        );
        let unit = (hash as u32) as f32 / u32::MAX as f32;
        Galaxy::NS_MERGER_DELAY_MIN + unit * Galaxy::NS_MERGER_DELAY_SPAN
    }

    fn white_dwarf_merger_delay(&self, binary_id: u32) -> f32 {
        let hash = splitmix64(
            self.master_seed
                ^ (binary_id as u64).wrapping_mul(0x94D0_49BB_1331_11EB)
                ^ 0x243F_6A88_85A3_08D3,
        );
        let unit = (hash as u32) as f32 / u32::MAX as f32;
        Galaxy::WD_MERGER_DELAY_MIN + unit * Galaxy::WD_MERGER_DELAY_SPAN
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
        let p = self.scenario.params();
        // Flow relaxation replaces plain drag: velocity decays toward
        // the local circular flow u(r) = v_c(r) t_hat, not toward rest.
        // Same dt-scaled exponential, but the attractor state is a
        // rotating disk - stillness is no longer where the sim settles.
        let flow_decay = (-p.flow_drag * time).exp();
        // Halo rotation curve v_c(r) = v_flat r / sqrt(r^2 + rc^2).
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let rc = p.halo_core_frac * disk_r;
        let rc2 = rc * rc;
        let v_flat2 = p.v_flat * p.v_flat;
        // Halo centripetal pull plus the circular-boundary spring (see
        // CONFINE_STIFFNESS). Applied here, not in the force kernels, so
        // the CPU, Barnes-Hut, and WebGPU paths all get them for free.
        // The halo makes the circular flow an actual force equilibrium -
        // relaxation alone would re-aim velocities it cannot sustain.
        for i in 0..self.n {
            if self.mass[i] == 0 {
                continue;
            }
            let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
            let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
            let r2 = x * x + y * y;
            let r = r2.sqrt();
            if r < 1e-3 {
                continue;
            }
            // a = v_c^2 / r inward = v_flat^2 / (r^2 + rc^2) * r_vec.
            let ah = v_flat2 / (r2 + rc2);
            self.acc_x[i] -= ah * x;
            self.acc_y[i] -= ah * y;
            // Point-mass pull from the live central hole. Keeping this in
            // the shared integration step gives CPU, Barnes-Hut, and WebGPU
            // gas paths the same nuclear potential.
            if self.bh_mass > 0.0 {
                let softened = r2 + Galaxy::BH_GAS_SOFTENING_SQ;
                let ab = Galaxy::GRAVATIONAL_CONSTANT * self.bh_mass / (softened * softened.sqrt());
                self.acc_x[i] -= ab * x;
                self.acc_y[i] -= ab * y;
            }
            if r > disk_r {
                let k = Galaxy::CONFINE_STIFFNESS * (r - disk_r) / r;
                self.acc_x[i] -= k * x;
                self.acc_y[i] -= k * y;
            }
        }

        // Zero scratch; momentum accumulators are local per-tick.
        for m in self.scratch_mass.iter_mut() {
            *m = 0;
        }
        for metals in self.scratch_metal_mass.iter_mut() {
            *metals = 0.0;
        }
        let mut p_x = vec![0.0f32; self.n];
        let mut p_y = vec![0.0f32; self.n];
        let mut frac_next_x = vec![0.0f32; self.n];
        let mut frac_next_y = vec![0.0f32; self.n];

        // Pass 1: integrate velocity and record each cell's INTENDED
        // move. Movement is resolved in a second pass that knows every
        // resident's intent - resolving in one sequential sweep made a
        // full destination block its mover even when the resident was
        // itself leaving this tick, which froze every dense cloud solid
        // for motion toward higher indices (+x/+y). Bulk cloud motion -
        // a convoy of full cells advancing together - needs the intent
        // pass.
        let mut want_vx = vec![0.0f32; self.n];
        let mut want_vy = vec![0.0f32; self.n];
        let mut want_fx = vec![0.0f32; self.n];
        let mut want_fy = vec![0.0f32; self.n];
        let mut want_ni = vec![0u32; self.n];
        let mut want_sx = vec![0i8; self.n];
        let mut want_sy = vec![0i8; self.n];
        for i in 0..self.n {
            if self.mass[i] == 0 {
                // Empty cells: clear so stale values don't propagate later.
                self.vel_x[i] = 0.0;
                self.vel_y[i] = 0.0;
                self.frac_x[i] = 0.0;
                self.frac_y[i] = 0.0;
                self.metal_mass[i] = 0.0;
                want_ni[i] = i as u32;
                continue;
            }

            // v += a · dt
            let mut vx = self.vel_x[i] + self.acc_x[i] * time;
            let mut vy = self.vel_y[i] + self.acc_y[i] * time;

            // Relax toward the local circular flow. Doubles as the
            // energy sink the grid-quantized sim overheats without -
            // dissipation circularizes orbits instead of stopping them.
            let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
            let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
            let r2 = x * x + y * y;
            let r = r2.sqrt();
            let (ux, uy) = if r > 1e-3 {
                let vc = p.flow_support * p.v_flat * r / (r2 + rc2).sqrt();
                (-y / r * vc, x / r * vc)
            } else {
                (0.0, 0.0)
            };
            vx = ux + (vx - ux) * flow_decay;
            vy = uy + (vy - uy) * flow_decay;

            // Sub-grid position update. The step cap clamps the VECTOR
            // norm, not each axis: a per-axis clamp lets diagonal movers
            // travel sqrt(2) faster than axis-aligned ones, which
            // funnels every fast transit (bang ejecta) into four
            // diagonal sectors and shreds rings into a 4-blob pinwheel.
            let mut dx = vx * time;
            let mut dy = vy * time;
            let step_len_sq = dx * dx + dy * dy;
            if step_len_sq > max_step * max_step {
                let scale = max_step / step_len_sq.sqrt();
                dx *= scale;
                dy *= scale;
            }
            let mut fx = self.frac_x[i] + dx;
            let mut fy = self.frac_y[i] + dy;

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
            want_vx[i] = vx;
            want_vy[i] = vy;
            want_fx[i] = fx;
            want_fy[i] = fy;
            want_ni[i] = self.col_row_to_index(new_col, new_row) as u32;
            want_sx[i] = step_dx as i8;
            want_sy[i] = step_dy as i8;
        }

        // Pass 2: resolve admissions iteratively, like a traffic wave.
        // A mover is admitted only when its destination has room
        // counting residents CONFIRMED to be leaving - trusting mere
        // intent is over-permissive in a jam (everyone intends to move,
        // nobody actually can) and lets whole clouds collapse onto
        // their leading edge into one mega-blob. Iterating unwinds a
        // convoy from its free end: the front car pulls away, the next
        // fills the gap. Incompressibility semantics are unchanged - a
        // genuinely full destination still parks the mover at its cell
        // edge with velocity intact.
        const RESOLVE_ITERATIONS: usize = 3;
        // 0 = pending, 1 = moving (admitted), 2 = staying.
        let mut resolved = vec![0u8; self.n];
        let mut arrivals = vec![0u32; self.n];
        for i in 0..self.n {
            if self.mass[i] == 0 || want_ni[i] as usize == i {
                resolved[i] = 2;
            }
        }
        for _ in 0..RESOLVE_ITERATIONS {
            let mut progress = false;
            for i in 0..self.n {
                if resolved[i] != 0 {
                    continue;
                }
                let m = self.mass[i] as u32;
                let ni = want_ni[i] as usize;
                let resident = if resolved[ni] == 1 {
                    0
                } else {
                    self.mass[ni] as u32
                };
                let dest_occ = arrivals[ni].saturating_add(resident);
                if dest_occ == 0 || dest_occ.saturating_add(m) <= Galaxy::CELL_MASS_CAP {
                    resolved[i] = 1;
                    arrivals[ni] = arrivals[ni].saturating_add(m);
                    progress = true;
                }
            }
            if !progress {
                break;
            }
        }

        for i in 0..self.n {
            let m = self.mass[i];
            if m == 0 {
                continue;
            }
            let mut vx = want_vx[i];
            let mut vy = want_vy[i];
            let mut fx = want_fx[i];
            let mut fy = want_fy[i];
            let mut ni = want_ni[i] as usize;

            if resolved[i] == 0 {
                // Unresolved after iteration: blocked. Park at the cell
                // edge, velocity intact.
                ni = i;
                vx *= Galaxy::BLOCKED_FRICTION;
                vy *= Galaxy::BLOCKED_FRICTION;
                if want_sx[i] != 0 {
                    fx = 0.49 * want_sx[i] as f32;
                }
                if want_sy[i] != 0 {
                    fy = 0.49 * want_sy[i] as f32;
                }
            }

            // Merge: sum mass, accumulate momentum, keep the fraction of
            // the *arriving* cell (approx — good enough for visuals).
            let sum = self.scratch_mass[ni].saturating_add(m as u32);
            self.scratch_mass[ni] = sum;
            self.scratch_metal_mass[ni] += self.metal_mass[i];
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
                let retained = m32 as f32 / self.scratch_mass[i].max(1) as f32;
                self.metal_mass[i] = (self.scratch_metal_mass[i] * retained).clamp(0.0, mf);
                self.vel_x[i] = p_x[i] / mf;
                self.vel_y[i] = p_y[i] / mf;
                self.frac_x[i] = frac_next_x[i];
                self.frac_y[i] = frac_next_y[i];
            } else {
                self.vel_x[i] = 0.0;
                self.vel_y[i] = 0.0;
                self.frac_x[i] = 0.0;
                self.frac_y[i] = 0.0;
                self.metal_mass[i] = 0.0;
            }
            self.acc_x[i] = 0.0;
            self.acc_y[i] = 0.0;
        }

        self.transport_structured_gas(time);

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
                let source_mass = self.mass[i];
                let moved_metals = if source_mass == 0 {
                    0.0
                } else {
                    self.metal_mass[i] * moved as f32 / source_mass as f32
                };
                self.mass[ni] = new_m;
                self.mass[i] -= moved;
                self.metal_mass[i] = (self.metal_mass[i] - moved_metals)
                    .max(0.0)
                    .min(self.mass[i] as f32);
                self.metal_mass[ni] =
                    (self.metal_mass[ni] + moved_metals).min(self.mass[ni] as f32);
            }
        }
    }

    /// Resolve sub-cell isothermal pressure as a conservative mass flux.
    /// Grid parcels otherwise merge irreversibly whenever their paths meet,
    /// so acceleration alone cannot keep a diffuse cloud resolved. This
    /// exchange carries the source metal fraction and momentum exactly.
    fn transport_structured_gas(&mut self, time: f32) {
        let p = self.scenario.params();
        let diffusion = (p.gas_pressure * time).clamp(0.0, 0.45);
        let arm_transport = (p.spiral_arm_transport * time).clamp(0.0, 0.2);
        let ring_transport = (p.ring_transport * time).clamp(0.0, 0.2);
        if diffusion <= 0.0 && arm_transport <= 0.0 && ring_transport <= 0.0 {
            return;
        }

        let size = self.size as i32;
        let center = self.size as f32 * 0.5;
        let disk_r = self.disk_radius();
        let inner = disk_r * 0.12;
        let outer = disk_r * 0.94;
        let taper_width = disk_r * 0.12;
        let pattern_phase = p.spiral_pattern_step * self.tick_count as f32;
        let mut delta_mass = vec![0i32; self.n];
        for i in 0..self.n {
            self.scratch_mass[i] = self.mass[i] as u32;
            self.scratch_metal_mass[i] = 0.0;
            self.acc_x[i] = 0.0;
            self.acc_y[i] = 0.0;
        }
        let offsets = [(1, 0), (0, 1), (-1, 0), (0, -1)];

        for i in 0..self.n {
            let mass = self.scratch_mass[i] as u16;
            if mass < 2 {
                continue;
            }
            let col = i as i32 % size;
            let row = i as i32 / size;
            let neighbors = offsets.map(|(dc, dr)| {
                self.col_row_to_index(wrap(col + dc, size) as u16, wrap(row + dr, size) as u16)
                    as usize
            });
            let neighborhood_mass = neighbors
                .iter()
                .fold(mass as u32, |total, &ni| total + self.scratch_mass[ni]);
            let local_mean = (neighborhood_mass / 5) as u16;
            let excess = mass.saturating_sub(local_mean);
            let budget = (excess as f32 * diffusion).floor() as u16;
            let deficits =
                neighbors.map(|ni| local_mean.saturating_sub(self.scratch_mass[ni] as u16));
            let total_deficit: u32 = deficits.iter().map(|&deficit| deficit as u32).sum();
            let mut transfers = [0u16; 4];
            if budget > 0 && total_deficit > 0 {
                transfers = deficits.map(|deficit| {
                    ((budget as u32 * deficit as u32) / total_deficit).min(deficit as u32) as u16
                });
            }
            let mut allocated: u16 = transfers.iter().copied().sum();
            let start = (self.tick_count as usize + i) % offsets.len();
            while allocated < budget && total_deficit > 0 {
                let mut progressed = false;
                for step in 0..offsets.len() {
                    let k = (start + step) % offsets.len();
                    if transfers[k] < deficits[k] {
                        transfers[k] += 1;
                        allocated += 1;
                        progressed = true;
                        if allocated == budget {
                            break;
                        }
                    }
                }
                if !progressed {
                    break;
                }
            }

            // Cooling gas also drifts a small distance down the rotating
            // arm potential. This conservative sub-grid transport stops
            // parcel merging from erasing the density wave between cells.
            if arm_transport > 0.0 && allocated < mass {
                let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
                let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
                let r = (x * x + y * y).sqrt();
                if r > inner && r < outer {
                    let inner_t = ((r - inner) / taper_width).clamp(0.0, 1.0);
                    let outer_t = ((outer - r) / taper_width).clamp(0.0, 1.0);
                    let taper = inner_t
                        * inner_t
                        * (3.0 - 2.0 * inner_t)
                        * outer_t
                        * outer_t
                        * (3.0 - 2.0 * outer_t);
                    let source_phase =
                        2.0 * y.atan2(x) - Galaxy::SPIRAL_PITCH * r.ln() - pattern_phase;
                    let source_score = source_phase.cos();
                    let mut best = (source_score, start);
                    for step in 0..offsets.len() {
                        let k = (start + step) % offsets.len();
                        let ni = neighbors[k];
                        let nx = self.xs_i[ni] as f32 + self.frac_x[ni] - center;
                        let ny = self.ys_i[ni] as f32 + self.frac_y[ni] - center;
                        let nr = (nx * nx + ny * ny).sqrt().max(1.0);
                        let score =
                            (2.0 * ny.atan2(nx) - Galaxy::SPIRAL_PITCH * nr.ln() - pattern_phase)
                                .cos();
                        if score > best.0 {
                            best = (score, k);
                        }
                    }
                    let improvement = (best.0 - source_score).max(0.0);
                    let arm_budget =
                        (mass as f32 * arm_transport * taper * improvement * 0.5).round() as u16;
                    let arm_budget = arm_budget.min(mass - allocated);
                    transfers[best.1] = transfers[best.1].saturating_add(arm_budget);
                    allocated += arm_budget;
                }
            }

            // Ring gas drifts toward the annular potential minimum. The
            // score is radial only, so this cannot introduce an azimuthal
            // arm or bar into the axisymmetric scenario.
            if ring_transport > 0.0 && allocated < mass {
                let target = disk_r * p.ring_radius_frac;
                let x = self.xs_i[i] as f32 + self.frac_x[i] - center;
                let y = self.ys_i[i] as f32 + self.frac_y[i] - center;
                let source_distance = ((x * x + y * y).sqrt() - target).abs();
                let mut best = (source_distance, start);
                for step in 0..offsets.len() {
                    let k = (start + step) % offsets.len();
                    let ni = neighbors[k];
                    let nx = self.xs_i[ni] as f32 + self.frac_x[ni] - center;
                    let ny = self.ys_i[ni] as f32 + self.frac_y[ni] - center;
                    let distance = ((nx * nx + ny * ny).sqrt() - target).abs();
                    if distance < best.0 {
                        best = (distance, k);
                    }
                }
                let improvement = (source_distance - best.0).clamp(0.0, 1.0);
                let ring_budget = (mass as f32 * ring_transport * improvement).round() as u16;
                let ring_budget = ring_budget.min(mass - allocated);
                transfers[best.1] = transfers[best.1].saturating_add(ring_budget);
            }

            let metal_fraction = self.metal_mass[i] / mass as f32;
            for (k, &amount) in transfers.iter().enumerate() {
                if amount == 0 {
                    continue;
                }
                let ni = neighbors[k];
                let amount_i = amount as i32;
                let amount_f = amount as f32;
                let metals = metal_fraction * amount_f;
                let px = self.vel_x[i] * amount_f;
                let py = self.vel_y[i] * amount_f;
                delta_mass[i] -= amount_i;
                delta_mass[ni] += amount_i;
                self.scratch_metal_mass[i] -= metals;
                self.scratch_metal_mass[ni] += metals;
                self.acc_x[i] -= px;
                self.acc_x[ni] += px;
                self.acc_y[i] -= py;
                self.acc_y[ni] += py;
            }
        }

        for i in 0..self.n {
            if delta_mass[i] == 0
                && self.scratch_metal_mass[i] == 0.0
                && self.acc_x[i] == 0.0
                && self.acc_y[i] == 0.0
            {
                continue;
            }
            let old_mass = self.mass[i] as f32;
            let new_mass_i = self.mass[i] as i32 + delta_mass[i];
            debug_assert!(new_mass_i >= 0);
            let new_mass = new_mass_i.max(0) as u16;
            if new_mass == 0 {
                self.mass[i] = 0;
                self.metal_mass[i] = 0.0;
                self.vel_x[i] = 0.0;
                self.vel_y[i] = 0.0;
                self.frac_x[i] = 0.0;
                self.frac_y[i] = 0.0;
                continue;
            }
            let new_mass_f = new_mass as f32;
            self.mass[i] = new_mass;
            self.metal_mass[i] =
                (self.metal_mass[i] + self.scratch_metal_mass[i]).clamp(0.0, new_mass_f);
            self.vel_x[i] = (self.vel_x[i] * old_mass + self.acc_x[i]) / new_mass_f;
            self.vel_y[i] = (self.vel_y[i] * old_mass + self.acc_y[i]) / new_mass_f;
        }
        self.acc_x.fill(0.0);
        self.acc_y.fill(0.0);
        self.scratch_metal_mass.fill(0.0);
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
        // Invariant for `?seed=...` URL sharing across scenarios.
        for mode in [Scenario::IrregularSpiral, Scenario::BangSpiral] {
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
        let g = Galaxy::new(10, 0).seed_with_mode(0, Scenario::IrregularSpiral);
        assert!(g.vel_x.iter().all(|&v| v == 0.0));
        assert!(g.vel_y.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn test_seed_uniform_produces_tangential_velocity() {
        // Orbital rotation is baked into every mode, uniform included.
        let g = Galaxy::new(20, 0).seed_with_mode(5, Scenario::IrregularSpiral);
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
        let g = Galaxy::new(30, 0).seed_with_mode(1000, Scenario::BangSpiral);
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
        let g = Galaxy::new(30, 0).seed_with_mode(10, Scenario::IrregularSpiral);
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
    fn test_tick_with_accel_keeps_scenario_pressure_when_external_forces_are_zero() {
        // External acceleration replaces gravity only. Scenario-owned
        // pressure must still redistribute an isolated dense cell while
        // preserving the total mass ledger.
        let mut g = Galaxy::new(8, 2);
        g.scenario = Scenario::IrregularElliptical;
        g.mass[2 * 8 + 3] = 30;
        g.mass[5 * 8 + 5] = 40;
        let n = g.n;
        let zeros = vec![0.0f32; n];

        let no_force = g.tick_with_accel(0.5, &zeros, &zeros);

        assert_ne!(no_force.mass, g.mass);
        assert_eq!(
            no_force.mass.iter().map(|&mass| mass as u64).sum::<u64>(),
            g.mass.iter().map(|&mass| mass as u64).sum::<u64>()
        );
    }
}

#[cfg(test)]
mod tests_dynamics {
    use super::*;

    #[test]
    fn test_disabled_spiral_wave_leaves_acceleration_untouched() {
        let mut g = Galaxy::new(20, 0);
        g.scenario = Scenario::IrregularElliptical;
        let i = 10 * 20 + 15;
        g.mass[i] = 40;
        g.acc_x[i] = 0.25;
        g.acc_y[i] = -0.5;

        g.process_spiral_density_wave(0.5);

        assert_eq!(g.acc_x[i], 0.25);
        assert_eq!(g.acc_y[i], -0.5);
    }

    #[test]
    fn test_spiral_wave_accelerates_gas_toward_an_arm() {
        let mut g = Galaxy::new(40, 0);
        g.scenario = Scenario::IrregularSpiral;
        let center = g.size as f32 * 0.5;
        let disk_r = g.disk_radius();
        let i = (0..g.n)
            .find(|&i| {
                let x = g.xs_i[i] as f32 - center;
                let y = g.ys_i[i] as f32 - center;
                let r = (x * x + y * y).sqrt();
                if r < disk_r * 0.3 || r > disk_r * 0.8 {
                    return false;
                }
                let phase = 2.0 * y.atan2(x) - Galaxy::SPIRAL_PITCH * r.ln();
                phase.sin().abs() > 0.7
            })
            .expect("grid must contain an off-arm cell");
        g.mass[i] = 40;
        let x = g.xs_i[i] as f32 - center;
        let y = g.ys_i[i] as f32 - center;
        let r2 = x * x + y * y;
        let phase = 2.0 * y.atan2(x) - Galaxy::SPIRAL_PITCH * r2.sqrt().ln();
        let grad_x = (-2.0 * y - Galaxy::SPIRAL_PITCH * x) / r2;
        let grad_y = (2.0 * x - Galaxy::SPIRAL_PITCH * y) / r2;

        g.process_spiral_density_wave(0.5);

        let phase_acceleration = grad_x * g.acc_x[i] + grad_y * g.acc_y[i];
        assert!(
            phase.sin() * phase_acceleration < 0.0,
            "wave acceleration must reduce the arm phase error"
        );
    }

    #[test]
    fn test_ring_wave_accelerates_gas_toward_the_annulus() {
        let mut g = Galaxy::new(40, 0);
        g.scenario = Scenario::BangRing;
        let center = g.size as usize / 2;
        let inside = center * 40 + center + 5;
        let outside = center * 40 + center + 15;
        g.mass[inside] = 40;
        g.mass[outside] = 40;

        g.process_ring_density_wave(0.5);

        assert!(g.acc_x[inside] > 0.0, "inner gas must move outward");
        assert!(g.acc_x[outside] < 0.0, "outer gas must move inward");
        assert_eq!(g.acc_y[inside], 0.0);
        assert_eq!(g.acc_y[outside], 0.0);
    }

    #[test]
    fn test_structured_gas_transport_conserves_mass_metals_and_momentum() {
        for scenario in [Scenario::BangRing, Scenario::IrregularSpiral] {
            let mut g = Galaxy::new(20, 0);
            g.scenario = scenario;
            let i = 10 * 20 + 15;
            g.mass[i] = 100;
            g.metal_mass[i] = 30.0;
            g.vel_x[i] = 1.25;
            g.vel_y[i] = -0.4;
            let mass_before: u64 = g.mass.iter().map(|&mass| mass as u64).sum();
            let metals_before: f32 = g.metal_mass.iter().sum();
            let px_before: f32 = g
                .mass
                .iter()
                .enumerate()
                .map(|(i, &mass)| mass as f32 * g.vel_x[i])
                .sum();
            let py_before: f32 = g
                .mass
                .iter()
                .enumerate()
                .map(|(i, &mass)| mass as f32 * g.vel_y[i])
                .sum();

            g.transport_structured_gas(0.5);

            let mass_after: u64 = g.mass.iter().map(|&mass| mass as u64).sum();
            let metals_after: f32 = g.metal_mass.iter().sum();
            let px_after: f32 = g
                .mass
                .iter()
                .enumerate()
                .map(|(i, &mass)| mass as f32 * g.vel_x[i])
                .sum();
            let py_after: f32 = g
                .mass
                .iter()
                .enumerate()
                .map(|(i, &mass)| mass as f32 * g.vel_y[i])
                .sum();
            assert!(g.mass.iter().filter(|&&mass| mass > 0).count() > 1);
            assert_eq!(mass_after, mass_before);
            assert!((metals_after - metals_before).abs() < 1e-4);
            assert!((px_after - px_before).abs() < 1e-4);
            assert!((py_after - py_before).abs() < 1e-4);
        }
    }

    #[test]
    fn test_spiral_remains_resolved_star_forming_and_coherent_for_100_ticks() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::BangSpiral, 42);
        let mut start_births = None;
        let mut min_coherence = (f32::INFINITY, 0);
        let mut min_coverage = (f32::INFINITY, 0);
        let mut min_occupied = (usize::MAX, 0);
        for tick in 1..=1100 {
            g = g.tick(0.5);
            if tick < 1000 {
                continue;
            }
            let coherence = g.spiral_coherence();
            let coverage = g.spiral_coverage();
            let occupied = g.mass.iter().filter(|&&mass| mass > 0).count();
            let births = g.events_executed(crate::events::EventKind::StarBirth as u32);
            start_births.get_or_insert(births);
            if coherence < min_coherence.0 {
                min_coherence = (coherence, tick);
            }
            if coverage < min_coverage.0 {
                min_coverage = (coverage, tick);
            }
            if occupied < min_occupied.0 {
                min_occupied = (occupied, tick);
            }
        }
        assert!(
            min_coherence.0 >= 0.3,
            "tick {} coherence was {}",
            min_coherence.1,
            min_coherence.0
        );
        assert!(
            min_coverage.0 >= 0.5,
            "tick {} coverage was {}",
            min_coverage.1,
            min_coverage.0
        );
        assert!(
            min_occupied.0 >= 200,
            "tick {} had only {} gas cells",
            min_occupied.1,
            min_occupied.0
        );
        assert!(
            g.events_executed(crate::events::EventKind::StarBirth as u32)
                > start_births.expect("tick 1000 checkpoint"),
            "the coherent arm window must remain actively star-forming"
        );
    }

    #[test]
    fn test_irregular_spiral_settles_into_resolved_star_forming_arms_for_100_ticks() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularSpiral, 42);
        let mut start_births = None;
        let mut min_coherence = (f32::INFINITY, 0);
        let mut min_coverage = (f32::INFINITY, 0);
        let mut min_occupied = (usize::MAX, 0);
        for tick in 1..=2093 {
            g = g.tick(0.5);
            if tick < 1993 {
                continue;
            }
            let coherence = g.spiral_coherence();
            let coverage = g.spiral_coverage();
            let occupied = g.mass.iter().filter(|&&mass| mass > 0).count();
            let births = g.events_executed(crate::events::EventKind::StarBirth as u32);
            start_births.get_or_insert(births);
            if coherence < min_coherence.0 {
                min_coherence = (coherence, tick);
            }
            if coverage < min_coverage.0 {
                min_coverage = (coverage, tick);
            }
            if occupied < min_occupied.0 {
                min_occupied = (occupied, tick);
            }
        }
        assert!(
            min_coherence.0 >= 0.3,
            "tick {} coherence was {}",
            min_coherence.1,
            min_coherence.0
        );
        assert!(
            min_coverage.0 >= 0.5,
            "tick {} coverage was {}",
            min_coverage.1,
            min_coverage.0
        );
        assert!(
            min_occupied.0 >= 300,
            "tick {} had only {} gas cells",
            min_occupied.1,
            min_occupied.0
        );
        assert!(
            g.events_executed(crate::events::EventKind::StarBirth as u32)
                > start_births.expect("tick 1993 checkpoint"),
            "the settled irregular arm window must remain actively star-forming"
        );
    }

    #[test]
    fn test_irregular_elliptical_relaxes_into_a_resolved_pressure_supported_spheroid() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularElliptical, 42);
        let mut start_births = None;
        let mut start_mixed = None;
        let mut min_concentration = (f32::INFINITY, 0);
        let mut max_concentration = (0.0f32, 0);
        let mut min_smoothness = (f32::INFINITY, 0);
        let mut min_axis_ratio = (f32::INFINITY, 0);
        let mut min_extent = (f32::INFINITY, 0);
        let mut max_extent = (0.0f32, 0);
        let mut max_rotation = (0.0f32, 0);
        let mut min_gas_cells = (usize::MAX, 0);
        let mut min_stars = (usize::MAX, 0);
        let mut max_spiral = (0.0f32, 0);
        let mut max_ring = (0.0f32, 0);
        for tick in 1..=1000 {
            g = g.tick(0.5);
            if tick < 900 {
                continue;
            }
            let concentration = g.spheroid_concentration();
            let smoothness = g.spheroid_smoothness();
            let axis_ratio = g.spheroid_axis_ratio();
            let extent = g.spheroid_extent();
            let rotation = g.spheroid_rotational_support();
            let gas_cells = g.mass.iter().filter(|&&mass| mass > 0).count();
            let stars = g.stars.len();
            let spiral = g.spiral_coherence();
            let ring = g.ring_concentration();
            start_births
                .get_or_insert(g.events_executed(crate::events::EventKind::StarBirth as u32));
            start_mixed.get_or_insert(g.phase_mixed_count);
            if concentration < min_concentration.0 {
                min_concentration = (concentration, tick);
            }
            if concentration > max_concentration.0 {
                max_concentration = (concentration, tick);
            }
            if smoothness < min_smoothness.0 {
                min_smoothness = (smoothness, tick);
            }
            if axis_ratio < min_axis_ratio.0 {
                min_axis_ratio = (axis_ratio, tick);
            }
            if extent < min_extent.0 {
                min_extent = (extent, tick);
            }
            if extent > max_extent.0 {
                max_extent = (extent, tick);
            }
            if rotation > max_rotation.0 {
                max_rotation = (rotation, tick);
            }
            if gas_cells < min_gas_cells.0 {
                min_gas_cells = (gas_cells, tick);
            }
            if stars < min_stars.0 {
                min_stars = (stars, tick);
            }
            if spiral > max_spiral.0 {
                max_spiral = (spiral, tick);
            }
            if ring > max_ring.0 {
                max_ring = (ring, tick);
            }
        }
        assert!(
            min_concentration.0 >= 0.45 && max_concentration.0 <= 0.85,
            "stellar concentration ranged from {:?} to {:?}",
            min_concentration,
            max_concentration
        );
        assert!(
            min_smoothness.0 >= 0.7,
            "smoothness low at {min_smoothness:?}"
        );
        assert!(
            min_axis_ratio.0 >= 0.65,
            "axis ratio low at {min_axis_ratio:?}"
        );
        assert!(
            min_extent.0 >= 0.3 && max_extent.0 <= 0.65,
            "stellar extent ranged from {:?} to {:?}",
            min_extent,
            max_extent
        );
        assert!(max_rotation.0 <= 0.6, "rotation high at {max_rotation:?}");
        assert!(
            min_gas_cells.0 >= 150,
            "gas unresolved at {min_gas_cells:?}"
        );
        assert!(
            min_stars.0 >= 500,
            "stellar body unresolved at {min_stars:?}"
        );
        assert!(
            max_spiral.0 <= 0.35,
            "spiral signature high at {max_spiral:?}"
        );
        assert!(max_ring.0 <= 0.25, "ring signature high at {max_ring:?}");
        assert!(
            g.events_executed(crate::events::EventKind::StarBirth as u32)
                > start_births.expect("tick 900 birth checkpoint"),
            "star formation must continue while the spheroid settles"
        );
        assert!(
            g.phase_mixed_count > start_mixed.expect("tick 900 phase-mix checkpoint"),
            "the stellar body must continue phase-mixing"
        );
    }

    #[test]
    fn test_spheroid_metrics_distinguish_a_resolved_body_from_a_bar_and_point_mass() {
        let mut spheroid = Galaxy::new(40, 0);
        let center = 20.0;
        for index in 0..16 {
            let angle = std::f32::consts::TAU * index as f32 / 16.0;
            spheroid.spawn_star(
                center + 8.0 * angle.cos(),
                center + 8.0 * angle.sin(),
                0.0,
                0.0,
                5.0,
            );
        }
        assert!(spheroid.spheroid_smoothness() > 0.95);
        assert!(spheroid.spheroid_axis_ratio() > 0.95);
        assert!(spheroid.spheroid_extent() > 0.3);

        let mut bar = Galaxy::new(40, 0);
        for offset in [-8.0, -6.0, -4.0, -2.0, 2.0, 4.0, 6.0, 8.0] {
            bar.spawn_star(center + offset, center, 0.0, 0.0, 10.0);
        }
        assert!(bar.spheroid_smoothness() < 0.05);
        assert!(bar.spheroid_axis_ratio() < 0.05);

        let mut point_mass = Galaxy::new(40, 0);
        point_mass.spawn_star(center, center, 0.0, 0.0, 80.0);
        assert!(point_mass.spheroid_concentration() > 0.95);
        assert!(point_mass.spheroid_extent() < 0.01);
    }

    #[test]
    fn test_ring_remains_hollow_resolved_and_star_forming_for_100_ticks() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::BangRing, 42);
        let mut start_births = None;
        let mut min_concentration = (f32::INFINITY, 0);
        let mut min_depletion = (f32::INFINITY, 0);
        let mut min_coverage = (f32::INFINITY, 0);
        let mut max_width = (0.0f32, 0);
        let mut min_occupied = (usize::MAX, 0);
        for tick in 1..=1500 {
            g = g.tick(0.5);
            if tick < 1400 {
                continue;
            }
            let concentration = g.ring_concentration();
            let depletion = g.ring_core_depletion();
            let coverage = g.ring_coverage();
            let width = g.ring_width();
            let occupied = g.mass.iter().filter(|&&mass| mass > 0).count();
            let births = g.events_executed(crate::events::EventKind::StarBirth as u32);
            start_births.get_or_insert(births);
            if concentration < min_concentration.0 {
                min_concentration = (concentration, tick);
            }
            if depletion < min_depletion.0 {
                min_depletion = (depletion, tick);
            }
            if coverage < min_coverage.0 {
                min_coverage = (coverage, tick);
            }
            if width > max_width.0 {
                max_width = (width, tick);
            }
            if occupied < min_occupied.0 {
                min_occupied = (occupied, tick);
            }
        }
        assert!(
            min_concentration.0 >= 0.75,
            "tick {} ring concentration was {}",
            min_concentration.1,
            min_concentration.0
        );
        assert!(
            min_depletion.0 >= 0.95,
            "tick {} core depletion was {}",
            min_depletion.1,
            min_depletion.0
        );
        assert!(
            min_coverage.0 >= 0.5,
            "tick {} ring coverage was {}",
            min_coverage.1,
            min_coverage.0
        );
        assert!(
            max_width.0 <= 0.12,
            "tick {} ring width was {}",
            max_width.1,
            max_width.0
        );
        assert!(
            min_occupied.0 >= 250,
            "tick {} had only {} gas cells",
            min_occupied.1,
            min_occupied.0
        );
        assert!(
            g.events_executed(crate::events::EventKind::StarBirth as u32)
                > start_births.expect("tick 1400 checkpoint"),
            "the coherent ring window must remain actively star-forming"
        );
    }

    #[test]
    fn test_galactic_fountain_changes_direction_and_closes_ledger() {
        let mut g = Galaxy::new(30, 0).seed_with_mode_seeded(15, Scenario::IrregularSpiral, 42);

        // Start the controller at an even cold/hot split without
        // changing the baryonic ledger.
        let mut lifted = 0u64;
        for m in &mut g.mass {
            let take = *m / 2;
            *m -= take;
            lifted += take as u64;
        }
        g.halo_gas_mass += lifted;
        let ledger = g.baryonic_total();
        let midpoint = g.gas_cold_fraction();

        // Quarter-cycle targets 60% visible gas, so halo gas cools into
        // the disk. Three-quarter-cycle targets 40%, so gas lifts out.
        g.tick_count = Galaxy::FOUNTAIN_PERIOD / 4;
        g.process_gas_fountain(0.5);
        let rising = g.gas_cold_fraction();
        assert!(
            rising > midpoint,
            "cold reservoir must rise toward its 60% target"
        );

        g.tick_count = Galaxy::FOUNTAIN_PERIOD * 3 / 4;
        g.process_gas_fountain(0.5);
        let falling = g.gas_cold_fraction();
        assert!(
            falling < rising,
            "cold reservoir must fall toward its 40% target"
        );
        assert!(
            (g.baryonic_total() - ledger).abs() < 1.0,
            "fountain exchange must conserve baryonic mass"
        );
    }

    #[test]
    fn test_gas_dissipation_relaxes_toward_circular_flow_not_rest() {
        // Mechanism guard for the rotation reflow: dissipation must pull
        // velocity toward the local circular flow u(r), NOT toward zero
        // (the old plain drag froze every galaxy into a static blob by
        // t=1000). A lone cell at rest spins UP toward prograde
        // tangential motion, and the gap to the flow closes by exactly
        // exp(-flow_drag dt) per tick after the halo kick.
        let mut g = Galaxy::new(20, 0);
        // Isolate axisymmetric flow relaxation from the spiral scenarios'
        // additional non-axisymmetric density-wave force.
        g.scenario = Scenario::IrregularElliptical;
        // Cell left of center: x = -5, y = 0 relative to center (10,10).
        let idx = 10 * 20 + 5;
        g.mass[idx] = 50;
        let p = g.scenario.params();
        let disk_r = g.disk_radius();
        let rc = p.halo_core_frac * disk_r;
        let r = 5.0f32;
        // Prograde tangential direction at (-5, 0) is (0, -1) for the
        // seeded (-y, x) rotation sense... (-y, x)/r with y=0, x=-5 is
        // (0, -1) scaled - i.e. negative vy.
        let vc = p.flow_support * p.v_flat * r / (r * r + rc * rc).sqrt();
        g.process_gravity(0.5);
        g.process_integrate_gas(0.5);
        let total_mass = g.mass.iter().map(|&mass| mass as f32).sum::<f32>();
        let actual_vt = g
            .mass
            .iter()
            .enumerate()
            .map(|(i, &mass)| mass as f32 * g.vel_y[i])
            .sum::<f32>()
            / total_mass;
        // After one tick from rest: v = a_halo dt + (v0 + a dt - u) part;
        // radially the halo pulls +x (toward center), tangentially the
        // relaxation closes the gap from 0 toward -vc by (1 - decay).
        let decay = (-p.flow_drag * 0.5f32).exp();
        let expected_vt = -vc * (1.0 - decay);
        assert!(
            (actual_vt - expected_vt).abs() < 1e-3,
            "tangential relaxation must close the gap by 1-exp(-flow_drag dt): expected {expected_vt}, got {}",
            actual_vt
        );
        assert!(
            actual_vt < 0.0,
            "a cell at rest must spin up prograde, got vy={}",
            actual_vt
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

    /// Golden values pin the mass field after 100 ticks per scenario
    /// (size 50, dt 0.5). Last recaptured for the persistent spiral
    /// density wave and conservative gas-pressure transport.
    /// If another deliberate change lands, recapture and say so in the
    /// commit.
    #[test]
    fn test_golden_mass_field_per_scenario() {
        let cases = [
            (Scenario::BangRing, 7u64),
            (Scenario::BangSpiral, 7),
            (Scenario::IrregularSpiral, 42),
            (Scenario::IrregularElliptical, 42),
        ];
        let mut actual = Vec::new();
        for (mode, seed) in cases {
            let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, mode, seed);
            for _ in 0..100 {
                g = g.tick(0.5);
            }
            actual.push(mass_hash(&g));
        }
        assert_eq!(
            actual,
            vec![
                10864616295652119511u64,
                11971691907940808674,
                4260859134213941995,
                16074849645298020283,
            ]
        );
    }

    #[test]
    fn test_rng_streams_are_reproducible_and_independent() {
        let g = Galaxy::new(10, 0).seed_with_mode_seeded(5, Scenario::IrregularSpiral, 99);
        let mut a1 = g.rng_stream(1);
        let mut a2 = g.rng_stream(1);
        let mut b = g.rng_stream(2);
        let draws1: Vec<u32> = (0..8).map(|_| a1.random()).collect();
        let draws2: Vec<u32> = (0..8).map(|_| a2.random()).collect();
        let draws_b: Vec<u32> = (0..8).map(|_| b.random()).collect();
        assert_eq!(draws1, draws2, "same (seed, process, tick) must repeat");
        assert_ne!(
            draws1, draws_b,
            "different processes must not share a stream"
        );
        let g2 = g.tick(0.5);
        let mut a_next = g2.rng_stream(1);
        let draws_next: Vec<u32> = (0..8).map(|_| a_next.random()).collect();
        assert_ne!(draws1, draws_next, "streams must advance across ticks");
    }
}

#[cfg(test)]
mod tests_stars_dynamics {
    use super::*;

    fn fill_radial_star_field(g: &mut Galaxy, inward: f32) {
        let res = Galaxy::FIELD_RES;
        let cell = g.size as f32 / res as f32;
        let center = g.size as f32 * 0.5;
        for fy in 0..res {
            for fx in 0..res {
                let x = (fx as f32 + 0.5) * cell - center;
                let y = (fy as f32 + 0.5) * cell - center;
                let r = (x * x + y * y).sqrt().max(1e-3);
                g.field_ax[fy * res + fx] = -inward * x / r;
                g.field_ay[fy * res + fx] = -inward * y / r;
            }
        }
    }

    fn birth_event(target: usize, payload: f32) -> Event {
        Event {
            id: 1,
            tick: 1,
            seq: 0,
            kind: crate::events::EventKind::StarBirth,
            source: target as u32,
            target: target as u32,
            payload,
            aux: 0.0,
            parent: crate::events::NO_PARENT,
        }
    }

    #[test]
    fn test_star_at_rest_falls_toward_the_disk_center() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularSpiral, 42);
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
    fn test_nearby_births_share_one_supported_association_orbit() {
        let mut g = Galaxy::new(50, 0);
        g.master_seed = 17;
        fill_radial_star_field(&mut g, 0.02);
        let first_cell = 25 * 50 + 35;
        g.vel_x[first_cell] = -1.0;
        g.handle_star_birth(&birth_event(first_cell, 120.0));

        assert_eq!(g.next_cluster_id, 1);
        assert!(g.stars.len() >= Galaxy::ASSOCIATION_MIN_MEMBERS as usize);
        assert!(g.stars.cluster_id.iter().all(|&cluster| cluster == 0));
        let association = g.association_aggregates()[0];
        let (vx, vy) = association.velocity();
        assert!(
            vx.abs() < 0.2,
            "birth must suppress radial gas inflow, got vx={vx}"
        );
        assert!(
            vy > 0.35,
            "birth must receive prograde circular support, got vy={vy}"
        );

        let nearby_cell = 25 * 50 + 37;
        g.handle_star_birth(&birth_event(nearby_cell, 100.0));
        assert_eq!(
            g.next_cluster_id, 1,
            "nearby young collapse must join the existing association"
        );
        assert!(g.stars.cluster_id.iter().all(|&cluster| cluster == 0));

        let distant_cell = 25 * 50 + 10;
        g.handle_star_birth(&birth_event(distant_cell, 100.0));
        assert_eq!(
            g.next_cluster_id, 2,
            "a distant collapse must begin a new association"
        );
        assert!(g.stars.cluster_id.contains(&1));
    }

    #[test]
    fn test_association_binding_is_momentum_neutral_and_tides_strip_members() {
        let mut bound = Galaxy::new(50, 0);
        bound.next_cluster_id = 1;
        for (x, vx) in [(33.0, -0.2), (34.0, -0.1), (35.0, 0.1), (36.0, 0.2)] {
            let i = bound.spawn_star(x, 25.0, vx, 0.0, 10.0);
            bound.stars.cluster_id[i] = 0;
        }
        let momentum_before: f32 = bound
            .stars
            .mass
            .iter()
            .zip(bound.stars.vel_x.iter())
            .map(|(&mass, &vx)| mass * vx)
            .sum();
        bound.process_integrate_stars(0.5);
        let momentum_after: f32 = bound
            .stars
            .mass
            .iter()
            .zip(bound.stars.vel_x.iter())
            .map(|(&mass, &vx)| mass * vx)
            .sum();
        assert!(
            (momentum_after - momentum_before).abs() < 1e-5,
            "internal binding must not accelerate the association center"
        );
        assert!(bound.stars.vel_x[0] > -0.2);
        assert!(bound.stars.vel_x[3] < 0.2);

        let mut stripped = Galaxy::new(50, 0);
        stripped.next_cluster_id = 1;
        fill_radial_star_field(&mut stripped, 0.3);
        for x in [30.0, 30.0, 30.0, 34.0] {
            let i = stripped.spawn_star(x, 25.0, 0.0, 0.0, 10.0);
            stripped.stars.cluster_id[i] = 0;
            stripped.stars.age[i] = Galaxy::ASSOCIATION_TIDAL_GRACE + 1.0;
        }
        stripped.process_integrate_stars(0.5);
        assert_eq!(
            stripped
                .stars
                .cluster_id
                .iter()
                .filter(|&&cluster| cluster == NO_CLUSTER)
                .count(),
            1,
            "the exterior member must leave the association as a tidal-stream star"
        );
    }

    #[test]
    fn test_ejected_star_phase_mixes_before_the_hard_clip() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularSpiral, 42);
        let star_index = g.spawn_star(25.0, 25.0, 6.0, 0.0, 10.0);
        let star_id = g.stars.id[star_index];
        let ledger = g.baryonic_total();
        let soft = 24.0f32;
        let hard = soft * Galaxy::HARD_CLIP_FACTOR;
        let mut max_r = 0.0f32;
        let mut g = g;
        for _ in 0..4000 {
            g = g.tick(0.5);
            let Some(i) = g.stars.id.iter().position(|&id| id == star_id) else {
                break;
            };
            let r = (g.stars.pos_x[i] - 25.0).hypot(g.stars.pos_y[i] - 25.0);
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
        assert_eq!(g.stars.index_of_id(star_id), None);
        assert!(g.phase_mixed_count > 0);
        assert!(
            (g.baryonic_total() - ledger).abs() < 1.0,
            "phase mixing must move rather than destroy stellar mass"
        );
    }

    #[test]
    fn test_star_render_data_shape() {
        let mut g = Galaxy::new(20, 0).seed_with_mode_seeded(5, Scenario::IrregularSpiral, 1);
        g.spawn_star(10.0, 10.0, 0.1, 0.0, 42.0);
        g.spawn_star(5.0, 5.0, 0.0, 0.1, 7.0);
        g.stars.age[0] = 12.5;
        assert_eq!(g.star_count(), 2);
        let rd = g.star_render_data();
        assert_eq!(rd.len(), 2 * crate::stars::RENDER_FLOATS);
        assert_eq!(rd[0], 10.0);
        assert_eq!(rd[6], 12.5, "render snapshot must carry stellar age");
        assert!(rd[2] > rd[9], "heavier star must be more luminous");
        assert!(rd[5] >= 4_000_000_000.0, "debug stars have no association");
    }

    #[test]
    fn test_outer_star_phase_mixes_into_a_conserved_halo() {
        let mut g = Galaxy::new(50, 0);
        g.spawn_star(55.0, 25.0, 0.0, 0.0, 10.0);
        let ledger = g.baryonic_total();
        for _ in 0..Galaxy::STELLAR_HALO_DWELL {
            g.process_stellar_halo(0.5);
        }
        assert_eq!(g.star_count(), 0);
        assert_eq!(g.phase_mixed_count, 1);
        assert_eq!(g.stellar_halo_mass, 10.0);
        assert!((g.baryonic_total() - ledger).abs() < f64::EPSILON);
    }

    #[test]
    fn test_neutron_star_merger_emits_a_short_burst_and_closes_ledger() {
        let mut g = Galaxy::new(20, 0);
        g.master_seed = 7;
        let a = g.spawn_star(9.8, 10.0, 0.0, 0.1, 6.0);
        let b = g.spawn_star(10.2, 10.0, 0.0, -0.1, 6.0);
        for i in [a, b] {
            g.stars.stage[i] = Stage::NeutronStar as u8;
            g.stars.binary_id[i] = 4;
            g.stars.lifetime[i] = 1.0;
        }
        let ledger = g.baryonic_total();

        g.process_stellar_aging(0.5);
        assert!(g
            .stars
            .stage
            .iter()
            .all(|&stage| stage == Stage::Merging as u8));
        g.tick_count = 1;
        let mergers = g.events.take_due(1);
        assert_eq!(mergers.len(), 1);
        g.execute_events(mergers, 0.5);
        assert_eq!(g.star_count(), 1);
        assert_eq!(g.stars.stage[0], Stage::MergedRemnant as u8);
        assert_eq!(
            g.events
                .executed_count(crate::events::EventKind::NeutronStarMerger),
            1
        );

        g.tick_count = 2;
        let bursts = g.events.take_due(2);
        assert_eq!(bursts.len(), 1);
        assert_eq!(bursts[0].kind, crate::events::EventKind::GammaRayBurst);
        assert_ne!(bursts[0].parent, crate::events::NO_PARENT);
        g.execute_events(bursts, 0.5);
        assert_eq!(
            g.events
                .executed_count(crate::events::EventKind::GammaRayBurst),
            1
        );
        assert!(g.radiation.iter().copied().fold(0.0f32, f32::max) > 0.0);
        assert!((g.baryonic_total() - ledger).abs() < 1e-5);
    }

    #[test]
    fn test_low_mass_star_sheds_a_planetary_nebula_and_leaves_a_white_dwarf() {
        let mut g = Galaxy::new(20, 0);
        g.master_seed = 17;
        let star = g.spawn_star(10.0, 10.0, 0.0, 0.0, 10.0);
        g.stars.metal_mass[star] = 1.0;
        let baryons = g.baryonic_total();
        let metals = g.tracked_metal_total();

        g.stars.age[star] = g.stars.lifetime[star];
        g.process_stellar_aging(0.5);
        assert_eq!(g.stars.stage[star], Stage::RedGiant as u8);
        assert_eq!(g.red_giant_count(), 1);

        g.stars.age[star] = g.stars.lifetime[star];
        g.process_stellar_aging(0.5);
        assert_eq!(g.stars.stage[star], Stage::WhiteDwarf as u8);
        g.tick_count = 1;
        let nebulae = g.events.take_due(1);
        assert_eq!(nebulae.len(), 1);
        assert_eq!(nebulae[0].kind, crate::events::EventKind::PlanetaryNebula);
        g.execute_events(nebulae, 0.5);

        assert_eq!(g.white_dwarf_count(), 1);
        assert_eq!(
            g.events
                .executed_count(crate::events::EventKind::PlanetaryNebula),
            1
        );
        assert!(g.mass.iter().any(|&mass| mass > 0));
        assert!(g.stars.mass[0] < 10.0);
        assert!((g.baryonic_total() - baryons).abs() < 1e-5);
        assert!((g.tracked_metal_total() - metals).abs() < 1e-5);
    }

    #[test]
    fn test_white_dwarf_binary_type_ia_disrupts_and_enriches() {
        let mut g = Galaxy::new(20, 0);
        g.master_seed = 23;
        let a = g.spawn_star(9.8, 10.0, 0.0, 0.0, 2.0);
        let b = g.spawn_star(10.2, 10.0, 0.0, 0.0, 2.0);
        for i in [a, b] {
            g.stars.stage[i] = Stage::WhiteDwarf as u8;
            g.stars.binary_id[i] = 12;
            g.stars.lifetime[i] = g.white_dwarf_merger_delay(12);
            g.stars.age[i] = g.stars.lifetime[i];
            g.stars.metal_mass[i] = 0.2;
        }
        let baryons = g.baryonic_total();
        let metals = g.tracked_metal_total();

        g.process_stellar_aging(0.5);
        assert!(g
            .stars
            .stage
            .iter()
            .all(|&stage| stage == Stage::Merging as u8));
        g.tick_count = 1;
        let type_ia = g.events.take_due(1);
        assert_eq!(type_ia.len(), 1);
        assert_eq!(type_ia[0].kind, crate::events::EventKind::TypeIaSupernova);
        assert_eq!(type_ia[0].aux as u32, g.stars.id[b]);
        g.execute_events(type_ia, 0.5);

        assert_eq!(g.star_count(), 0);
        assert_eq!(
            g.events
                .executed_count(crate::events::EventKind::TypeIaSupernova),
            1
        );
        assert!(g.metal_produced_total > 0.0);
        assert!((g.baryonic_total() - baryons).abs() < 1e-5);
        assert!((g.tracked_metal_total() - metals - g.metal_produced_total).abs() < 1e-5);
        assert!(g
            .events
            .pending()
            .any(|event| event.kind == crate::events::EventKind::ShockWave));
    }

    #[test]
    fn test_white_dwarf_delay_is_seeded_and_repeatable() {
        let a = Galaxy::new(20, 0).seed_with_mode_seeded(5, Scenario::IrregularSpiral, 7);
        let b = Galaxy::new(20, 0).seed_with_mode_seeded(5, Scenario::IrregularSpiral, 7);
        let c = Galaxy::new(20, 0).seed_with_mode_seeded(5, Scenario::IrregularSpiral, 8);
        assert_eq!(a.white_dwarf_merger_delay(4), b.white_dwarf_merger_delay(4));
        assert_ne!(a.white_dwarf_merger_delay(4), c.white_dwarf_merger_delay(4));
    }

    #[test]
    fn test_sim_state_round_trip_preserves_star_evolution() {
        // The worker boundary contract: exporting gas + star + meta state
        // and rehydrating must continue the exact same trajectory.
        let mut a = Galaxy::new(30, 0).seed_with_mode_seeded(10, Scenario::IrregularSpiral, 9);
        a.spawn_star(20.0, 15.0, 0.0, 0.4, 30.0);
        a.spawn_star(10.0, 15.0, 0.0, -0.4, 60.0);
        for _ in 0..5 {
            a = a.tick(0.5);
        }
        let mut b = Galaxy::from_state(30, a.mass(), a.vel_x(), a.vel_y(), a.frac_x(), a.frac_y());
        b.restore_sim_state_stars(&a.sim_state_stars());
        b.restore_sim_state_field(&a.sim_state_field());
        b.restore_sim_state_meta(&a.sim_state_meta());
        for _ in 0..20 {
            a = a.tick(0.5);
            b = b.tick(0.5);
        }
        assert_eq!(
            a.stars.pos_x, b.stars.pos_x,
            "star x trajectories must match"
        );
        assert_eq!(
            a.stars.pos_y, b.stars.pos_y,
            "star y trajectories must match"
        );
        assert_eq!(a.mass, b.mass, "gas must match");
        assert_eq!(a.tick_count, b.tick_count);
    }
}

#[cfg(test)]
mod tests_composition {
    use super::*;
    use crate::events::{EventKind, NO_PARENT};

    fn root_event(kind: EventKind, cell: usize) -> Event {
        Event {
            id: 1,
            tick: 0,
            seq: 0,
            kind,
            source: cell as u32,
            target: cell as u32,
            payload: 0.0,
            aux: 0.0,
            parent: NO_PARENT,
        }
    }

    fn assert_carriers_bounded(g: &Galaxy) {
        for (&metals, &mass) in g.metal_mass.iter().zip(g.mass.iter()) {
            assert!(metals >= 0.0 && metals <= mass as f32 + 1e-6);
        }
        for (&metals, &mass) in g.stars.metal_mass.iter().zip(g.stars.mass.iter()) {
            assert!(metals >= 0.0 && metals <= mass + 1e-6);
        }
    }

    #[test]
    fn test_pressure_overflow_advects_metals_proportionally() {
        let mut g = Galaxy::new(10, 0);
        let cell = 5 * 10 + 5;
        g.mass[cell] = 140;
        g.metal_mass[cell] = 14.0;
        let initial = g.tracked_metal_total();

        g.apply_acceleration(0.0);

        assert_eq!(g.mass[cell], Galaxy::CELL_MASS_CAP as u16);
        assert!((g.tracked_metal_total() - initial).abs() < 1e-5);
        for (&metals, &mass) in g.metal_mass.iter().zip(g.mass.iter()) {
            if mass > 0 {
                assert!((metals / mass as f32 - 0.1).abs() < 1e-5);
            }
        }
        assert_carriers_bounded(&g);
    }

    #[test]
    fn test_collapse_and_birth_preserve_composition() {
        let mut g = Galaxy::new(20, 0);
        g.master_seed = 7;
        let cell = 10 * 20 + 10;
        g.mass[cell] = 100;
        g.metal_mass[cell] = 5.0;
        let initial = g.tracked_metal_total();

        g.handle_cloud_collapse(&root_event(EventKind::CloudCollapse, cell));
        let pending_metals = g.pending_birth_metals();
        assert!(pending_metals > 0.0);
        assert!((g.tracked_metal_total() - initial).abs() < 1e-6);

        g.tick_count = 1;
        let births = g.events.take_due(1);
        g.execute_events(births, 0.5);
        let stellar_metals: f64 = g.stars.metal_mass.iter().map(|&m| m as f64).sum();
        assert!((stellar_metals - pending_metals).abs() < 1e-5);
        assert!((g.tracked_metal_total() - initial).abs() < 1e-5);
        assert_carriers_bounded(&g);
    }

    #[test]
    fn test_supernova_adds_an_explicit_yield_without_adding_mass() {
        let mut g = Galaxy::new(20, 0);
        let cell = 10 * 20 + 10;
        let star = g.spawn_star(10.0, 10.0, 0.0, 0.0, 40.0);
        g.stars.metal_mass[star] = 0.4;
        let baryons = g.baryonic_total();
        let initial_metals = g.tracked_metal_total();
        let mut event = root_event(EventKind::Supernova, cell);
        event.source = g.stars.id[star];
        event.payload = g.stars.mass[star];

        g.handle_supernova(&event);

        assert!(g.metal_produced_total > 0.0);
        assert!((g.baryonic_total() - baryons).abs() < 1e-5);
        assert!((g.tracked_metal_total() - initial_metals - g.metal_produced_total).abs() < 1e-5);
        assert_carriers_bounded(&g);
    }

    #[test]
    fn test_composition_state_round_trip_is_exact() {
        let mut a = Galaxy::new(12, 3);
        a.metal_mass[7] = 0.75;
        a.halo_gas_mass = 10;
        a.halo_metal_mass = 0.4;
        a.stellar_halo_mass = 4.0;
        a.stellar_halo_metal_mass = 0.2;
        a.bh_mass = 6.0;
        a.bh_metal_mass = 0.3;
        a.radiated_total = 2.0;
        a.radiated_metal_mass = 0.1;
        a.metal_produced_total = 0.9;
        let star = a.spawn_star(4.0, 5.0, 0.0, 0.0, 8.0);
        a.stars.metal_mass[star] = 0.6;

        let mut b = Galaxy::from_state(12, a.mass(), a.vel_x(), a.vel_y(), a.frac_x(), a.frac_y());
        b.restore_sim_state_stars(&a.sim_state_stars());
        b.restore_sim_state_field(&a.sim_state_field());
        b.restore_sim_state_meta(&a.sim_state_meta());

        assert_eq!(b.metal_mass, a.metal_mass);
        assert_eq!(b.stars.metal_mass, a.stars.metal_mass);
        assert_eq!(b.halo_metal_mass, a.halo_metal_mass);
        assert_eq!(b.stellar_halo_metal_mass, a.stellar_halo_metal_mass);
        assert_eq!(b.bh_metal_mass, a.bh_metal_mass);
        assert_eq!(b.radiated_metal_mass, a.radiated_metal_mass);
        assert_eq!(b.metal_produced_total, a.metal_produced_total);
        assert_eq!(b.tracked_metal_total(), a.tracked_metal_total());
    }
}

#[cfg(test)]
mod tests_causal_loop {
    use super::*;
    use crate::events::EventKind;

    #[test]
    fn test_stars_form_unattended_from_cold_gas() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularSpiral, 42);
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
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularSpiral, 42);
        let initial = g.baryonic_total();
        let initial_metals = g.tracked_metal_total();
        for _ in 0..3000 {
            g = g.tick(0.5);
            let now = g.baryonic_total();
            assert!(
                (now - initial).abs() < 1.0,
                "ledger drifted at tick {}: {initial} -> {now}",
                g.tick_count
            );
            assert!(
                (g.tracked_metal_total() - initial_metals - g.metal_produced_total).abs() < 0.02,
                "composition ledger drifted at tick {}",
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
    fn test_seeded_lifecycle_reaches_phase_mixing_and_short_bursts() {
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularSpiral, 42);
        for _ in 0..4000 {
            g = g.tick(0.5);
            if g.phase_mixed_count > 0 && g.events.executed_count(EventKind::GammaRayBurst) > 0 {
                break;
            }
        }
        assert!(g.phase_mixed_count > 0, "seeded stars must phase-mix");
        assert!(
            g.events.executed_count(EventKind::GammaRayBurst) > 0,
            "seeded compact binaries must produce a short burst"
        );
    }

    #[test]
    fn test_determinism_same_seed_same_trajectory_at_two_depths() {
        // Same seed + same dt sequence -> identical star arrays and
        // event log, checked at two depths to catch cadence-boundary
        // nondeterminism. Both depths reach into the star-formation era.
        fn run(n: usize) -> (Vec<f32>, Vec<f32>, Vec<f32>, u64, [u64; 5]) {
            let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularSpiral, 42);
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
                g.metal_mass.clone(),
                g.stars.len() as u64,
                counts,
            )
        }
        for n in [900usize, 1800] {
            let a = run(n);
            let b = run(n);
            assert_eq!(a.3, b.3, "star count must be deterministic at n={n}");
            assert!(a.3 > 0, "depth n={n} must include star formation");
            assert_eq!(a.0, b.0, "star positions must be deterministic at n={n}");
            assert_eq!(a.1, b.1, "star velocities must be deterministic at n={n}");
            assert_eq!(a.2, b.2, "gas composition must be deterministic at n={n}");
            assert_eq!(a.4, b.4, "event log must be deterministic at n={n}");
        }
    }

    #[test]
    fn test_full_causal_chain_supernova_induces_star_birth() {
        // Construct the loop at one dense cell so the assertion tests event
        // ancestry instead of waiting for an incidental spatial overlap.
        use std::collections::HashMap;
        let mut g = Galaxy::new(20, 0);
        g.master_seed = 42;
        let cell = 10 * 20 + 10;
        g.mass[cell] = Galaxy::CELL_MASS_CAP as u16;
        for _ in 0..3 {
            let i = g.spawn_star(10.0, 10.0, 0.0, 0.0, 120.0);
            g.events.emit(
                0,
                EventKind::Supernova,
                g.stars.id[i],
                cell as u32,
                g.stars.mass[i],
                crate::events::NO_PARENT,
            );
        }

        g.tick_count = 1;
        let supernovae = g.events.take_due(1);
        g.execute_events(supernovae, 0.5);
        g.tick_count = 2;
        let shocks = g.events.take_due(2);
        g.execute_events(shocks, 0.5);
        g.mass[cell] = Galaxy::CELL_MASS_CAP as u16;
        g.radiation.fill(0.0);

        for attempt in 0..100u64 {
            g.tick_count = 3 + attempt * 16;
            g.process_collapse_watch(0.5);
            if g.events
                .pending()
                .any(|ev| ev.kind == EventKind::CloudCollapse)
            {
                break;
            }
        }
        let collapse_tick = g.tick_count + 1;
        g.tick_count = collapse_tick;
        let collapses = g.events.take_due(collapse_tick);
        assert!(
            !collapses.is_empty(),
            "shock-heated dense gas must collapse"
        );
        g.execute_events(collapses, 0.5);
        let birth_tick = g.tick_count + 1;
        g.tick_count = birth_tick;
        let births = g.events.take_due(birth_tick);
        assert!(!births.is_empty(), "collapse must schedule a star birth");
        g.execute_events(births, 0.5);

        let mut log: HashMap<u64, (EventKind, u64)> = HashMap::new();
        for ev in g.events.recent() {
            log.insert(ev.id, (ev.kind, ev.parent));
        }
        let found = log.iter().any(|(_, &(kind, parent))| {
            kind == EventKind::StarBirth
                && matches!(log.get(&parent), Some(&(EventKind::CloudCollapse, gp))
                    if matches!(log.get(&gp), Some(&(EventKind::ShockWave, ggp))
                        if matches!(log.get(&ggp), Some(&(EventKind::Supernova, _)))))
        });
        assert!(
            found,
            "missing StarBirth <- CloudCollapse <- ShockWave <- Supernova ancestry \
             (events: col={} birth={} sn={} shock={})",
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
        let mut g = Galaxy::new(50, 0).seed_with_mode_seeded(25, Scenario::IrregularSpiral, 42);
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
        let mut g = Galaxy::new(20, 0).seed_with_mode_seeded(2, Scenario::IrregularSpiral, 7);
        let bh0 = g.bh_mass_value();
        assert!(
            bh0 > 0.0 && bh0 < 100.0,
            "test wants a small hole, got {bh0}"
        );
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

    #[test]
    fn test_black_hole_pulls_gas_and_preserves_a_leaking_nuclear_ring() {
        let mut falling = Galaxy::new(20, 0);
        falling.bh_mass = 500.0;
        let outer = 10 * 20 + 14;
        falling.mass[outer] = 20;
        falling.apply_acceleration(0.5);
        let occupied = falling
            .mass
            .iter()
            .position(|&mass| mass > 0)
            .expect("falling gas");
        assert!(
            falling.vel_x[occupied] < 0.0,
            "central mass must pull gas inward"
        );

        let mut g = Galaxy::new(20, 0);
        g.bh_mass = 500.0;
        let ring_cell = 10 * 20 + 14;
        let inner_cell = 10 * 20 + 11;
        g.mass[ring_cell] = 100;
        g.mass[inner_cell] = 10;
        g.vel_y[ring_cell] = 1.0;
        let bh_before = g.bh_mass;
        for tick in 1..=64 {
            g.tick_count = tick;
            g.process_bh_accretion(0.5);
        }
        assert!(
            g.mass[ring_cell] > 90,
            "nuclear viscosity must preserve the orbiting ring on short runs"
        );
        assert!(
            g.bh_mass > bh_before,
            "the inner nucleus must still leak inward"
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

        let mut rehydrated = Galaxy::from_state(
            8,
            mass.clone(),
            vx.clone(),
            vy.clone(),
            fx.clone(),
            fy.clone(),
        );
        rehydrated.restore_sim_state_stars(&g.sim_state_stars());
        rehydrated.restore_sim_state_field(&g.sim_state_field());
        rehydrated.restore_sim_state_meta(&g.sim_state_meta());

        assert_eq!(rehydrated.mass, mass);
        assert_eq!(rehydrated.vel_x, vx);
        assert_eq!(rehydrated.vel_y, vy);
        assert_eq!(rehydrated.frac_x, fx);
        assert_eq!(rehydrated.frac_y, fy);

        // Ticking the fully rehydrated galaxy should produce the same next
        // state. Gas arrays alone no longer suffice because the live black
        // hole participates in gas acceleration.
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

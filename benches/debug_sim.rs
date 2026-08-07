//! Native physics probe: seeded runs of every scenario with structure
//! metrics at checkpoints.
//! `cargo run --bin debug_sim [ticks] [size] [seeds] [start-seed]`.
//! Iterating on sim constants goes through this harness, not the browser -
//! same kernel, no webpack in the loop.

use galaxy_gen_backend::galaxy::{Galaxy, Scenario};
use galaxy_gen_backend::process;
use galaxy_gen_backend::stars::STAR_FLOATS;

struct Metrics {
    nonzero: usize,
    total: u64,
    /// Mass-weighted mean tangential velocity, r in [0.2, 0.7] disk_r.
    /// Positive = prograde. The headline "is it still rotating" number.
    vt: f64,
    /// vt / mean speed in the same annulus - 1.0 is pure circular flow.
    rot_support: f64,
    /// Radius of the peak radial-profile bin, as a fraction of disk_r.
    /// Ring signature: high r_peak with a low central fraction.
    r_peak_frac: f64,
    /// Gas mass fraction inside 0.25 disk_r. Elliptical: high. Ring: low.
    central_frac: f64,
    /// m=2 azimuthal Fourier amplitude over the disk - arms/lobes.
    m2: f64,
    /// Pitch-aware logarithmic m=2 amplitude - rejects bars and blobs.
    spiral: f64,
    /// Fraction of radial bands carrying the same pitched arm phase.
    spiral_coverage: f64,
    /// Gas mass fraction inside the scenario's target annulus.
    ring_concentration: f64,
    /// Visible gas fraction outside the hollow ring core.
    ring_core_depletion: f64,
    /// Fraction of populated azimuthal sectors in the target annulus.
    ring_coverage: f64,
    /// Radial RMS distance from the target annulus, in disk radii.
    ring_width: f64,
    /// Star population tangential velocity (same annulus) - the stars
    /// must rotate too, not just the gas.
    star_vt: f64,
    /// Fraction of stars inside 0.3 disk_r - the elliptical is mostly
    /// a star glow, so its concentration lives here, not in the gas.
    star_central: f64,
    /// Mean disk-star tangential speed over the circular speed of the
    /// field they read. 1.0 is a balanced disk; above 1.0 the population
    /// carries more angular momentum than its potential holds.
    star_circ_ratio: f64,
    /// v_rot / sigma over the disk - the metric that actually separates
    /// a rotating disk (>1.5) from a pressure-supported mush (<0.7).
    v_over_sigma: f64,
    /// The same ratio a newborn is handed at 0.5 disk_r. Separates
    /// "born wrong" from "drifted wrong".
    birth_circ_ratio: f64,
    /// Resolved stellar spheroid measurements.
    spheroid_concentration: f64,
    spheroid_smoothness: f64,
    spheroid_axis_ratio: f64,
    spheroid_extent: f64,
    spheroid_rotational_support: f64,
    bh_growth: f64,
    bh_accretion_rate: f64,
    quasar_activity: f64,
    quasar_episodes: u32,
}

fn metrics(g: &Galaxy, size: u16) -> Metrics {
    let mass = g.mass();
    let vx = g.vel_x();
    let vy = g.vel_y();
    let s = size as usize;
    let c = size as f64 * 0.5;
    let disk_r = size as f64 * 0.5 - 1.0;

    let (mut nonzero, mut total) = (0usize, 0u64);
    let (mut vt_num, mut vt_den) = (0f64, 0f64);
    let mut speed_num = 0f64;
    let (mut m2_re, mut m2_im, mut m2_den) = (0f64, 0f64, 0f64);
    let mut central = 0f64;
    const BINS: usize = 24;
    let mut profile = [0f64; BINS];

    for (i, &m) in mass.iter().enumerate() {
        if m == 0 {
            continue;
        }
        nonzero += 1;
        total += m as u64;
        let mf = m as f64;
        let x = (i % s) as f64 - c;
        let y = (i / s) as f64 - c;
        let r = (x * x + y * y).sqrt();
        let rf = r / disk_r;
        if rf < 1.0 {
            profile[(rf * BINS as f64) as usize] += mf;
        }
        if rf < 0.25 {
            central += mf;
        }
        if r > 3.0 && rf < 1.0 {
            let theta = y.atan2(x);
            m2_re += mf * (2.0 * theta).cos();
            m2_im += mf * (2.0 * theta).sin();
            m2_den += mf;
        }
        if (0.2..=0.7).contains(&rf) {
            vt_num += mf * (x * vy[i] as f64 - y * vx[i] as f64) / r;
            speed_num += mf * ((vx[i] as f64).powi(2) + (vy[i] as f64).powi(2)).sqrt();
            vt_den += mf;
        }
    }

    // Star tangential velocity from the full flat state:
    // [x, y, vx, vy, ...] per STAR_FLOATS chunk.
    let star_flat = g.sim_state_stars();
    let (mut svt_num, mut svt_den) = (0f64, 0f64);
    let (mut s_central, mut s_total) = (0f64, 0f64);
    for chunk in star_flat.chunks_exact(STAR_FLOATS) {
        let x = chunk[0] as f64 - c;
        let y = chunk[1] as f64 - c;
        let r = (x * x + y * y).sqrt();
        let rf = r / disk_r;
        s_total += 1.0;
        if rf < 0.3 {
            s_central += 1.0;
        }
        if (0.2..=0.7).contains(&rf) {
            svt_num += (x * chunk[3] as f64 - y * chunk[2] as f64) / r;
            svt_den += 1.0;
        }
    }

    let peak_bin = profile
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap_or(0);
    Metrics {
        nonzero,
        total,
        vt: if vt_den > 0.0 { vt_num / vt_den } else { 0.0 },
        rot_support: if speed_num > 0.0 {
            vt_num / speed_num
        } else {
            0.0
        },
        r_peak_frac: (peak_bin as f64 + 0.5) / BINS as f64,
        central_frac: if total > 0 {
            central / total as f64
        } else {
            0.0
        },
        m2: if m2_den > 0.0 {
            (m2_re * m2_re + m2_im * m2_im).sqrt() / m2_den
        } else {
            0.0
        },
        spiral: g.spiral_coherence() as f64,
        spiral_coverage: g.spiral_coverage() as f64,
        ring_concentration: g.ring_concentration() as f64,
        ring_core_depletion: g.ring_core_depletion() as f64,
        ring_coverage: g.ring_coverage() as f64,
        ring_width: g.ring_width() as f64,
        star_vt: if svt_den > 0.0 {
            svt_num / svt_den
        } else {
            0.0
        },
        star_central: if s_total > 0.0 {
            s_central / s_total
        } else {
            0.0
        },
        star_circ_ratio: g.star_circular_ratio() as f64,
        v_over_sigma: g.rotation_dispersion_ratio() as f64,
        birth_circ_ratio: g.birth_circular_ratio(0.5) as f64,
        spheroid_concentration: g.spheroid_concentration() as f64,
        spheroid_smoothness: g.spheroid_smoothness() as f64,
        spheroid_axis_ratio: g.spheroid_axis_ratio() as f64,
        spheroid_extent: g.spheroid_extent() as f64,
        spheroid_rotational_support: g.spheroid_rotational_support() as f64,
        bh_growth: g.bh_growth_factor() as f64,
        bh_accretion_rate: g.bh_accretion_rate() as f64,
        quasar_activity: g.quasar_activity() as f64,
        quasar_episodes: g.quasar_episode_count(),
    }
}

fn main() {
    let max_ticks: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1000);
    let size: u16 = std::env::args()
        .nth(2)
        .and_then(|a| a.parse().ok())
        .unwrap_or(150);
    let n_seeds: u64 = std::env::args()
        .nth(3)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1);
    let start_seed: u64 = std::env::args()
        .nth(4)
        .and_then(|a| a.parse().ok())
        .unwrap_or(12345);
    let scenario_filter: Option<usize> = std::env::args().nth(5).and_then(|a| a.parse().ok());
    let checkpoints = [
        0usize, 250, 500, 900, 1000, 1100, 1200, 1500, 2000, 2300, 2400, 2500, 2600, 2700, 3000,
        4000, 5000,
    ];

    for (scenario_index, (mode, name)) in [
        (Scenario::BangRing, "bang=>ring"),
        (Scenario::BangSpiral, "bang=>spiral"),
        (Scenario::IrregularSpiral, "irregular=>spiral"),
        (Scenario::IrregularElliptical, "irregular=>elliptical"),
    ]
    .into_iter()
    .enumerate()
    {
        if scenario_filter.is_some_and(|filter| filter != scenario_index) {
            continue;
        }
        if n_seeds == 1 {
            // Per-process timing profile at the seeded state (native only -
            // the wasm build has no monotonic clock without JS interop).
            let probe = Galaxy::new(size, 0).seed_with_mode_seeded(25, mode, 12345);
            print!("profile[{name}]:");
            for p in process::registry() {
                let mut clone = probe.clone();
                let t0 = std::time::Instant::now();
                (p.run)(&mut clone, 0.5);
                print!("  {}={:.2}ms", p.name, t0.elapsed().as_secs_f64() * 1000.0);
            }
            println!();
        }
        for seed in start_seed..start_seed.saturating_add(n_seeds) {
            let mut g = Galaxy::new(size, 0).seed_with_mode_seeded(25, mode, seed);
            println!("--- {name} (size={size}, seed={seed}, dt=0.5) ---");
            let mut done = 0usize;
            for &cp in checkpoints.iter().filter(|&&c| c <= max_ticks) {
                while done < cp {
                    g = g.tick(0.5);
                    done += 1;
                }
                let m = metrics(&g, size);
                println!(
                    "t={cp:5}  nz={:5}  gas={:6}  vt={:+.3}  rot={:+.2}  r_pk={:.2}  ctr={:.2}  m2={:.2}  spi={:.2}  cov={:.2}  ring={:.2}  hollow={:.2}  rcov={:.2}  rw={:.2}  svt={:+.3}  sctr={:.2}  vsig={:.2}  scirc={:.2}  bcirc={:.2}  econ={:.2}  esm={:.2}  axis={:.2}  ext={:.2}  erot={:.2}  bh={:.2}  q={:.2}  qep={}  qrate={:.5}  stars={:5}  mixed={:5}  ev(col/b/sn/sh/d/cap/nsm/grb/pn/ia/q)={}/{}/{}/{}/{}/{}/{}/{}/{}/{}/{}",
                    m.nonzero,
                    m.total,
                    m.vt,
                    m.rot_support,
                    m.r_peak_frac,
                    m.central_frac,
                    m.m2,
                    m.spiral,
                    m.spiral_coverage,
                    m.ring_concentration,
                    m.ring_core_depletion,
                    m.ring_coverage,
                    m.ring_width,
                    m.star_vt,
                    m.star_central,
                    m.v_over_sigma,
                    m.star_circ_ratio,
                    m.birth_circ_ratio,
                    m.spheroid_concentration,
                    m.spheroid_smoothness,
                    m.spheroid_axis_ratio,
                    m.spheroid_extent,
                    m.spheroid_rotational_support,
                    m.bh_growth,
                    m.quasar_activity,
                    m.quasar_episodes,
                    m.bh_accretion_rate,
                    g.star_count(),
                    g.phase_mixed_count(),
                    g.events_executed(0),
                    g.events_executed(1),
                    g.events_executed(2),
                    g.events_executed(3),
                    g.events_executed(4),
                    g.events_executed(5),
                    g.events_executed(6),
                    g.events_executed(7),
                    g.events_executed(8),
                    g.events_executed(9),
                    g.events_executed(10),
                );
            }
        }
    }
}

//! Native physics probe: seeded runs of every initial condition with
//! structure metrics at checkpoints. `cargo run --bin debug_sim [ticks]`.
//! Iterating on sim constants goes through this harness, not the browser -
//! same kernel, no webpack in the loop.

use galaxy_gen_backend::galaxy::{Galaxy, InitialCondition};

struct Metrics {
    nonzero: usize,
    max: u16,
    total: u64,
    rms_radius: f64,
    /// Mass-weighted mean tangential velocity in the r ∈ [5, 12] annulus.
    /// Wrap-safe rotation signal (global L_z is meaningless on a torus).
    annulus_vt: f64,
}

fn metrics(g: &Galaxy, size: u16) -> Metrics {
    let mass = g.mass();
    let vx = g.vel_x();
    let vy = g.vel_y();
    let s = size as usize;
    let c = size as f64 * 0.5;

    let (mut nonzero, mut max, mut total) = (0usize, 0u16, 0u64);
    let (mut mx, mut my) = (0f64, 0f64);
    for (i, &m) in mass.iter().enumerate() {
        if m == 0 {
            continue;
        }
        nonzero += 1;
        max = max.max(m);
        total += m as u64;
        mx += (i % s) as f64 * m as f64;
        my += (i / s) as f64 * m as f64;
    }
    if total == 0 {
        return Metrics {
            nonzero,
            max,
            total,
            rms_radius: 0.0,
            annulus_vt: 0.0,
        };
    }
    mx /= total as f64;
    my /= total as f64;

    let mut rms = 0f64;
    let (mut vt_num, mut vt_den) = (0f64, 0f64);
    for (i, &m) in mass.iter().enumerate() {
        if m == 0 {
            continue;
        }
        let x = (i % s) as f64;
        let y = (i / s) as f64;
        let (dx, dy) = (x - mx, y - my);
        rms += m as f64 * (dx * dx + dy * dy);

        let (cx, cy) = (x - c, y - c);
        let r = (cx * cx + cy * cy).sqrt();
        if (5.0..=12.0).contains(&r) {
            vt_num += m as f64 * (cx * vy[i] as f64 - cy * vx[i] as f64) / r;
            vt_den += m as f64;
        }
    }
    Metrics {
        nonzero,
        max,
        total,
        rms_radius: (rms / total as f64).sqrt(),
        annulus_vt: if vt_den > 0.0 { vt_num / vt_den } else { 0.0 },
    }
}

fn main() {
    let max_ticks: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(4000);
    let size = 50u16;
    let checkpoints = [0usize, 100, 500, 1000, 2000, 4000, 8000];

    for (mode, name) in [
        (InitialCondition::Uniform, "uniform"),
        (InitialCondition::Bang, "bang"),
    ] {
        let mut g = Galaxy::new(size, 0).seed_with_mode_seeded(25, mode, 12345);
        println!("--- {name} (size={size}, seed=12345, dt=0.5) ---");
        let mut done = 0usize;
        for &cp in checkpoints.iter().filter(|&&c| c <= max_ticks) {
            while done < cp {
                g = g.tick(0.5);
                done += 1;
            }
            let m = metrics(&g, size);
            println!(
                "t={cp:5}  nz={:4}  max={:5}  total={:6}  rms_r={:5.1}  vt={:+.3}  stars={:4}  ev(col/birth/sn/shock/diss)={}/{}/{}/{}/{}",
                m.nonzero,
                m.max,
                m.total,
                m.rms_radius,
                m.annulus_vt,
                g.star_count(),
                g.events_executed(0),
                g.events_executed(1),
                g.events_executed(2),
                g.events_executed(3),
                g.events_executed(4),
            );
        }
    }
}

// Per-process tick profiler. Run: cargo run --release --bin perf_profile
//
// `tick_bench` answers "how slow is a tick". This answers "which process
// inside the tick is slow", which is the question that says where effort
// belongs. It drives `Galaxy::tick_instrumented`, which is the real
// scheduler with a timer around each phase, so the attribution follows
// the declared causal chain instead of a sampled guess.
//
// Size, tick count, warmup, and seed come from argv, so a run can target
// the size and the regime under investigation without a recompile:
//   cargo run --release --bin perf_profile -- 500 40
//   cargo run --release --bin perf_profile -- 500 40 1000   # mature
//   cargo run --release --bin perf_profile -- 500 40 1000 424242

use galaxy_gen_backend::galaxy::{Galaxy, Scenario};
use galaxy_gen_backend::process;
use std::time::{Duration, Instant};

/// Clears the seeding transient only - far too few to reach a galaxy
/// with stars in it. Override from argv; see docs/perf-rewrite.md.
const WARMUP_TICKS: u32 = 8;

const DT: f32 = 0.5;

/// `seed()` draws a fresh master seed per process, so a mature warmup
/// varies run to run. Held fixed, matching the e2e perf specs.
const FIXED_SEED: u64 = 424_242;

fn profile(size: u16, ticks: u32, seed_mass: u16, warmup: u32, seed: u64) {
    let mut g = Galaxy::new(size, 0);
    g = g.seed_with_mode_seeded(seed_mass, Scenario::IrregularSpiral, seed);
    for _ in 0..warmup {
        g = g.tick(DT);
    }

    let registry = process::registry();
    let mut totals = vec![Duration::ZERO; registry.len() + 2];
    let mut sample = Vec::new();

    let t_all = Instant::now();
    for _ in 0..ticks {
        g = g.tick_instrumented(DT, &mut sample);
        for (slot, v) in totals.iter_mut().zip(sample.iter()) {
            *slot += *v;
        }
    }
    let per_tick_ms = t_all.elapsed().as_secs_f64() * 1000.0 / ticks as f64;

    println!(
        "\nsize={size}  cells={}  stars={}  ticks={ticks}  per_tick={per_tick_ms:.2}ms  \
         ticks/s={:.1}",
        (size as u32).pow(2),
        g.star_count(),
        1000.0 / per_tick_ms,
    );

    let mut rows: Vec<(String, f64)> =
        vec![("clone (immutable tick)".into(), ms(totals[0], ticks))];
    for (i, p) in registry.iter().enumerate() {
        if totals[i + 1].is_zero() {
            continue;
        }
        rows.push((
            format!("{} (cadence {})", p.name, p.cadence),
            ms(totals[i + 1], ticks),
        ));
    }
    rows.push(("events".into(), ms(totals[registry.len() + 1], ticks)));
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));

    for (name, v) in &rows {
        println!(
            "  {name:38} {v:8.3} ms/tick  {:5.1}%",
            100.0 * v / per_tick_ms
        );
    }
}

fn ms(d: Duration, ticks: u32) -> f64 {
    d.as_secs_f64() * 1000.0 / ticks as f64
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let sizes: Vec<u16> = match args.first() {
        Some(s) => vec![s.parse().expect("size must be a u16")],
        None => vec![250, 500],
    };
    let ticks: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let warmup: u32 = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(WARMUP_TICKS);
    let seed: u64 = args
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(FIXED_SEED);

    println!("== galaxy-gen per-process profile ==  seed={seed}");
    for size in sizes {
        profile(size, ticks, 25, warmup, seed);
    }
}

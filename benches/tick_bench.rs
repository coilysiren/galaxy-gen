// Native perf bench: build a galaxy, run N ticks, report wall-clock.
// Run with: cargo run --release --bin tick_bench
// (Added as a [[bin]] in Cargo.toml.)

use galaxy_gen_backend::galaxy::Galaxy;
use std::time::Instant;

fn bench(size: u16, ticks: u32, seed_mass: u16) {
    let mut g = Galaxy::new(size, 0);
    g = g.seed(seed_mass);

    // Warmup
    g = g.tick(0.01);

    let t0 = Instant::now();
    for _ in 0..ticks {
        g = g.tick(0.01);
    }
    let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
    let per_tick = elapsed_ms / ticks as f64;
    println!(
        "size={:4}  cells={:7}  {:4} ticks  total={:8.2}ms  per_tick={:8.2}ms  ticks/s={:6.1}",
        size,
        (size as u32).pow(2),
        ticks,
        elapsed_ms,
        per_tick,
        1000.0 / per_tick,
    );

    // Keep `g` alive so the compiler can't elide work.
    std::hint::black_box(&g);
}

fn main() {
    println!("== galaxy-gen tick bench ==");
    bench(20, 20, 50);
    bench(50, 10, 50);
    bench(75, 5, 50);
    bench(100, 5, 50);
    bench(150, 3, 50);
    bench(250, 2, 50);
}

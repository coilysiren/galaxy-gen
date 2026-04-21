use galaxy_gen_backend::galaxy::Galaxy;

fn main() {
    let size = 20u16;
    let mut g = Galaxy::new(size, 0);
    g = g.seed(25);
    let before = g.mass();
    let before_sum: u32 = before.iter().map(|&m| m as u32).sum();
    println!("size={}, initial mass sum: {}", size, before_sum);
    println!("initial[0..10]: {:?}", &before[0..10]);

    for i in 0..50 {
        g = g.tick(0.5);
        if i % 5 == 0 || i == 49 {
            let m = g.mass();
            let s: u32 = m.iter().map(|&x| x as u32).sum();
            let non_zero = m.iter().filter(|&&x| x > 0).count();
            println!(
                "tick {:2}: sum={}, non_zero_cells={}, first10={:?}",
                i,
                s,
                non_zero,
                &m[0..10]
            );
        }
    }
    let after = g.mass();
    let changed = before.iter().zip(after.iter()).filter(|(a, b)| a != b).count();
    println!("changed cells after 50 ticks: {} / {}", changed, before.len());
}

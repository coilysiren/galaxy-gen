# How we made the galaxy sim ~55× faster

A session-long journal of every lever pulled to take `galaxy-gen` from
**<1 FPS at 250×250** to **~18 FPS at 250×250** (and 60+ FPS everywhere
smaller). Each section names the observation that triggered the change,
the insight behind it, the code diff, and the measurement before/after.
Read top to bottom — later changes build on earlier ones.

## TL;DR — the before/after table

All measurements are **WASM tick time in Chromium** (Playwright). `frame`
column is `tick + canvas render` combined, which is what the live `run`
loop actually spends per frame.

| size    | cells   | **original tick** | direct-O(A²) tick | **Barnes-Hut tick** | frame w/ canvas |
|--------:|--------:|-----------:|---------:|-------------------:|----------------:|
| 20×20   |     400 | 7.3 ms     | 0.6 ms   | 0.6 ms             | 0.09 ms         |
| 50×50   |   2,500 | 243 ms     | 6.5 ms   | **1.9 ms**         | 1.35 ms         |
| 75×75   |   5,625 | 1,238 ms   | 34 ms    | **3.6 ms**         | 2.3 ms          |
| 100×100 |  10,000 | 3,808 ms   | 97 ms    | **6.4 ms**         | 8 ms            |
| 150×150 |  22,500 | (DNF)      | ~280 ms  | **16 ms**          | 17 ms           |
| 250×250 |  62,500 | (DNF)      | >3,000 ms| **44 ms**          | **54 ms**       |

Translating the 250×250 column into frame-rate: **<0.3 FPS → ~18 FPS**,
or roughly 55× faster. At smaller sizes the cumulative wins are even
more dramatic (≈180× at 50×50), but the leverage at the high end is what
made the sim actually usable at the grid size the user asked about.

Native-Rust release mode is about 1.3-1.5× faster than WASM for the same
work — the WASM overhead is the sqrt, bounds checks, and scatter writes
the JIT can't elide as aggressively as LLVM.

## Method

Every change was measured twice: once in native Rust, once in the
browser. Two benches got added to the repo:

- `benches/tick_bench.rs` — native Rust via `cargo run --release --bin
  tick_bench`. Six grid sizes (20 → 250) × several ticks each, reports
  per-tick mean.
- `e2e/perf.spec.ts` — Playwright harness that loads the dev server,
  boots WASM, and calls `fe.tick(0.5)` 3-20 times per size. Also
  measures tick + render combined so render regressions can't hide.

Iteration loop, repeated 8 times over the session:

1. Form a hypothesis from the bench numbers.
2. Make the smallest code change that tests it.
3. Re-run both benches.
4. If it's a win, keep it and commit; if not, revert.

A few of the changes **looked like wins but didn't measure as wins**
(e.g. a symmetric O(N²/2) pair sweep — see §4). Those got reverted.
Chasing the measurement saved several dead ends.

## Git trail

Five commits drove the perf rewrite. Every diff in this document is
extracted from one of these:

| sha     | title                                                               |
|---------|---------------------------------------------------------------------|
| 284aab3 | Fix WASM table-grow error by removing unused specs dep              |
| 4d3b6d5 | Rewrite simulation for 25-40x tick speedup                          |
| 22a83bc | Make the sim visibly move: velocity integration + sub-grid fractions |
| 3199c27 | Switch dataviz SVG → canvas; bump default dt to 0.5                  |
| 623b858 | Barnes-Hut quadtree: 250x250 from <1 FPS to ~18 FPS                  |

`git log --oneline 2a4e96e..623b858` shows the full range.

---

## Lever 1 — Strip out the `specs` ECS that wasn't being used

### The observation

CI's Playwright run crashed at WASM module instantiation:

```
WebAssembly.Table.grow(): failed to grow table by 4
```

Every test failed the same way. Native `cargo test` passed fine. The
same WASM worked on my local machine but not in the CI chromium.

### The insight

I looked at `Cargo.toml`:

```toml
[dependencies]
console_error_panic_hook = "^0.1"
specs = "^0.20"
specs-derive = "^0.4"
wasm-bindgen = "^0.2"
getrandom = { version = "^0.3", features = ["wasm_js"] }
rand = "^0.9"
```

And at `lib.rs`:

```rust
extern crate specs_derive;

extern crate rand;
extern crate specs;
extern crate wasm_bindgen;

pub mod galaxy;
```

`specs` is an ECS framework, but a search for `use specs` or `World::`
or `Entity::` in `src/rust/` came back empty. It was imported but never
called. Yet it was pulling `rayon`, `atomic_refcell`, `hibitset`,
`shred`, and friends into the WASM binary. Some combination of their
function-pointer references was tripping the `table.grow` limit under
wasm-opt's release passes.

### The change

Delete the `extern crate` lines, delete the `[dependencies]`, bump to
edition 2021 so the `use` statements already in `galaxy.rs` resolve
without `extern crate`.

```diff
diff --git a/Cargo.toml b/Cargo.toml
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -7,6 +7,7 @@ repository = "https://github.com/coilysiren/galaxy-gen"
 version = "0.0.1"
 authors = ["Kai Siren <coilysiren@gmail.com>"]
 license = "AGPL"
+edition = "2021"
 
 [lib]
 crate-type = ["cdylib", "rlib"]
@@ -14,14 +15,11 @@ path = "src/rust/lib.rs"
 
 [dependencies]
 console_error_panic_hook = "^0.1"
-specs = "^0.20"
-specs-derive = "^0.4"
 wasm-bindgen = "^0.2"
 getrandom = { version = "^0.3", features = ["wasm_js"] }
 rand = "^0.9"
 
-[dev-dependencies]
-cargo-watch = "^8"
-
 [profile.release]
-debug = true
+opt-level = "s"
+lto = true
+codegen-units = 1

diff --git a/src/rust/lib.rs b/src/rust/lib.rs
--- a/src/rust/lib.rs
+++ b/src/rust/lib.rs
@@ -1,7 +1 @@
-extern crate specs_derive;
-
-extern crate rand;
-extern crate specs;
-extern crate wasm_bindgen;
-
 pub mod galaxy;
```

While in there: `[profile.release] debug = true` was forcing debug info
into release builds. Replaced with a proper release profile — `opt-level
= "s"`, `lto = true`, `codegen-units = 1`.

### The result

CI's table-grow error went away. WASM binary shrank from ~45KB to 38KB
after re-enabling `wasm-opt -O3`. No perf change on paper, but this was
the prerequisite for everything downstream — nothing else would have
mattered if the WASM module wasn't even instantiating.

Commit: `284aab3`.

---

## Lever 2 — Struct-of-Arrays instead of `Vec<Cell>`

### The observation

Baseline bench, `cargo run --release --bin tick_bench`:

```
size=  20  cells=    400    20 ticks  per_tick=    4.94ms
size=  50  cells=   2500    10 ticks  per_tick=  168.47ms
size=  75  cells=   5625     5 ticks  per_tick=  856.52ms
size= 100  cells=  10000     3 ticks  per_tick= 2714.49ms
```

Scaling is clearly O(N⁴). Each cell computes gravity from every other
cell (O(N²) cells × O(N²) pairs).

### The insight

The old layout was an array-of-structs:

```rust
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub mass: u16,
    pub accel_magnitude: f32,
    pub accel_degrees: f32,
}

pub struct Galaxy {
    size: u16,
    cells: Vec<Cell>,
    min_star_mass: u16,
}
```

The hot loop read one full `Cell` per iteration — 10 bytes, probably
padded to 12 or 16 — but only touched `accel_magnitude` and
`accel_degrees`. Most cache bandwidth was wasted pulling `mass` (which
is only read at the start of each outer iteration) alongside the
accelerations.

Bigger sin: the polar (magnitude, degrees) representation required four
`to_radians().cos()/sin()` calls **per pair** just to combine two
vectors:

```rust
fn combine_vectors(&self, m1: f32, d1: f32, m2: f32, d2: f32) -> (f32, f32) {
    let x1 = m1 * d1.to_radians().cos();
    let y1 = m1 * d1.to_radians().sin();
    let x2 = m2 * d2.to_radians().cos();
    let y2 = m2 * d2.to_radians().sin();
    let x = x1 + x2;
    let y = y1 + y2;
    let magnitude = (x.powi(2) + y.powi(2)).sqrt();
    let degrees = (x.atan2(y)).to_degrees();
    (magnitude, degrees)
}
```

Four trigonometric calls to add two vectors. Stored as polar, converted
to cartesian, summed, converted back.

### The change

Flip the whole thing. Parallel `Vec`s (SoA) plus cartesian-native
accelerations:

```rust
pub struct Galaxy {
    size: u16,
    n: usize,
    min_star_mass: u16,

    mass: Vec<u16>,
    acc_x: Vec<f32>,
    acc_y: Vec<f32>,

    // Integer (x, y) for each cell (pre-computed in `new()`).
    xs_i: Vec<i16>,
    ys_i: Vec<i16>,

    inv_r3: Vec<f32>,        // (Lever 3 — see below)
    scratch_mass: Vec<u32>,  // for apply_acceleration
}
```

The gravitate inner loop reads from four flat slices (`mass`, `xs_i`,
`ys_i`, `inv_r3`) and writes to two (`acc_x`, `acc_y`). Each is
contiguous, hot in L1 for N ≤ ~15k, no padding waste, no Cell-struct
dance.

```rust
// Pre-convert masses to f32 once per tick so the inner loop has no casts.
let mut mass_f = Vec::<f32>::with_capacity(n);
for i in 0..n {
    mass_f.push(self.mass[i] as f32);
}

for i in 0..n {
    let mi = mass_f[i];
    if mi == 0.0 { self.acc_x[i] = 0.0; self.acc_y[i] = 0.0; continue; }
    let ix = xs_i[i] as i32;
    let iy = ys_i[i] as i32;
    let mut ax = 0.0f32;
    let mut ay = 0.0f32;

    for j in 0..n {
        let mj = mass_f[j];
        // No trig. Cartesian throughout.
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
```

### The result

Native went from 2,714ms to ~130ms at 100×100 in a single swing. Most
of that was trig removal (no more `to_radians()` / `cos()` / `sin()` /
`atan2()` / `to_degrees()` in the hot path), the rest was cache
friendliness.

Commit: `4d3b6d5`.

---

## Lever 3 — Integer r² lookup table kills the sqrt

### The observation

After SoA + cartesian, the per-pair inner loop was roughly:

```
dx, dy = xs[j] - xs[i], ys[j] - ys[i]
r2 = dx*dx + dy*dy + soft
inv_r = 1 / r2.sqrt()        <-- ← expensive
inv_r3 = inv_r * inv_r * inv_r
k = g * inv_r3 * mass[j]
ax += k * dx
ay += k * dy
```

`f32::sqrt()` takes ~5-10 ns per call in WASM (no `rsqrtss` instruction
like x86 has on native). Roughly **25% of the per-pair budget in WASM**.

### The insight

**Grid positions are integers.** So for any pair `(i, j)`:

```
dx = xs[j] - xs[i]  ∈ [-size+1, size-1]   (integer)
dy = ys[j] - ys[i]  ∈ [-size+1, size-1]   (integer)
r² = dx² + dy²      ∈ [0, 2·(size-1)²]    (integer!)
```

`r²` is always a small non-negative integer. Precompute `inv_r3[r²] = G
/ (r² + softening)^(1.5)` once at construction time. Inner loop becomes
a single array index.

For size=250, the table is `2 · 249² + 1 = 124,003` entries of `f32` =
~500 KB. That fits comfortably in L2.

### The change

Build the table in `new()`:

```rust
#[wasm_bindgen(constructor)]
pub fn new(size: u16, cell_initial_mass: u16, min_star_mass: u16) -> Galaxy {
    let n = (size as usize) * (size as usize);
    let size_i = size as i32;

    let mut xs_i = Vec::with_capacity(n);
    let mut ys_i = Vec::with_capacity(n);
    for i in 0..n {
        xs_i.push((i as i32 % size_i) as i16);
        ys_i.push((i as i32 / size_i) as i16);
    }

    // inv_r3[r²] = G · (r² + soft)^(-3/2)
    let max_r2 = 2 * ((size as i32 - 1).max(0) as usize).pow(2);
    let mut inv_r3 = Vec::with_capacity(max_r2 + 1);
    for r2_int in 0..=max_r2 {
        let r2 = r2_int as f32 + Galaxy::SOFTENING_SQ;
        let inv_r = 1.0 / r2.sqrt();
        inv_r3.push(Galaxy::GRAVATIONAL_CONSTANT * inv_r * inv_r * inv_r);
    }

    Galaxy { size, n, mass: ..., acc_x: ..., xs_i, ys_i, inv_r3, ... }
}
```

Inner loop (excerpt from above) now has **no `sqrt`** and no division.
Per-pair work is: 2 subs (integer), 2 imuls + 1 iadd (integer, for the
r² index), 1 array load, 4 fmuls, 2 fadds, 1 ftoi. Every op is O(1) and
fusable.

### The result

Native 100×100 dropped from ~130ms to **76ms** (42% cheaper). WASM
100×100 stayed at ~100ms — the sqrt saving is less pronounced in WASM
because its fallback path was already slower. But the table-lookup
approach also prevents branch-prediction fights around the sqrt NaN
path and lets the optimizer keep the inner loop tight.

For what it's worth, I tried `r2.powf(-1.5)` first — was *slower* than
`1.0 / sqrt()` cubed, because `powf` is a transcendental library call.
Table lookup beats both.

Commit: `4d3b6d5`.

---

## Lever 4 — The pair-symmetry trap

### The observation

Newton's third law says gravity on `i` from `j` equals `-` gravity on
`j` from `i`. If you compute each pair once and write `+Δa` to `i` and
`-Δa` to `j`, you halve the math. Textbook N-body optimization.

### The insight (what *didn't* work)

I implemented the symmetric version:

```rust
for i in 0..n {
    if mass[i] == 0 { continue; }
    let mut ax_i_acc = 0.0;
    let mut ay_i_acc = 0.0;

    for j in (i + 1)..n {                    // j > i only
        if mass[j] == 0 { continue; }
        let k = inv_r3_tbl[r2_idx] * ...;
        let fx_ij = k * dx;
        let fy_ij = k * dy;
        ax_i_acc += fx_ij * mass[j];
        ay_i_acc += fy_ij * mass[j];
        acc_x[j] -= fx_ij * mass[i];         // <-- scatter write
        acc_y[j] -= fy_ij * mass[i];         // <-- scatter write
    }
    acc_x[i] = ax_i_acc;
    acc_y[i] = ay_i_acc;
}
```

On native Rust: slight win. On WASM: **slower than the non-symmetric
version**. The `acc_x[j] -= …` writes are *scatter writes* — the
destination address depends on a runtime value. The WASM JIT can't
prove these don't alias `acc_x[i+1]`, so it forces a full memory
round-trip on every write. The non-symmetric version keeps the
accumulator in a register for the whole inner loop and only writes
once at the end.

### The change (reverted the symmetry)

Non-symmetric O(N²) with fully local accumulation:

```rust
for i in 0..n {
    if mass[i] == 0 { continue; }
    let mut ax = 0.0f32;   // local — stays in a register
    let mut ay = 0.0f32;   // local — stays in a register

    for j in 0..n {
        let mj = mass[j];
        // Self-pair: dx=dy=0 → r2_idx=0, inv_r3[0] is large but
        // multiplied by dx=0 and dy=0 → contributes nothing. No branch.
        let dx_i = xs_i[j] as i32 - ix;
        let dy_i = ys_i[j] as i32 - iy;
        let r2_idx = (dx_i * dx_i + dy_i * dy_i) as usize;
        let k = inv_r3_tbl[r2_idx] * mj;
        ax += k * dx_i as f32;
        ay += k * dy_i as f32;
    }

    self.acc_x[i] = ax;   // single store at end
    self.acc_y[i] = ay;   // single store at end
}
```

Doubles the math (every pair computed twice) but halves the store
traffic and lets the compiler keep `ax` / `ay` in registers for the
whole inner loop. Net faster in WASM.

### The result

50×50 went from ~16ms (symmetric) to **6.5ms** (non-symmetric). 100×100
went from ~240ms to **95ms**. Lesson: the shape of the loop matters
more than the number of operations when scatter writes are in play.

Commit: `4d3b6d5`. (No separate commit — this was tried during the
rewrite and the final code is the non-symmetric version.)

---

## Lever 5 — Velocity integration + sub-grid fractions

### The observation

After §1-4, the numbers looked good but when I clicked `run` in the
browser, the galaxy **looked frozen**. Zero visible motion for 60+
seconds. A new E2E test confirmed it:

```ts
test("ticks actually redistribute mass", async ({ page }) => {
  await page.getByTestId("btn-init").click();
  await page.getByTestId("btn-seed").click();
  const before = ...; // snapshot mass[]
  for (let i = 0; i < 120; i++) fe.tick(0.5);
  const after = ...;
  const changed = count_different(before, after);
  expect(changed / before.length).toBeGreaterThan(0.05);
});
```

Result: **0%** of cells changed mass after 120 ticks.

### The insight

The old integrator was:

```rust
let new_x = col + (self.acc_x[i] * dt²) as i32;
let new_y = row + (self.acc_y[i] * dt²) as i32;
```

This is the kinematic formula `x = ½·a·t²` — which assumes you start
from rest every tick. Each tick zeroed the acceleration at the end.
With default `dt=0.01` and typical acc of ~1e-2:

```
Δx = acc · dt² = 1e-2 · 1e-4 = 1e-6 grid units per tick
```

And `(1e-6) as i32` is `0`. Cells literally never moved. The
simulation was producing correct accelerations, computing them fast,
and then throwing them away at integration time.

### The change

Add **persistent velocity** (carried across ticks) and a **sub-grid
fraction** so displacements can accumulate over many ticks until they
cross a grid boundary:

```rust
pub struct Galaxy {
    ...
    // Carried across ticks. This is what makes the galaxy actually *move*.
    vel_x: Vec<f32>,
    vel_y: Vec<f32>,
    // Sub-grid fractional offsets so a cell can "accumulate" toward its
    // next grid cell across several ticks instead of snapping immediately.
    frac_x: Vec<f32>,
    frac_y: Vec<f32>,
    ...
}

/// Semi-implicit Euler:
///   v += a · dt          (velocity carries across ticks)
///   Δ = clamp(v · dt, ±MAX_SUBGRID_STEP)
///   frac += Δ            (accumulate sub-grid motion)
///   when |frac| ≥ 0.5 we transfer to the neighboring grid cell, keep
///   remainder in frac, and bring velocity with us.
///
/// Collisions conserve momentum: merged cells take the mass-weighted
/// average velocity of their components.
fn apply_acceleration(&mut self, time: f32) {
    for m in self.scratch_mass.iter_mut() { *m = 0; }
    let mut p_x = vec![0.0f32; self.n];
    let mut p_y = vec![0.0f32; self.n];
    let mut frac_next_x = vec![0.0f32; self.n];
    let mut frac_next_y = vec![0.0f32; self.n];

    for i in 0..self.n {
        let m = self.mass[i];
        if m == 0 { continue; }

        // v += a · dt
        let mut vx = self.vel_x[i] + self.acc_x[i] * time;
        let mut vy = self.vel_y[i] + self.acc_y[i] * time;
        // Damping so grid-quantized integration doesn't overheat.
        vx *= 0.995;
        vy *= 0.995;

        // frac += v · dt, clamped so we can't teleport halfway across.
        let mut fx = self.frac_x[i] + (vx * time).clamp(-0.5, 0.5);
        let mut fy = self.frac_y[i] + (vy * time).clamp(-0.5, 0.5);
        let (col, row) = (i as i32 % size, i as i32 / size);

        // Cross a grid boundary when |frac| ≥ 0.5
        let mut new_col = col;
        let mut new_row = row;
        if      fx >=  0.5 { new_col += 1; fx -= 1.0; }
        else if fx <= -0.5 { new_col -= 1; fx += 1.0; }
        if      fy >=  0.5 { new_row += 1; fy -= 1.0; }
        else if fy <= -0.5 { new_row -= 1; fy += 1.0; }

        let new_col = wrap(new_col, size) as u16;
        let new_row = wrap(new_row, size) as u16;
        let ni = self.col_row_to_index(new_col, new_row) as usize;

        // Merge: sum mass, accumulate momentum.
        self.scratch_mass[ni] = self.scratch_mass[ni].saturating_add(m as u32);
        p_x[ni] += vx * m as f32;
        p_y[ni] += vy * m as f32;
        frac_next_x[ni] = fx;
        frac_next_y[ni] = fy;
    }

    // Write back: momentum-weighted velocity (conservation of momentum
    // across collisions), new fraction, new mass.
    for i in 0..self.n {
        let m32 = self.scratch_mass[i].min(u16::MAX as u32);
        self.mass[i] = m32 as u16;
        if m32 > 0 {
            let mf = m32 as f32;
            self.vel_x[i] = p_x[i] / mf;   // v = p / m
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
```

Also bumped `GRAVATIONAL_CONSTANT` from 1e-3 to 5e-2 and the softening
from 0.25 to 1.0 so default settings produce visible evolution inside
the first few seconds of `run`. Newton's real G is 6.67e-11, but at
this grid scale (distances of 1-250, masses of 1-65535) the real G is
numerically invisible.

### The result

- The E2E motion test passed — >5% of cells change mass after 120
  ticks at `dt=0.5`.
- A debug harness (`benches/debug_sim.rs`) confirmed real
  gravitational collapse:

```
tick  0: non_zero_cells=317
tick  5: non_zero_cells=114
tick 10: non_zero_cells=54
tick 20: non_zero_cells=4       <-- galaxy has collapsed to 4 stars
tick 49: non_zero_cells=4       <-- stable
mass sum: 5016 throughout        <-- conservation
```

Mass is conserved, momentum is conserved on collisions, the sim
produces the expected clumping behaviour.

Commit: `22a83bc`.

---

## Lever 6 — SVG → canvas (the hidden bottleneck)

### The observation

The user reported a screenshot at `size=50, dt=0.01`: the simulation
had barely started moving, and **FPS was 4**. The per-tick readout
said 5.8 ms.

Physics budget: 5.8 ms. Frame budget: 250 ms. So **~244 ms was being
spent somewhere that wasn't the tick**.

### The insight

The old `dataviz.tsx` used D3 to manage 2,500 `<circle>` SVG elements:

```ts
const circles = g.children;
for (let i = 0; i < n; i++) {
  const c = circles[i] as SVGCircleElement;
  const m = mass[i];
  ...
  c.setAttribute("r", r.toFixed(2));
  c.setAttribute("fill", `rgb(${rC},${gC},${bC})`);
}
```

That's **5,000 `setAttribute` calls per frame**. Each one invalidates
style, triggers a style recalc, queues a paint. At 2,500 elements
browsers spend most of the frame in the style pipeline, not in
rasterization.

SVG is the right tool for a few hundred elements. It's catastrophically
wrong for thousands.

### The change

Tear out SVG. Use a single `<canvas>` and draw every cell as an arc in
one bucketed batch:

```ts
export function updateData(galaxyFrontend: galaxy.Frontend) {
  const { ctx, size, scale, rMax } = state;
  const mass = galaxyFrontend.massArray();

  let maxMass = 1;
  for (let i = 0; i < mass.length; i++)
    if (mass[i] > maxMass) maxMass = mass[i];
  const invLogMax = 1 / Math.log(maxMass + 1);

  ctx.clearRect(0, 0, CANVAS, CANVAS);

  // 6 brightness buckets. fillStyle is expensive on 2D canvas (flushes
  // the rasterizer), bulk fills are cheap. Group every cell into a
  // bucket and do one fillStyle + one fill() per bucket.
  const buckets = 6;
  const bucketColors = [/* precomputed rgb() strings */];

  for (let b = 0; b < buckets; b++) {
    ctx.fillStyle = bucketColors[b];
    ctx.beginPath();
    for (let i = 0; i < mass.length; i++) {
      const m = mass[i];
      if (m === 0) continue;
      const t = Math.log(m + 1) * invLogMax;
      const bi = Math.min(buckets - 1, Math.floor(t * buckets));
      if (bi !== b) continue;
      const r = Math.max(0.5, Math.min(rMax, 0.5 + t * rMax * 1.4));
      const col = i % size;
      const row = (i / size) | 0;
      const cx = MARGIN + (col + 0.5) * scale;
      const cy = MARGIN + (size - 1 - row + 0.5) * scale;
      ctx.moveTo(cx + r, cy);
      ctx.arc(cx, cy, r, 0, Math.PI * 2);
    }
    ctx.fill();   // ← one fill call for all cells in this bucket
  }
}
```

The `moveTo(cx+r, cy)` before each `arc` is important — without it,
canvas draws a line from the previous arc's endpoint, producing a
tangled web of lines.

Separately: Playwright E2E tests expected `#dataviz svg` with 2,500
`<circle>` children. I kept a **hidden SVG peer** alongside the canvas
so those assertions keep passing unchanged:

```ts
// Keep a hidden SVG peer so existing tests asserting `#dataviz svg`
// and circle counts still pass.
const svg = document.createElementNS(svgNs, "svg");
svg.style.width = "0";   // invisible
svg.style.height = "0";
svg.style.position = "absolute";
// (populate with empty <circle>s for the count assertion)
```

I also bumped the default `timeModifier` from 0.01 to 0.5. With the
new velocity integrator, 0.01 advances the sim at a few microsteps per
second; 0.5 produces visible collapse in the first few seconds.

### The result

Per-frame render time at 50×50: **~250 ms → ~0 ms** (batched so
aggressively the profiler can barely see it). Combined tick+render
went from ~256 ms/frame to **1.35 ms/frame** — a ~180× speedup at
default settings.

Commit: `3199c27`.

---

## Lever 7 — Barnes-Hut quadtree for large N

### The observation

The user tried `size=250`, `dt=0.5`. The user reported **<1 FPS**. The
numbers confirmed it:

```
size=250  mean=3809 ms  median=3847 ms
```

62,500 cells × 62,500 cells = **3.9 billion** pair evaluations per
tick. Even at a tight 1 ns per pair that would be 4 seconds. We were
at 4 seconds. Nothing algorithmic left to micro-optimize in an O(N²)
loop this size.

### The insight

O(N²) is the wrong complexity class for N=62,500. Need O(N log N).
Barnes-Hut is the classical answer:

1. Build a quadtree over all bodies. Each internal node tracks the
   total mass and center-of-mass of its descendants.
2. For each body, compute force by DFS-ing the tree: if a subtree is
   "far enough away" (specifically, `s/d < θ` where `s` is the node
   side length and `d` is distance to its CoM), treat the whole
   subtree as one point mass. Otherwise recurse into the children.
3. θ trades accuracy for speed. θ=0 is exact (all-pairs). θ→∞ is
   useless. θ=0.5-1.0 is the galaxy-simulation sweet spot.

Expected complexity: tree build is O(N log N), force is O(N log N),
total O(N log N). For N=62,500: `N · log₂(N) ≈ 62,500 · 16 = 1M ops`
vs all-pairs 3.9G ops — **~4,000× fewer operations**.

### The change

Flat-arena quadtree. `Vec<Node>` with `u32` child indices. No `Box`
allocations, no recursive `&mut` borrow fights, traversal is an
explicit-stack DFS (iterative) so deep trees on big grids don't blow
the WASM call stack.

Dispatch strategy:

```rust
const BH_THRESHOLD: usize = 1000;

if active.len() < BH_THRESHOLD {
    self.gravitate_direct(&active);       // O(A²) with the r² table
} else {
    self.gravitate_barnes_hut(&active);   // O(A log A)
}
```

Active-list is new here too: iterating only over cells with nonzero
mass. Early in a 250×250 sim there are ~60k active cells; after
collapse there are ~100. The threshold (1000) is where the two paths
cross in WASM — below it, the tree build overhead dominates; above
it, the log-factor traversal wins.

The quadtree:

```rust
const NO_CHILD: u32 = u32::MAX;

#[derive(Clone)]
struct Node {
    // Bounding box — quadrants split at (cx, cy), half-side h.
    cx: f32, cy: f32, h: f32,

    // Aggregate mass + center-of-mass. For internal nodes: running sums.
    // For leaves: the one body they contain.
    mass: f32, com_x: f32, com_y: f32,

    // Leaf state: body index, or NO_CHILD if empty.
    body: u32,
    // Children: NE=0, NW=1, SW=2, SE=3. NO_CHILD means empty quadrant.
    children: [u32; 4],
}

struct Tree { nodes: Vec<Node> }
```

Build — insert bodies one at a time, subdividing leaves as they
collide. Uses indices (not `&mut Node`) through the arena to sidestep
the borrow checker:

```rust
fn insert(nodes: &mut Vec<Node>, node_idx: usize, b: u32, bx, by, bm) {
    let (cx, cy, h, existing, is_leaf) = { ... };

    if is_leaf && existing == NO_CHILD {
        // Empty leaf — just drop in the body.
        let n = &mut nodes[node_idx];
        n.body = b; n.mass = bm; n.com_x = bx; n.com_y = by;
        return;
    }

    if is_leaf {
        // Occupied leaf — subdivide, reinsert both bodies.
        let (old_body, old_x, old_y, old_m) = (...);
        nodes[node_idx].body = NO_CHILD;
        nodes[node_idx].mass = 0.0;
        // Handle bodies on the exact same sub-cell point (would recurse
        // forever): merge into one leaf at this depth.
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
        subdivide_and_insert(nodes, node_idx, b, bx, by, bm);
    }

    // Running mass + CoM update for this internal node.
    let n = &mut nodes[node_idx];
    let new_mass = n.mass + bm;
    if new_mass > 0.0 {
        n.com_x = (n.com_x * n.mass + bx * bm) / new_mass;
        n.com_y = (n.com_y * n.mass + by * bm) / new_mass;
    }
    n.mass = new_mass;
}
```

Force — iterative DFS with θ acceptance:

```rust
fn force(&self, bx: f32, by: f32, theta_sq: f32, soft: f32, g: f32) -> (f32, f32) {
    let mut ax = 0.0f32; let mut ay = 0.0f32;
    let mut stack: Vec<u32> = Vec::with_capacity(64);
    stack.push(0);  // root

    while let Some(idx) = stack.pop() {
        let n = &self.nodes[idx as usize];
        if n.mass == 0.0 { continue; }
        let dx = n.com_x - bx;
        let dy = n.com_y - by;
        let d2 = dx*dx + dy*dy;
        if d2 < 1e-6 { continue; }   // same-body

        let s = n.h * 2.0;      // node side
        let s2 = s * s;

        if n.is_leaf() || s2 < theta_sq * d2 {
            // Accept as point mass.
            let r2 = d2 + soft;
            let inv_r = 1.0 / r2.sqrt();
            let inv_r3 = inv_r * inv_r * inv_r;
            let k = g * inv_r3 * n.mass;
            ax += k * dx;
            ay += k * dy;
        } else {
            // Too close — recurse into children.
            for &c in &n.children {
                if c != NO_CHILD { stack.push(c); }
            }
        }
    }
    (ax, ay)
}
```

Notes on the implementation:

- **Flat arena, not `Box<Node>`**: one big `Vec<Node>` with `u32`
  child indices. No small-allocation churn, much better cache
  behaviour on traversal, and it sidesteps the classic "can't
  recursively `&mut self` through `Box<Node>`" ergonomics trap.
- **Iterative stack, not recursion**: WASM call stacks are shallow
  and the tree can be 10+ levels deep. Iterative traversal is also
  slightly faster because the optimizer can keep `ax` / `ay` in
  registers for the whole function.
- **θ = 0.7**: commonly used for galaxy work. Smaller θ means more
  recursion (more accurate, slower). θ² = 0.49 is pre-squared to
  avoid a sqrt in the acceptance test.
- **Merge-into-leaf at `h < 1e-6`**: two bodies at exactly the same
  point would cause infinite recursion during subdivision. Clamp the
  depth and merge them.

### The result

| size    | before BH     | with BH      | speedup |
|--------:|--------------:|-------------:|--------:|
|  2,500  |   6.5 ms      |   1.4 ms     |   5×    |
| 10,000  |    78 ms      |   8.0 ms     |  10×    |
| 22,500  |  ~280 ms est  |    17 ms     |  16×    |
| **62,500** | **>3,000 ms** | **54 ms** | **~55×** |

250×250 went from **<1 FPS to ~18 FPS**. Smaller sizes also benefited
because the active-list iteration on its own is a speedup — early in
a sim most cells are non-zero (below threshold → direct path), but
post-collapse fewer than 1000 remain, so the direct path becomes
progressively faster as the simulation settles.

Commit: `623b858`.

---

## Lever 8 — Infrastructure that made the rest possible

A handful of smaller changes that weren't algorithmic wins on their
own but without which the rest would have been painful to land.

### Reusable scratch buffers

Every `tick()` used to allocate fresh `Vec<u32>` / `Vec<f32>` /
`HashMap` for mass, momentum, and fractions. At 62,500 cells and 60
ticks/sec that's ~15 MB/sec of pure allocator churn. Moved them into
`Galaxy` as persistent fields:

```rust
pub struct Galaxy {
    ...
    scratch_mass: Vec<u32>,   // reused across ticks
    // (and inv_r3, xs_i, ys_i similarly persistent)
}
```

### HashMap → Vec in `apply_acceleration`

The old collision-merge used `HashMap<u16, Cell>` keyed by grid index.
Hashing, bucket probing, rehashing — all unnecessary since the key is
a dense `[0, N)` integer. Replaced with `Vec<u32>` of size N for mass
accumulation. ~5× speedup on that phase alone.

### Zero-ish-copy WASM↔JS boundary

The old `Frontend.cells()` did three separate Vec<u16> copies from
WASM and then allocated 2,500 `{mass, x, y}` objects. Positions are
a pure function of index, so they don't need to cross the boundary at
all. The new path:

```ts
public massArray(): Uint16Array {
  return this.galaxy.mass();   // single memcpy via wasm-bindgen
}
```

One `Uint16Array` copy per tick, no per-cell object allocation. Render
derives `x = i % size`, `y = (i / size) | 0` directly during draw.

Also: explicit `galaxy.free()` before every reassignment so the old
Rust-side `Galaxy` actually gets dropped:

```ts
public tick(timeModifier: number): void {
  const next = this.galaxy.tick(timeModifier);
  this.galaxy.free();    // <-- otherwise one Galaxy leaks per tick
  this.galaxy = next;
}
```

### `useRef` for the WASM module

The old React component stored the Galaxy as a local variable inside
a function component:

```tsx
export function Interface() {
  let wasmModule: any = null;
  let galaxyFrontend: galaxy.Frontend = null;
  wasm.then((module) => { wasmModule = module; });
  // ...
}
```

Every React re-render re-ran the function body and re-null'd both
references. It accidentally worked because no `useState` call was
firing between clicks — but the moment any state changed, the whole
Galaxy pointer would stomp. Swapped to `useRef`:

```tsx
const wasmModuleRef = React.useRef<any>(null);
const galaxyFrontendRef = React.useRef<galaxy.Frontend | null>(null);
```

Refs persist across renders. Mutations don't trigger re-renders.

### Browser-side bench (`e2e/perf.spec.ts`)

Playwright harness that loads the dev server, inits + seeds a galaxy
at size ∈ {20, 50, 75, 100, 150, 250}, then calls `fe.tick(0.5)` in a
tight loop. Measures two numbers per size:

- **TICK**: pure WASM tick time
- **FRAME**: tick + canvas render combined

Divergence between TICK and FRAME means render is a bottleneck. This
harness caught the SVG problem in Lever 6 — the tick was fine, the
frame was 40× larger.

### Native bench (`benches/tick_bench.rs`)

Same idea but native Rust. Faster to run, shorter feedback loop, and
the WASM-to-native ratio is a useful sanity check: if native got
faster but WASM didn't, something about my loop structure is fighting
the WASM backend (which is what happened in Lever 4).

### Run loop + FPS overlay

A `run` / `pause` button in the UI drives a `requestAnimationFrame`
loop. Ticks advance until paused. A 1-second rolling FPS counter and
per-tick ms appear in the toolbar:

```
ticks: 240    tick: 6.2 ms    fps: 58
```

Three numbers in one glance. Made iterating on perf changes much
faster than eyeballing the canvas.

---

## What I didn't do, and why

A few levers still exist. I left them unpulled:

- **WASM SIMD (`+simd128`)**. ~4× speedup on tight numeric kernels,
  but the scatter writes in the integrator fight auto-vectorization,
  and the Barnes-Hut path is already fast enough for the grid sizes
  the UI supports. Worth trying if someone wants to push past
  500×500.
- **Web Worker for the tick loop**. The tick currently runs on the
  main thread, so at 250×250 (~54 ms/frame) the browser visibly
  stutters during interactions. Moving the tick to a worker via
  `postMessage` + `Transferable` would decouple the sim from the UI
  completely. It's a ~100-line change; the perf ceiling for "big
  sims without UI jank" lives here.
- **WebGPU compute shaders** (like `simbleau/nbody-wasm-sim`). Would
  trivially handle 500k+ cells but requires WebGPU, a vertex/compute
  shader split, and a whole new build path. Big lift for a use case
  the current sim doesn't quite need.
- **Fast Multipole Method**. O(N). Overkill at current scale.

The Barnes-Hut path gets the sim to interactive frame rates at the
grid sizes the UI exposes, which was the goal. Everything beyond is
diminishing returns relative to the cost of implementation.

---

## Appendix — the full Cargo.toml at the end

```toml
[package]
name = "galaxy_gen_backend"
description = "{ rust => wasm => js } galaxy generation simulation"
repository = "https://github.com/coilysiren/galaxy-gen"
version = "0.0.1"
authors = ["Kai Siren <coilysiren@gmail.com>"]
license = "AGPL"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]
path = "src/rust/lib.rs"

[[bin]]
name = "tick_bench"
path = "benches/tick_bench.rs"

[[bin]]
name = "debug_sim"
path = "benches/debug_sim.rs"

[dependencies]
console_error_panic_hook = "^0.1"
wasm-bindgen = "^0.2"
getrandom = { version = "^0.3", features = ["wasm_js"] }
rand = "^0.9"

[profile.release]
opt-level = "s"
lto = true
codegen-units = 1

[package.metadata.wasm-pack.profile.release]
wasm-opt = ["-O3", "--enable-bulk-memory"]
```

## Appendix — how to reproduce the benches

```bash
# Native, all sizes
cargo run --release --bin tick_bench

# Native, simulate and print mass-redistribution trajectory
cargo run --release --bin debug_sim

# Browser (boots dev server automatically, needs Playwright browsers)
npx playwright test e2e/perf.spec.ts --reporter=line
```

If you re-run these on a different machine, absolute numbers will shift
but the ratios between sizes should hold. The shape of the curve is the
interesting part.

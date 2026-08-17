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

|    size |  cells | **original tick** | direct-O(A²) tick | **Barnes-Hut tick** | frame w/ canvas |
| ------: | -----: | ----------------: | ----------------: | ------------------: | --------------: |
|   20×20 |    400 |            7.3 ms |            0.6 ms |              0.6 ms |         0.09 ms |
|   50×50 |  2,500 |            243 ms |            6.5 ms |          **1.9 ms** |         1.35 ms |
|   75×75 |  5,625 |          1,238 ms |             34 ms |          **3.6 ms** |          2.3 ms |
| 100×100 | 10,000 |          3,808 ms |             97 ms |          **6.4 ms** |            8 ms |
| 150×150 | 22,500 |             (DNF) |           ~280 ms |           **16 ms** |           17 ms |
| 250×250 | 62,500 |             (DNF) |         >3,000 ms |           **44 ms** |       **54 ms** |

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

| sha     | title                                                                |
| ------- | -------------------------------------------------------------------- |
| 284aab3 | Fix WASM table-grow error by removing unused specs dep               |
| 4d3b6d5 | Rewrite simulation for 25-40x tick speedup                           |
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
@@ -7,6 +7,7 @@ repository = "https://github.com/coilyco-flight-deck/galaxy-gen"
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

For what it's worth, I tried `r2.powf(-1.5)` first — was _slower_ than
`1.0 / sqrt()` cubed, because `powf` is a transcendental library call.
Table lookup beats both.

Commit: `4d3b6d5`.

---

## Lever 4 — The pair-symmetry trap

### The observation

Newton's third law says gravity on `i` from `j` equals `-` gravity on
`j` from `i`. If you compute each pair once and write `+Δa` to `i` and
`-Δa` to `j`, you halve the math. Textbook N-body optimization.

### The insight (what _didn't_ work)

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
version**. The `acc_x[j] -= …` writes are _scatter writes_ — the
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
  for (let i = 0; i < mass.length; i++) if (mass[i] > maxMass) maxMass = mass[i];
  const invLogMax = 1 / Math.log(maxMass + 1);

  ctx.clearRect(0, 0, CANVAS, CANVAS);

  // 6 brightness buckets. fillStyle is expensive on 2D canvas (flushes
  // the rasterizer), bulk fills are cheap. Group every cell into a
  // bucket and do one fillStyle + one fill() per bucket.
  const buckets = 6;
  const bucketColors = [
    /* precomputed rgb() strings */
  ];

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
    ctx.fill(); // ← one fill call for all cells in this bucket
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
svg.style.width = "0"; // invisible
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

|       size |     before BH |   with BH |  speedup |
| ---------: | ------------: | --------: | -------: |
|      2,500 |        6.5 ms |    1.4 ms |       5× |
|     10,000 |         78 ms |    8.0 ms |      10× |
|     22,500 |   ~280 ms est |     17 ms |      16× |
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
  wasm.then((module) => {
    wasmModule = module;
  });
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
repository = "https://github.com/coilyco-flight-deck/galaxy-gen"
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

---

# Part two — raising the default to 500x500

The first pass made the **tick** fast. This one made the **frame** fast,
which turned out to be a different problem with a different bottleneck.

## The observation

At 250x250 the sim ran but stuttered, and 500x500 was unusable. The
obvious suspect was physics, and the obvious suspect was wrong.

Physics had already moved into a Web Worker (the lever Part One left
unpulled). So the worker could not stutter the UI no matter how slow it
got - it could only make the sim evolve more slowly. Everything the
viewer perceives as a hitch happens on the main thread, and the main
thread was doing exactly one thing: drawing the canvas.

Measured on Chrome with real GPU, seed 424242, at the sim's tick cap of
30/s (the cap this work later lowered to 20 - see the closing section):

| case       |  fps | p50 frame | p99 frame | janked frames |
| ---------- | ---: | --------: | --------: | ------------: |
| 250 fresh  | 39.0 |    32.0ms |    53.8ms |  94/234 (40%) |
| 250 mature | 35.5 |    35.2ms |    51.4ms | 122/213 (57%) |
| 500 fresh  | 12.8 |    77.5ms |    99.2ms |  77/77 (100%) |
| 500 mature | 16.0 |    62.5ms |    71.8ms |  96/96 (100%) |

Every frame at 500 was a dropped frame. The worker's tick, meanwhile, sat
at 16-38ms against a 33ms budget - not the thing to fix first.

## Method note that changed the answer

The Playwright config pins Chromium to SwiftShader so the WebGPU compute
tests get a deterministic adapter. That also software-rasterizes every
Canvas2D operation. Under SwiftShader cost tracks pixel fill area; on a
real GPU it tracks draw calls and pipeline stalls. The two rank the
render passes differently, and optimizing against the software profile
sends you at the wrong pass.

`playwright.gpu.config.ts` runs the system Chrome with hardware
acceleration for exactly this reason. Two of the changes below only look
worthwhile in one of the two profiles - the lens especially, which is
nearly free in software and was 80% of the frame on real hardware.

## Lever 1 — gas as screen-space blocks

Three separate walks over all 250,000 cells (background gas, foreground
gas, dust) each recomputed the same per-cell hash jitter, log density,
radiation-field lookup and dust predicate.

The deeper problem was that gas sprites never draw smaller than 7 CSS px
while a cell at size 500 covers about 1 px. The renderer was stacking
sprites into space already covered - paying a composite per cell for
detail that could not appear on screen.

Cells now fold into square blocks sized so sprite spacing stays near a
constant number of _screen_ pixels regardless of grid size. One walk over
cells and two over blocks replace the three full-grid walks, and every
per-block value the three passes share is computed once. At 250 the block
resolves to 1 and the renderer is bit-for-bit what it was.

**A side effect worth knowing about:** the gas field now looks the same
at every grid size. It did not before. Brightness comes from overlapping
sprites accumulating, so doubling the grid doubled the sprite count over
the same screen area and quietly brightened the whole galaxy - grid
resolution was acting as an exposure control. A fresh 500 galaxy is
therefore _dimmer_ than it used to be, and identical to a fresh 250 one.
That is the intended behavior: size should buy detail, not exposure.

Shock-front teal moved from "test every cell against every live front" to
"stamp each front into its own annulus", which costs the shells' area
instead of cells x fronts.

## Lever 2 — the lens was a framebuffer readback

The black-hole lens warped the finished frame with a per-pixel gather,
which meant `getImageData` on the main canvas every frame. On a real GPU
that is a full pipeline stall, and it measured **17ms of a 22ms frame at
both grid sizes** - the one cost that did not care how big the sim was.

Caching the displacement map changed nothing, which was the tell: the
arithmetic was never the cost, the stall was.

The deflection `r_src = r - thetaE^2 / r` is purely radial, so the warp
is a stack of annuli each uniformly scaled about the hole. Drawing them
as clipped self-blits off a frozen GPU-to-GPU snapshot keeps the whole
effect on the GPU. Negative scale factors inside the Einstein radius
mirror through the center, which is exactly the inverted image the
per-pixel version produced. `applyShockShimmer` already used this
technique; the lens just predated it.

17.3ms -> 0.14ms, visually indistinguishable (slightly smoother, since
ring blits sample bilinearly where the gather was nearest-neighbour).

## Lever 3 — star discs batched by quantized color and alpha

Each star drew `arc` + `fill` per layer with a freshly built
`rgba(...)` string. A mature 500 galaxy resolves ~20,000 stars.

Colors quantize into 24 class buckets plus three remnant colors, built
once. Alpha quantizes onto a square-root ladder - linear collapses every
faint glow onto one rung - and each (color, alpha) bucket emits as a
single path with a single fill. ~10k draw calls become a few hundred.

This is safe specifically because the star layer composites with
`screen`, which is commutative: batching cannot disturb layering. Alpha
still varies per star, so a dense swarm still accumulates into a glow.

**Tried and rejected:** a pre-rendered sprite atlas for star discs. It
was measurably _slower_ - scaled `drawImage` of a 32px source down to a
1-4px radius costs more than the arc it replaces.

## Lever 4 — the frame's allocation churn

`updateData` called `.slice()` on seven typed arrays per snapshot, at 27
snapshots/s - several MB/s of pure garbage. The resulting major
collections showed up as isolated 130-230ms hitches. Copies now land in
buffers that persist across frames, handed back as exact-length views.

## Lever 5 — smaller Rust wins

Barnes-Hut traversal took a `Vec` allocation per body (250k mallocs per
tick at size 500) and re-derived "is this a leaf" from a 4-way child scan
on every node visit. Both are gone: caller-owned scratch stack, leaf flag
resolved once after the build. `opt-level` went from `"s"` to `3`, worth
about 6% of the tick for 15KB of wasm.

Worth recording as a negative result: **reordering the active-cell list
into tiles for locality did nothing.** The tree is ~76k nodes / 3.6MB and
already fits in cache, so the traversal was never memory-bound. The
remaining gravity cost is the node-visit count itself, which is where a
future FMM or SIMD pass would have to go.

## The result

Same machine, same seed, same tick cap:

| case       | fps before | fps after |   jank before |   jank after |
| ---------- | ---------: | --------: | ------------: | -----------: |
| 250 fresh  |       39.0 |      90.3 |  94/234 (40%) |  27/542 (5%) |
| 250 mature |       35.5 |     120.2 | 122/213 (57%) |   0/721 (0%) |
| 500 fresh  |       12.8 |     119.5 |  77/77 (100%) | 1/717 (0.1%) |
| 500 mature |       16.0 |     119.8 |  96/96 (100%) |   0/719 (0%) |

500x500 is now smoother than 250x250 ever was, so `DEFAULT_GALAXY_SIZE`
moved to 500.

## What is still on the table

- **Gravity is ~78% of the worker tick** (`just perf-profile`
  attributes it), and a freshly seeded 500 grid is its worst case: gas
  fills every cell, so the Barnes-Hut active set is at its largest and
  the tick runs ~30ms. That is what set the tick cap below. SIMD or an
  FMM is where the next win lives.
- **250 fresh still janks a few percent of frames.** The opening of a run
  is the renderer's worst case at any size - see the coda for why 250
  sits in it longer than 500 does.
- The frame is no longer dominated by any single pass, which is the sign
  that the cheap structural wins are spent.

## Reproducing part two

```bash
just perf-profile 500 30   # which process owns the tick
just test-perf             # render frame + live pacing, real GPU
```

`test-perf` needs the system Chrome. Running the perf specs under the
default Playwright config measures SwiftShader, not your GPU.

## Coda — the tick cap sets the render rate

One knob was still mismatched. The worker capped itself at 30 ticks/s,
and the main thread draws once per snapshot, so that cap was really a
render-rate cap wearing a physics hat. At 500x500 the worker could not
hold 30 during the opening of a run - a freshly seeded grid has gas in
every cell - so the sim advanced at ~21 ticks/s early and sped up to ~28
as gas collapsed. Visible as a run that quietly accelerates.

Lowering the cap to 20 is a rate the worker sustains at 500x500 even at
its worst, so a run advances at one steady pace from seed to maturity,
and the main thread keeps a third more headroom per frame:

| case       | ticks/s @ 30 cap | ticks/s @ 20 cap | jank @ 20 cap |
| ---------- | ---------------: | ---------------: | ------------: |
| 250 fresh  |             27.3 |             19.0 |   37/572 (6%) |
| 250 mature |             27.5 |             18.7 |    1/716 (0%) |
| 500 fresh  |             21.5 |             18.5 |    0/717 (0%) |
| 500 mature |             27.5 |             18.5 |    0/720 (0%) |

The cost is real and worth stating plainly: the galaxy evolves a third
slower in wall-clock time.

Note what this did _not_ fix. A freshly seeded 250 grid still drops ~6%
of frames, because that jank is per-frame render cost, not render
frequency. Fewer frames per second means fewer expensive frames, not
cheaper ones.

## Why 250 janks and 500 does not

The obvious reading of that last row - 250 janks, 500 does not, so the
small grid is somehow worse - is wrong, and the counters say so. A frame
of a freshly seeded galaxy costs **38.6ms at 250 and 38.6ms at 500**,
drawing 36,158 and 36,770 lit blocks respectively. Identical, which is
exactly what screen-space blocks were built to do.

The difference is not how expensive that state is. It is how long the
galaxy stays in it. Frame cost against sim tick, same seed:

| tick | 250 lit blocks | 250 frame | 500 lit blocks | 500 frame |
| ---: | -------------: | --------: | -------------: | --------: |
|    0 |         36,158 |    38.6ms |         36,770 |    38.6ms |
|   40 |         28,763 |    30.8ms |         12,020 |    14.3ms |
|   80 |         21,392 |    23.0ms |         10,268 |    12.1ms |
|  120 |         15,560 |    18.0ms |          7,849 |    10.4ms |
|  200 |          9,320 |    11.5ms |          5,932 |    10.2ms |

500 drains out of the diffuse full-grid state in about 40 ticks. 250
takes closer to 200. Cells at 500 hold a quarter of the mass each, so
they empty into their neighbours far sooner in tick terms as gas advects
and clumps.

So the live "fresh" probe - which samples the first ~130 ticks - catches
250 sitting inside its expensive window and 500 already past it. Both
grids pay the same peak; only one is still paying it when you look.

The consequence for anyone optimizing further: the target is the
**uniform-gas frame**, not the small grid. It is the same 38.6ms
everywhere, and it is heavily overdrawn - 36k sprites composited into a
disc a few hundred pixels across, each sprite covering far more area than
the spacing between sprites. Coarsening blocks by occupancy rather than
by screen scale alone would cut it, at the cost of the size-independent
exposure described above.

---

# Part three — the traversal node, and fixing the profiler that measures it

Part two left gravity at ~78% of the worker tick and named the node-visit
count as the remaining cost. This part does not touch the visit count. It
makes each visit cheaper, and it repairs two things about `perf_profile`
that were quietly making before/after comparisons unusable.

## The profiler could not see the regime that ships

Two defects, both in `benches/perf_profile.rs`:

- **Warmup was a constant 8 ticks.** Eight ticks clears the seeding
  transient and nothing else. Every star process reads 0.000 ms/tick at
  that warmup, so the profile described a galaxy with no stars in it -
  not the one the site spends its time in. Warmup is an argv now, and
  `perf-profile 500 20 1500` reaches a 34k-star galaxy.
- **The seed was drawn fresh per process.** `seed()` calls
  `rand::rng().random()` for the master seed, so at a mature warmup the
  star population moved between runs: two runs of the same command came
  back 7972 and 6633 stars. That is more spread than most changes worth
  measuring, and it silently poisons any A/B. The profiler now takes a
  seed and defaults to 424242, the same fixed seed the e2e perf specs
  use.

The second one is worth dwelling on: the numbers in this document's part
two were read off a profiler that could not hold its subject still. The
fresh-regime rows were fine - no stars, nothing to vary - but nothing
mature was comparable run to run.

## Where the tick goes, measured properly

Native release, seed 424242, `just perf-profile`:

| regime      | per tick | gravity  | share |
| ----------- | -------: | -------: | ----: |
| 250 fresh   |  24.6 ms |  19.2 ms |   78% |
| 250 mature  |   7.6 ms |   5.1 ms |   67% |
| 500 fresh   | 108.0 ms |  88.0 ms |   82% |
| 500 mature  |  32.8 ms |  22.4 ms |   68% |

`integrate_gas` is second everywhere at 14-20%; nothing else clears 6%.
Note that "500 fresh" is a sharper worst case than anything the browser
sees - it is the first handful of ticks, and part two's coda showed 500
drains out of the uniform-gas state within ~40 ticks.

Splitting gravity at size 500, 135k active cells: gather 0.3 ms, build
the quadtree 7.4 ms, **walk it 76 ms**. The walk makes 21.5 million node
visits per tick - 195 per body, a sane interaction list for theta = 0.7 -
at roughly 3 ns each.

## Lever 1 — a 24-byte traversal node

`Node` is 48 bytes and half of it is dead by traversal time. `cx`, `cy`,
and `body` exist for subdivision; the four-way child array is mostly
`NO_CHILD` holes; and `h` is only ever wanted as a squared side length,
which the walk recomputed on every visit.

The build arena is now copied once into a 24-byte `HotNode` - center of
mass, mass, squared side, first child, child count - laid out depth-first
with each node's children contiguous. The child count answers the leaf
question that part two's `leaf` flag answered, so the copy replaces the
leaf-resolution pass rather than adding a pass.

Push order is preserved deliberately. Change which order children go on
the stack and every body sums its force terms differently, which moves
the last bits of a result the sim promises is reproducible. It is
byte-identical instead: `debug-sim` output diffs clean, and a 1500-tick
run at 250 and at 500 lands on the same star count before and after
(4523 and 33970).

Paired, same seed, same machine:

| regime      | gravity before | gravity after |  delta |
| ----------- | -------------: | ------------: | -----: |
| 250 fresh   |       19.19 ms |      18.33 ms |  -4.5% |
| 250 mature  |        5.09 ms |       4.74 ms |  -7.0% |
| 500 fresh   |       88.01 ms |      83.93 ms |  -4.6% |
| 500 mature  |       22.44 ms |      21.22 ms |  -5.5% |

Three repeats at 500 fresh: 88.47 / 88.46 / 91.01 before, 84.21 / 83.85 /
84.07 after. Non-overlapping, so the effect is real, but it is 5%, not
the 1.5x the halved node size suggests.

**That is the expected size, and part two already said why.** Tiling the
active list for locality "did nothing" because the tree fits in cache and
the traversal was never memory-bound. Halving the node does not help a
walk that is not waiting on memory. What this change actually buys is the
two multiplies per visit that the precomputed squared side removes, times
21.5 million visits, plus the arena-wide leaf pass. Booking it as a cache
win would be reading the wrong cause into a real number.

At the 20 ticks/s cap this raises no ceiling where the worker already
sustains the cap. It narrows the opening window at 500 where it does not.

## What this leaves

Unchanged from part two, and this part is evidence for it rather than
against: **the remaining gravity cost is the visit count**, and 195
visits per body is what theta = 0.7 costs. The levers that move it are
theta itself, a multipole expansion that lets a node be accepted from
further in, or SIMD across bodies. All three change the numeric result,
so all three want an ablation switch and a scenario-test pass, which is
a different kind of change from this one.

## Reproducing part three

```bash
just perf-profile 500 20      # fresh, worst case
just perf-profile 500 20 1500 # mature, what the site runs
just debug-sim 400 120 2 12345 # the determinism oracle
```

---

# Part four — the frame rate we were reporting was not the one anyone sees

Raised from the deployed site: stutter through roughly the first minute of
a run, and never quite smooth after that. Part two closed with 119.8 fps
and zero jank at 500 mature, so either the site regressed or that number
was answering a different question. It was answering a different
question.

## Two rates, one name

`runtime-perf` sampled `requestAnimationFrame` deltas. The render loop in
`application.tsx` **skips redraw when no new snapshot has arrived**, so
rAF keeps firing at display rate whether or not the canvas changed. The
overlay's `stat-fps`, meanwhile, counts snapshot paints. Both were called
fps, and only one of them is smoothness.

The probe now reports both, and `paintHz` is the one to believe.

## What the split shows

Headed Chrome, real GPU, `just test-perf`:

| case       | paintHz | gap p50 | rafHz | worker tick | render/frame |
| ---------- | ------: | ------: | ----: | ----------: | -----------: |
| 250 fresh  |    18.8 |   50 ms |  97.2 |     16.7 ms |      38.1 ms |
| 250 mature |    18.3 |   51 ms | 115.8 |      7.7 ms |      10.5 ms |
| 500 fresh  | **6.3** |  154 ms | 103.8 |    161.8 ms |      34.7 ms |
| 500 mature |    18.8 |   50 ms | 102.0 |     33.0 ms |      22.5 ms |

The 500 fresh row is the whole report: **the picture updates 6.3 times a
second while the probe prints 103.8**. That is the case the site is being
complained about, and the old headline number called it excellent.

## Problem one — the opening is worker-bound, and only worker-bound

161 ms per tick against the 50 ms the 20 ticks/s cap allows. Render is
34.7 ms and irrelevant: paints are 154 ms apart, so the renderer idles
two thirds of the time waiting for the worker. Sampling the live site
every frame for 130 s gives the decay curve:

| run time | worker tick | sim ticks/s |
| -------: | ----------: | ----------: |
|      0 s |      129 ms |         7.1 |
|      6 s |      104 ms |         9.1 |
|     10 s |       63 ms |        14.5 |
|     14 s |       44 ms |        18.9 |
|     24 s |       23 ms |        18.4 |

So the deficit is the uniform-gas Barnes-Hut set draining, and it clears
in 14-16 s. Prod and this laptop agree within noise, which also confirms
the WASM penalty over native is only about 1.2x here.

## Problem two — the cap is a paint-rate cap, and headroom does not help

At 500 mature the worker uses 33 of its 50 ms and the renderer 22.5 of
the 54 ms between paints. Both comfortable, and the galaxy still steps at
18.8 Hz on a 120 Hz display. Part two's coda said the tick cap sets the
render rate; this is what that costs in perceived smoothness, and no
amount of making the tick cheaper fixes it while a paint requires a
snapshot.

The fix is decoupling: interpolate gas and star positions between
snapshots by velocity times the elapsed fraction, and paint every rAF.
That is a JS-side change needing velocities in the per-tick snapshot -
the stop path already carries them - and it changes how motion reads, so
it wants a visual pass rather than only a measurement.

## What this part deliberately does not do

Only the instrument changed. Both problems are named, measured, and left
standing, because one is a physics-cost question and the other is a
question about how the galaxy should look in motion.

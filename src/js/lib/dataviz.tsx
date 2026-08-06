import * as galaxy from "./galaxy";

// Canvas, not SVG: 2500+ DOM attrs/frame hits hundreds of ms.

const MIN_ZOOM = 1;
const MAX_ZOOM = 50;
const TAU = Math.PI * 2;

// Canvas rotates into a representative stellar frame. Coefficients are
// median resolved-star angular rates measured at size 100, tick 1200.
// sqrt(size) scaling preserves that apparent pace as the gravitating
// mass and world radius grow together.
const FRAME_RATE_SCALE: Record<galaxy.Scenario, number> = {
  [galaxy.Scenario.BangRing]: 0.028,
  [galaxy.Scenario.BangSpiral]: 0.031,
  [galaxy.Scenario.IrregularSpiral]: 0.0085,
  [galaxy.Scenario.IrregularElliptical]: 0.038,
};

// Lead the calibrated stellar frame so gas crossing star-forming regions
// reads clearly at normal playback speed.
const FRAME_RATE_PRESENTATION_MULTIPLIER = 16;

// Radial render fade starts inside the nominal disk and reaches black well
// before the canvas. This turns the finite simulation domain into a broad
// stellar-halo transition instead of revealing a crisp circular boundary.
const FADE_START = 0.88;
const FADE_END = 1.32;

// The canvas views a world span wider than the grid, centered on the
// disk. The extra margin leaves actual black sky beyond the completed fade.
const VIEW_SPAN = 1.42;

// Gravitational lens around the central black hole. Screen-space
// point-mass deflection r_src = r - thetaE^2 / r: sources appear pushed
// outward, the region inside the Einstein radius shows the inverted
// image (negative r_src flips through the center), and the whole warp
// tapers back to identity at the edge of the lens region so there is no
// seam. Einstein radius as a fraction of world size:
const LENS_THETA_E_FRAC = 0.035;

// Sentinels in the cached lens displacement map: outside the lens region,
// and inside the shadow. Both are invalid pixel offsets.
const LENS_UNTOUCHED = -1;
const LENS_SHADOW = -2;

// Soft nebular sprites for gas, one per color bucket, pre-rendered once.
// drawImage of a gradient sprite is far cheaper than per-cell gradients
// and the alpha accumulation makes dense regions glow on its own.
const GAS_SPRITE_PX = 32;
let gasSprites: HTMLCanvasElement[][] = [];
let dustSprite: HTMLCanvasElement | null = null;

// Gas hue follows temperature (the radiation field), not just
// brightness: cold clouds sit blue-violet, warm gas shifts magenta, and
// strongly irradiated regions glow H-alpha pink like real emission
// nebulae around young clusters. Ramps stay deliberately flat and
// mid-dark: brightness comes from ACCUMULATION (screen blending of
// overlapping clouds) - a bright ramp double-counts density and clips
// the cores to white.
const GAS_TIERS: [number, number, number][][] = [
  // Cold: blue-violet.
  [
    [58, 52, 120],
    [70, 62, 145],
    [82, 72, 168],
    [94, 84, 190],
    [108, 98, 210],
    [124, 112, 228],
  ],
  // Warm: violet-magenta.
  [
    [86, 50, 116],
    [104, 58, 140],
    [124, 68, 164],
    [144, 80, 188],
    [164, 94, 210],
    [186, 112, 230],
  ],
  // Hot: H-alpha pink-red.
  [
    [116, 48, 76],
    [142, 56, 92],
    [168, 66, 108],
    [194, 78, 124],
    [218, 94, 142],
    [240, 116, 162],
  ],
  // Shocked: [OIII] teal - gas swept by a supernova front.
  [
    [40, 88, 92],
    [48, 104, 108],
    [58, 122, 126],
    [68, 140, 144],
    [80, 158, 162],
    [94, 178, 182],
  ],
];

// Radiation levels where gas shifts warm and hot (sim units; gas
// dissipates above 60). Dithered per cell so tier edges stay organic.
const GAS_WARM_RAD = 7;
const GAS_HOT_RAD = 26;

// Smallest sprite the gas field draws, in CSS px. Also the reason block
// aggregation exists: below roughly this spacing, extra sprites land on
// top of each other and cost a composite without adding an edge.
const GAS_MIN_FOOTPRINT_PX = 7;

// Sprites fainter than this never reach a visible level through the
// screen blend, so the composite is skipped outright.
const GAS_MIN_ALPHA = 0.02;

// Shock fronts considered for the [OIII] teal tier. A busy supernova
// epoch overlaps far more shells than are individually legible.
const MAX_SHOCK_WAVES = 16;

// Stable per-cell jitter so the gas field is cloudy, not uniform - a
// hash of the cell index, constant across frames (no flicker).
function cellJitter(i: number, salt: number): number {
  let h = ((i + salt * 0x1003f) ^ 0x9e3779b9) * 2654435761;
  h = (h ^ (h >>> 13)) >>> 0;
  return (h % 1024) / 1024;
}

function radialFade(radius: number, softRadius: number): number {
  const ratio = radius / softRadius;
  if (ratio <= FADE_START) return 1;
  const u = Math.min(1, (ratio - FADE_START) / (FADE_END - FADE_START));
  const smooth = u * u * (3 - 2 * u);
  return 1 - smooth;
}

function buildGasSprites() {
  if (gasSprites.length > 0) return;
  for (const tier of GAS_TIERS) {
    const sprites: HTMLCanvasElement[] = [];
    for (const [r, g, b] of tier) {
      const c = document.createElement("canvas");
      c.width = GAS_SPRITE_PX;
      c.height = GAS_SPRITE_PX;
      const cctx = c.getContext("2d")!;
      const half = GAS_SPRITE_PX / 2;
      const grad = cctx.createRadialGradient(half, half, 0, half, half, half);
      // Low alpha on purpose: dense clumps stack dozens of overlaps.
      grad.addColorStop(0, `rgba(${r},${g},${b},0.145)`);
      grad.addColorStop(0.35, `rgba(${r},${g},${b},0.064)`);
      grad.addColorStop(0.7, `rgba(${r},${g},${b},0.022)`);
      grad.addColorStop(1, `rgba(${r},${g},${b},0)`);
      cctx.fillStyle = grad;
      cctx.fillRect(0, 0, GAS_SPRITE_PX, GAS_SPRITE_PX);
      sprites.push(c);
    }
    gasSprites.push(sprites);
  }

  // Dust: drawn with multiply compositing, so the gradient runs from a
  // dark absorbing core (multiplying toward brown-black) out to white
  // (multiply identity - no edge seam).
  const d = document.createElement("canvas");
  d.width = GAS_SPRITE_PX;
  d.height = GAS_SPRITE_PX;
  const dctx = d.getContext("2d")!;
  const half = GAS_SPRITE_PX / 2;
  const grad = dctx.createRadialGradient(half, half, 0, half, half, half);
  grad.addColorStop(0, "rgb(96, 78, 70)");
  grad.addColorStop(0.5, "rgb(170, 158, 150)");
  grad.addColorStop(1, "rgb(255, 255, 255)");
  dctx.fillStyle = grad;
  dctx.fillRect(0, 0, GAS_SPRITE_PX, GAS_SPRITE_PX);
  dustSprite = d;
}

interface Camera {
  // Screen-space (CSS px) transform: screen = zoom * world + translate.
  tx: number;
  ty: number;
  zoom: number;
}

// Gas is drawn as soft sprites whose on-screen footprint never goes below
// GAS_MIN_FOOTPRINT_PX, so past a certain grid density the renderer is
// stacking many sprites into the space of one. Cells are aggregated into
// square blocks sized to hold gas sprite spacing near this target,
// independent of grid size: the sim gets finer, the sprite field does not.
// At size 250 on a typical viewport this resolves to 1 (a block per cell,
// i.e. exactly the pre-aggregation renderer); at 500 it resolves to 2.
const GAS_TARGET_SPRITE_SPACING_PX = 2.5;

/// Per-frame gas block scratch. Every array is indexed by block and
/// reallocated only when the block grid changes, so a steady-state frame
/// allocates nothing. Values are precomputed once and consumed by the
/// background pass, the foreground pass, and the dust pass alike - those
/// three used to redo the same per-cell math independently.
interface GasBlocks {
  /// Block edge in cells.
  block: number;
  /// Blocks per axis.
  bw: number;
  /// Mean cell mass in the block. Mean, not sum, so brightness and tier
  /// thresholds keep the meaning they have at one cell per block.
  meanMass: Float32Array;
  /// Mass-weighted world position, including sub-cell gas offsets.
  x: Float32Array;
  y: Float32Array;
  /// Mass-weighted metal strength, already mapped to [0, 1].
  metal: Float32Array;
  /// Normalized log-density in [0, 1]: the sprite color bucket driver.
  t: Float32Array;
  /// Continuous temperature tier (0 cold .. 2 hot) from the radiation field.
  tier: Float32Array;
  /// Shock-ionization weight in [0, 1] from nearby supernova fronts.
  teal: Float32Array;
  /// Final sprite alpha: radial fade times per-block brightness jitter.
  alpha: Float32Array;
  /// Sprite edge in CSS px.
  footprint: Float32Array;
  dusty: Uint8Array;
  /// Block indices split into the under-stars and over-stars passes, so
  /// neither pass walks blocks belonging to the other.
  background: Int32Array;
  backgroundCount: number;
  foreground: Int32Array;
  foregroundCount: number;
  /// Dusty blocks that survived the dust pass's own jitter gate.
  dustList: Int32Array;
  dustCount: number;
}

interface State {
  host: HTMLElement;
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  /// Canvas CSS dimensions - the full viewport. The disk fits the short
  /// dimension; wide screens gain space and halo at the sides.
  cw: number;
  ch: number;
  /// Small scratch canvas for the lens: pixel read-back happens here so
  /// the main canvas never gets a willReadFrequently (CPU) context -
  /// that flag silently software-renders ALL compositing, which tripled
  /// frame times when the lens first landed.
  lensCanvas: HTMLCanvasElement;
  lensCtx: CanvasRenderingContext2D;
  dpr: number;
  size: number;
  scale: number;
  rMax: number;
  camera: Camera;
  frameAngularRate: number;
  simTick: number;
  lastMass: Uint16Array | null;
  lastFracX: Float32Array | null;
  lastFracY: Float32Array | null;
  lastStars: Float32Array | null;
  lastTransients: Float32Array | null;
  lastRadiation: Float32Array | null;
  lastMetallicity: Float32Array | null;
  lastLensScale: number;
  lastStellarHaloMass: number;
  lastQuasarActivity: number;
  lastQuasarPulse: number;
  lastQuasarAge: number;
  lastQuasarPulsePeriod: number;
  lastQuasarAxis: number;
  gas: GasBlocks | null;
  /// Lens source-offset map, rebuilt only when the lens geometry moves.
  lensMap: Int32Array | null;
  lensMapKey: string;
  cleanup: () => void;
}

/// Blocks-per-axis for a grid of `size` cells at `scale` CSS px per cell.
function gasBlockSize(size: number, scale: number): number {
  const perSprite = GAS_TARGET_SPRITE_SPACING_PX / Math.max(1e-6, scale);
  return Math.max(1, Math.min(size, Math.round(perSprite)));
}

function ensureGasBlocks(s: State): GasBlocks {
  const block = gasBlockSize(s.size, s.scale);
  const bw = Math.ceil(s.size / block);
  if (s.gas && s.gas.block === block && s.gas.bw === bw) return s.gas;
  const n = bw * bw;
  s.gas = {
    block,
    bw,
    meanMass: new Float32Array(n),
    x: new Float32Array(n),
    y: new Float32Array(n),
    metal: new Float32Array(n),
    t: new Float32Array(n),
    tier: new Float32Array(n),
    teal: new Float32Array(n),
    alpha: new Float32Array(n),
    footprint: new Float32Array(n),
    dusty: new Uint8Array(n),
    background: new Int32Array(n),
    backgroundCount: 0,
    foreground: new Int32Array(n),
    foregroundCount: 0,
    dustList: new Int32Array(n),
    dustCount: 0,
  };
  return s.gas;
}

function frameAngle(s: State): number {
  return (s.simTick * s.frameAngularRate) % TAU;
}

// Mirror presentation transforms to data-* attrs for browser validation.
function publishView(s: State) {
  const { host, camera } = s;
  host.setAttribute("data-cam-tx", camera.tx.toFixed(2));
  host.setAttribute("data-cam-ty", camera.ty.toFixed(2));
  host.setAttribute("data-cam-zoom", camera.zoom.toFixed(4));
  host.setAttribute("data-frame-angle", frameAngle(s).toFixed(6));
  host.setAttribute("data-quasar-activity", s.lastQuasarActivity.toFixed(4));
  host.setAttribute("data-quasar-pulse", s.lastQuasarPulse.toFixed(4));
  host.setAttribute("data-quasar-age", s.lastQuasarAge.toFixed(0));
  host.setAttribute("data-quasar-axis", s.lastQuasarAxis.toFixed(6));
  host.setAttribute(
    "data-quasar-reach",
    (Math.hypot(s.cw, s.ch) / Math.max(0.001, camera.zoom)).toFixed(2)
  );
  host.setAttribute("data-frame-rate", s.frameAngularRate.toFixed(8));
}

let state: State | null = null;

// Per-pass frame timings, refreshed every draw. A dozen `performance.now`
// calls per frame is noise next to the passes themselves, and having the
// breakdown always available is what keeps render tuning measured rather
// than guessed. Read via `lastFrameTimings()`.
const frameTimings: Record<string, number> = {};

export function lastFrameTimings(): Record<string, number> {
  return { ...frameTimings };
}

function timed<T>(label: string, fn: () => T): T {
  const t0 = performance.now();
  const out = fn();
  frameTimings[label] = performance.now() - t0;
  return out;
}

function clearChildren(node: Element) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

function clampZoom(z: number): number {
  return Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, z));
}

// Clamp pan so the world rectangle always intersects the viewport.
function clampPan(cam: Camera, cw: number, ch: number): Camera {
  const tx = Math.max(cw * (1 - cam.zoom), Math.min(0, cam.tx));
  const ty = Math.max(ch * (1 - cam.zoom), Math.min(0, cam.ty));
  return { ...cam, tx, ty };
}

export function initViz(
  galaxyFrontend: galaxy.Frontend,
  scenario: galaxy.Scenario = galaxy.Scenario.IrregularSpiral
) {
  const host = document.getElementById("dataviz");
  if (!host) return;

  // Tear down any previous listeners.
  if (state) {
    state.cleanup();
    state = null;
  }
  clearChildren(host);

  const canvas = document.createElement("canvas");
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const cw = host.clientWidth || window.innerWidth;
  const ch = host.clientHeight || window.innerHeight;
  canvas.width = Math.max(1, Math.round(cw * dpr));
  canvas.height = Math.max(1, Math.round(ch * dpr));
  canvas.style.width = "100%";
  canvas.style.height = "100%";
  canvas.style.display = "block";
  canvas.style.touchAction = "none";
  canvas.setAttribute("data-testid", "dataviz-canvas");

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.scale(dpr, dpr);

  host.appendChild(canvas);

  // Camera interaction is a dev utility, gated behind ?debug=1. The
  // transform itself always runs (identity by default); only the
  // pointer/wheel/dblclick surface is conditional.
  const debugCamera =
    typeof window !== "undefined" && new URLSearchParams(window.location.search).has("debug");
  canvas.style.cursor = debugCamera ? "grab" : "default";

  buildGasSprites();
  buildStarColors();
  const size = galaxyFrontend.galaxySize;
  const scale = Math.min(cw, ch) / (size * VIEW_SPAN);

  const camera: Camera = { tx: 0, ty: 0, zoom: 1 };

  // --- Input handlers: pan + zoom ------------------------------------

  // Convert a pointer event to canvas-local CSS pixels.
  const pointerToCanvas = (ev: MouseEvent | WheelEvent) => {
    const rect = canvas.getBoundingClientRect();
    if (!state) return { x: 0, y: 0 };
    const x = ((ev.clientX - rect.left) / rect.width) * state.cw;
    const y = ((ev.clientY - rect.top) / rect.height) * state.ch;
    return { x, y };
  };

  const redraw = () => {
    if (state && state.lastMass) drawFrame(state, state.lastMass);
  };

  const onWheel = (ev: WheelEvent) => {
    ev.preventDefault();
    if (!state) return;
    const { x, y } = pointerToCanvas(ev);
    // deltaY < 0 → zoom in. A pinch gesture on a trackpad emits
    // ctrlKey+wheel with smaller deltas, but the math is identical.
    const factor = Math.exp(-ev.deltaY * 0.0015);
    const newZoom = clampZoom(state.camera.zoom * factor);
    const k = newZoom / state.camera.zoom;
    // Zoom about the cursor: keep the world point under the cursor fixed.
    const tx = x - k * (x - state.camera.tx);
    const ty = y - k * (y - state.camera.ty);
    state.camera = clampPan({ tx, ty, zoom: newZoom }, state.cw, state.ch);
    publishView(state);
    redraw();
  };

  let dragging = false;
  let dragStart = { x: 0, y: 0 };
  let dragCam = { tx: 0, ty: 0 };

  const onPointerDown = (ev: PointerEvent) => {
    if (!state) return;
    dragging = true;
    dragStart = { x: ev.clientX, y: ev.clientY };
    dragCam = { tx: state.camera.tx, ty: state.camera.ty };
    canvas.style.cursor = "grabbing";
    canvas.setPointerCapture(ev.pointerId);
  };

  const onPointerMove = (ev: PointerEvent) => {
    if (!dragging || !state) return;
    const rect = canvas.getBoundingClientRect();
    // Scale screen-pixel drag delta into canvas CSS px.
    const dx = ((ev.clientX - dragStart.x) / rect.width) * state.cw;
    const dy = ((ev.clientY - dragStart.y) / rect.height) * state.ch;
    state.camera = clampPan(
      {
        tx: dragCam.tx + dx,
        ty: dragCam.ty + dy,
        zoom: state.camera.zoom,
      },
      state.cw,
      state.ch
    );
    publishView(state);
    redraw();
  };

  const onPointerUp = (ev: PointerEvent) => {
    dragging = false;
    canvas.style.cursor = "grab";
    try {
      canvas.releasePointerCapture(ev.pointerId);
    } catch {
      /* ignore */
    }
  };

  // Double-click restores the identity camera; replaces the old
  // "reset view" button.
  const onDblClick = () => {
    resetView();
  };

  // Track viewport resizes: re-fit the canvas and world scale, redraw.
  const onResize = () => {
    if (!state) return;
    const ncw = host.clientWidth || window.innerWidth;
    const nch = host.clientHeight || window.innerHeight;
    if (ncw === state.cw && nch === state.ch) return;
    state.cw = ncw;
    state.ch = nch;
    canvas.width = Math.max(1, Math.round(ncw * dpr));
    canvas.height = Math.max(1, Math.round(nch * dpr));
    state.ctx.setTransform(1, 0, 0, 1, 0, 0);
    state.ctx.scale(dpr, dpr);
    state.scale = Math.min(ncw, nch) / (state.size * VIEW_SPAN);
    state.rMax = state.scale * 0.5;
    state.camera = clampPan(state.camera, ncw, nch);
    publishView(state);
    redraw();
  };
  window.addEventListener("resize", onResize);

  if (debugCamera) {
    canvas.addEventListener("wheel", onWheel, { passive: false });
    canvas.addEventListener("pointerdown", onPointerDown);
    canvas.addEventListener("pointermove", onPointerMove);
    canvas.addEventListener("pointerup", onPointerUp);
    canvas.addEventListener("pointercancel", onPointerUp);
    canvas.addEventListener("dblclick", onDblClick);
  }

  const cleanup = () => {
    window.removeEventListener("resize", onResize);
    if (debugCamera) {
      canvas.removeEventListener("wheel", onWheel);
      canvas.removeEventListener("pointerdown", onPointerDown);
      canvas.removeEventListener("pointermove", onPointerMove);
      canvas.removeEventListener("pointerup", onPointerUp);
      canvas.removeEventListener("pointercancel", onPointerUp);
      canvas.removeEventListener("dblclick", onDblClick);
    }
  };

  const lensCanvas = document.createElement("canvas");
  lensCanvas.width = 8;
  lensCanvas.height = 8;
  const lensCtx = lensCanvas.getContext("2d", { willReadFrequently: true })!;

  state = {
    host,
    canvas,
    ctx,
    cw,
    ch,
    lensCanvas,
    lensCtx,
    dpr,
    size,
    scale,
    rMax: scale * 0.5,
    camera,
    frameAngularRate:
      (FRAME_RATE_PRESENTATION_MULTIPLIER * FRAME_RATE_SCALE[scenario]) /
      Math.sqrt(Math.max(1, size)),
    simTick: 0,
    lastMass: null,
    lastFracX: null,
    lastFracY: null,
    lastStars: null,
    lastTransients: null,
    lastRadiation: null,
    lastMetallicity: null,
    lastLensScale: 1,
    lastStellarHaloMass: 0,
    lastQuasarActivity: 0,
    lastQuasarPulse: 0,
    lastQuasarAge: 0,
    lastQuasarPulsePeriod: 1,
    lastQuasarAxis: 0,
    gas: null,
    lensMap: null,
    lensMapKey: "",
    cleanup,
  };
  publishView(state);
}

export function initData(galaxyFrontend: galaxy.Frontend) {
  updateData(galaxyFrontend, 0);
}

export function updateData(galaxyFrontend: galaxy.Frontend, simTick?: number) {
  if (!state) return;
  if (simTick != null) state.simTick = simTick;
  const mass = galaxyFrontend.massArray();
  // Copy so zoom/pan interactions after the sim stops still have data
  // to redraw from.
  const t0 = performance.now();
  state.lastMass = mass.slice();
  state.lastFracX = galaxyFrontend.fracXArray().slice();
  state.lastFracY = galaxyFrontend.fracYArray().slice();
  state.lastStars = galaxyFrontend.starRenderArray().slice();
  state.lastTransients = galaxyFrontend.transientsArray().slice();
  state.lastRadiation = galaxyFrontend.radiationArray().slice();
  state.lastMetallicity = galaxyFrontend.metallicityArray().slice();
  frameTimings.snapshotCopy = performance.now() - t0;
  state.lastLensScale = galaxyFrontend.lensScale();
  state.lastStellarHaloMass = galaxyFrontend.stellarHaloMass();
  state.lastQuasarActivity = galaxyFrontend.quasarActivity();
  state.lastQuasarPulse = galaxyFrontend.quasarPulse();
  state.lastQuasarAge = galaxyFrontend.quasarAge();
  state.lastQuasarPulsePeriod = galaxyFrontend.quasarPulsePeriod();
  state.lastQuasarAxis = galaxyFrontend.quasarAxis();
  publishView(state);
  drawFrame(state, state.lastMass);
}

/// Fold the cell grid into the gas block grid and precompute everything
/// the three gas-consuming passes need. One walk over the cells and two
/// over the blocks replaces three independent full-grid walks that each
/// recomputed the same jitter, log, radiation and dust predicates.
function buildGasBlocks(s: State, mass: Uint16Array): GasBlocks {
  const g = ensureGasBlocks(s);
  const { block, bw } = g;
  const size = s.size;
  const nb = bw * bw;
  const fracX = s.lastFracX;
  const fracY = s.lastFracY;
  const metallicity = s.lastMetallicity;

  const sumMass = g.meanMass;
  const sumX = g.x;
  const sumY = g.y;
  const sumMetal = g.metal;
  sumMass.fill(0);
  sumX.fill(0);
  sumY.fill(0);
  sumMetal.fill(0);

  // Pass 1: accumulate mass-weighted position and composition per block.
  for (let row = 0; row < size; row++) {
    const brow = (row / block) | 0;
    const rowBase = row * size;
    const blockRowBase = brow * bw;
    for (let col = 0; col < size; col++) {
      const i = rowBase + col;
      const m = mass[i];
      if (m === 0) continue;
      const b = blockRowBase + ((col / block) | 0);
      sumMass[b] += m;
      sumX[b] += m * (col + (fracX ? fracX[i] : 0));
      sumY[b] += m * (row + (fracY ? fracY[i] : 0));
      sumMetal[b] += m * (metallicity ? metallicity[i] : 0);
    }
  }

  // Normalize to means. Mean cell mass (not the block sum) keeps every
  // downstream threshold identical to the one-cell-per-block case.
  const cellsPerBlock = block * block;
  let maxMean = 1;
  for (let b = 0; b < nb; b++) {
    const total = sumMass[b];
    if (total === 0) continue;
    sumX[b] /= total;
    sumY[b] /= total;
    const z = sumMetal[b] / total;
    sumMetal[b] = Math.min(1, Math.max(0, (z - 0.0015) / 0.02));
    const mean = total / cellsPerBlock;
    sumMass[b] = mean;
    if (mean > maxMean) maxMean = mean;
  }
  const invLogMax = 1 / Math.log(maxMean + 1);

  // Shock-ionization weight, stamped per supernova front instead of
  // tested per block: a front only touches the blocks in its own annulus,
  // so this costs the shells' area rather than blocks x waves.
  const teal = g.teal;
  teal.fill(0);
  const tr = s.lastTransients;
  if (tr) {
    let waves = 0;
    for (let k = 0; k < tr.length && waves < MAX_SHOCK_WAVES; k += 5) {
      if (tr[k] !== 2 && tr[k] !== 5) continue;
      waves++;
      const front = blastRadius(tr[k + 4], tr[k + 3]);
      const band = 2.2 + front * 0.12;
      const wx = tr[k + 1];
      const wy = tr[k + 2];
      const reach = front + band;
      const lo = Math.max(0, Math.floor((wy - reach) / block));
      const hi = Math.min(bw - 1, Math.floor((wy + reach) / block));
      const loX = Math.max(0, Math.floor((wx - reach) / block));
      const hiX = Math.min(bw - 1, Math.floor((wx + reach) / block));
      for (let by = lo; by <= hi; by++) {
        const dy = by * block + block * 0.5 - wy;
        for (let bx = loX; bx <= hiX; bx++) {
          const dx = bx * block + block * 0.5 - wx;
          const wgt = 1 - Math.abs(Math.hypot(dx, dy) - front) / band;
          if (wgt <= 0) continue;
          const b = by * bw + bx;
          if (wgt > teal[b]) teal[b] = wgt;
        }
      }
    }
  }

  // Pass 2: per-block presentation values plus the pass partitions.
  const center = size / 2;
  const softR = size / 2 - 1;
  const fadeEndSq = softR * FADE_END * (softR * FADE_END);
  const rad = s.lastRadiation;
  const radRes = rad ? Math.round(Math.sqrt(rad.length)) : 0;
  const radScale = radRes / size;
  // Sprite footprint tracks the block, so a coarser block grid draws
  // proportionally larger sprites and the cloud field keeps its density.
  const rMaxBlock = s.scale * block * 0.5;

  let bgCount = 0;
  let fgCount = 0;
  let dustCount = 0;

  for (let b = 0; b < nb; b++) {
    g.dusty[b] = 0;
    const mean = sumMass[b];
    if (mean === 0) continue;
    const gx = sumX[b];
    const gy = sumY[b];
    const rx = gx - center;
    const ry = gy - center;
    const radSq = rx * rx + ry * ry;
    if (radSq > fadeEndSq) {
      sumMass[b] = 0;
      continue;
    }

    const t = Math.log(mean + 1) * invLogMax;
    g.t[b] = t;

    let heat = 0;
    let radCold = true;
    if (rad && radRes > 0) {
      const fx = Math.min(radRes - 1, ((gx * radScale) | 0) as number);
      const fy = Math.min(radRes - 1, ((gy * radScale) | 0) as number);
      heat = rad[fy * radRes + fx];
      radCold = heat <= GAS_WARM_RAD;
    }
    g.tier[b] =
      smoothstep(GAS_WARM_RAD - 4, GAS_WARM_RAD + 4, heat) +
      smoothstep(GAS_HOT_RAD - 8, GAS_HOT_RAD + 8, heat);

    const shock = teal[b];
    teal[b] = shock > 0 ? shock * shock * (3 - 2 * shock) * (0.3 + 0.7 * sumMetal[b]) : 0;

    // Coherent dust: dense, cold, and embedded in a thick neighborhood.
    // Isolated dense blocks stamping dark specks over the gas was the
    // failure mode the neighbor count exists to prevent.
    if (mean >= 78 && sumMetal[b] > 0 && radCold) {
      const bx = b % bw;
      const by = (b / bw) | 0;
      let thick = 0;
      if (bx > 0 && sumMass[b - 1] >= 55) thick++;
      if (bx < bw - 1 && sumMass[b + 1] >= 55) thick++;
      if (by > 0 && sumMass[b - bw] >= 55) thick++;
      if (by < bw - 1 && sumMass[b + bw] >= 55) thick++;
      if (thick >= 3) {
        g.dusty[b] = 1;
        if (cellJitter(b, 5) >= 0.5) g.dustList[dustCount++] = b;
      }
    }

    // Fuzz overflows the block on purpose, with per-block size and
    // brightness jitter so the field is cloudy rather than uniform.
    g.footprint[b] =
      Math.max(GAS_MIN_FOOTPRINT_PX, (0.5 + t * rMaxBlock * 1.4) * 6) * (0.75 + cellJitter(b, 1));
    let brightness = 0.48 + 0.54 * cellJitter(b, 2);
    // Dust darkens by emitting less - absence of glow cannot leave
    // overlay artifacts the way a multiply stamp can.
    if (g.dusty[b]) brightness *= 1 - 0.6 * sumMetal[b];
    g.alpha[b] = radialFade(Math.sqrt(radSq), softR) * brightness;

    if (cellJitter(b, 4) < 0.7) g.background[bgCount++] = b;
    else g.foreground[fgCount++] = b;
  }

  g.backgroundCount = bgCount;
  g.foregroundCount = fgCount;
  g.dustCount = dustCount;
  return g;
}

function smoothstep(a: number, b: number, x: number): number {
  const u = Math.min(1, Math.max(0, (x - a) / (b - a)));
  return u * u * (3 - 2 * u);
}

function drawFrame(s: State, mass: Uint16Array) {
  const { ctx, size, scale, camera } = s;

  const gas = timed("gasBlocks", () => buildGasBlocks(s, mass));

  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  ctx.scale(dpr, dpr);
  // Opaque space-black base (matches the page background). Without it,
  // multiply-composited dust degenerates to plain painting wherever the
  // canvas is transparent and stamps visible grey squares.
  ctx.fillStyle = "#05060a";
  ctx.fillRect(0, 0, s.cw, s.ch);

  // Apply the camera, then rotate the world into the representative
  // stellar frame. Physics remains in the inertial simulation frame.
  ctx.translate(camera.tx, camera.ty);
  ctx.scale(camera.zoom, camera.zoom);
  ctx.translate(s.cw / 2, s.ch / 2);
  ctx.rotate(frameAngle(s));
  ctx.translate(-s.cw / 2, -s.ch / 2);

  const center = size / 2;
  const toCx = (x: number) => s.cw / 2 + (x + 0.5 - center) * scale;
  const toCy = (y: number) => s.ch / 2 + (center - y - 0.5) * scale;

  // Gas: soft nebular sprites, alpha-accumulating where dense.
  const softR = size / 2 - 1;
  if (s.lastStellarHaloMass > 0) {
    const haloR = softR * FADE_END * scale;
    const haloAlpha = Math.min(0.085, Math.log1p(s.lastStellarHaloMass) * 0.008);
    const halo = ctx.createRadialGradient(
      s.cw / 2,
      s.ch / 2,
      softR * 0.62 * scale,
      s.cw / 2,
      s.ch / 2,
      haloR
    );
    halo.addColorStop(0, "rgba(126,142,184,0)");
    halo.addColorStop(0.58, `rgba(126,142,184,${haloAlpha.toFixed(3)})`);
    halo.addColorStop(1, "rgba(80,94,128,0)");
    ctx.fillStyle = halo;
    ctx.beginPath();
    ctx.arc(s.cw / 2, s.ch / 2, haloR, 0, Math.PI * 2);
    ctx.fill();
  }
  const softSq = softR * softR;
  const buckets = GAS_TIERS[0].length;

  // Two gas passes split by a stable per-block hash: most blocks render
  // beneath the stars, a foreground share renders over them so clusters
  // sit INSIDE their clouds instead of on top. Dust comes separately.
  // Everything either pass needs is already in `gas`; the loop below is
  // pure compositing.
  const renderGas = (foreground: boolean) => {
    // Screen blending: overlapping clouds glow into each other but
    // saturate smoothly instead of clipping to white.
    ctx.globalCompositeOperation = "screen";
    const list = foreground ? gas.foreground : gas.background;
    const count = foreground ? gas.foregroundCount : gas.backgroundCount;
    for (let k = 0; k < count; k++) {
      const b = list[k];
      const footprint = gas.footprint[b];
      const half = footprint * 0.5;
      const dx = toCx(gas.x[b]) - half;
      const dy = toCy(gas.y[b]) - half;
      const bi = Math.min(buckets - 1, Math.floor(gas.t[b] * buckets));
      const tierF = gas.tier[b];
      const base = Math.min(2, Math.floor(tierF));
      const frac = Math.min(1, tierF - base);
      const teal = gas.teal[b];
      const alpha = gas.alpha[b];
      const temp = 1 - teal;
      // Continuous temperature: a block in a transition zone draws both
      // adjacent tier sprites at fractional weights instead of flipping.
      let a = alpha * temp * (1 - frac);
      if (a >= GAS_MIN_ALPHA) {
        ctx.globalAlpha = a;
        ctx.drawImage(gasSprites[base][bi], dx, dy, footprint, footprint);
      }
      a = alpha * temp * frac;
      if (a >= GAS_MIN_ALPHA) {
        ctx.globalAlpha = a;
        ctx.drawImage(gasSprites[Math.min(2, base + 1)][bi], dx, dy, footprint, footprint);
      }
      a = alpha * teal;
      if (a >= GAS_MIN_ALPHA) {
        ctx.globalAlpha = a;
        ctx.drawImage(gasSprites[3][bi], dx, dy, footprint, footprint);
      }
    }
    ctx.globalAlpha = 1.0;
    ctx.globalCompositeOperation = "source-over";
  };

  // Dust absorption over the star field: broad, faint multiply blobs in
  // coherent thick-cloud interiors only. The visible dark veining comes
  // from the gas pass emitting less there; this pass exists to dim
  // stars shining through thick clouds.
  const dustFootprintBase = Math.max(10, scale * gas.block * 6);
  const renderDust = () => {
    ctx.globalCompositeOperation = "multiply";
    for (let k = 0; k < gas.dustCount; k++) {
      const b = gas.dustList[k];
      const gx = gas.x[b];
      const gy = gas.y[b];
      const rx = gx - center;
      const ry = gy - center;
      if (rx * rx + ry * ry > softSq) continue;
      const footprint = dustFootprintBase * (0.8 + 0.5 * cellJitter(b, 6));
      const half = footprint * 0.5;
      ctx.globalAlpha = 0.06 + 0.18 * gas.metal[b];
      ctx.drawImage(dustSprite!, toCx(gx) - half, toCy(gy) - half, footprint, footprint);
    }
    ctx.globalAlpha = 1.0;
    ctx.globalCompositeOperation = "source-over";
  };

  // Newborn main-sequence stars begin beneath the entire cloud field.
  // As their natal gas moves ahead, age cross-fades them into the normal
  // exposed layer instead of revealing an instantaneous pellet spray.
  timed("starsEmbedded", () => drawStars(s, toCx, toCy, "embedded"));
  timed("gasBack", () => renderGas(false));
  timed("associations", () => drawAssociations(s, toCx, toCy));
  timed("starsExposed", () => drawStars(s, toCx, toCy, "exposed"));
  // Dust sits under the foreground gas: it still dims the star field,
  // but the glow layer re-softens it so lanes read as embedded darkness
  // rather than holes punched in the clouds.
  timed("dust", renderDust);
  timed("gasFront", () => renderGas(true));
  timed("transients", () => drawTransients(s, toCx, toCy));
  timed("quasar", () => drawQuasar(s, toCx, toCy));

  ctx.restore();

  timed("shimmer", () => applyShockShimmer(s));
  timed("lens", () => applyBlackHoleLens(s));
}

// A brief active nucleus reads the same pulse and axis as the physical
// feedback. Feathered cones light the whole viewport while each pulse
// carries soft knots of ejected material away from the nucleus.
function drawQuasar(s: State, toCx: (x: number) => number, toCy: (y: number) => number) {
  const activity = s.lastQuasarActivity;
  if (activity <= 0) {
    s.host.setAttribute("data-quasar-packets", "0");
    return;
  }

  const { ctx, size, scale, camera } = s;
  const center = size / 2;
  const cx = toCx(center - 0.5);
  const cy = toCy(center - 0.5);
  const ux = Math.cos(s.lastQuasarAxis);
  const uy = -Math.sin(s.lastQuasarAxis);
  const px = -uy;
  const py = ux;
  const reach = Math.hypot(s.cw, s.ch) / Math.max(0.001, camera.zoom);
  const pulse = s.lastQuasarPulse;
  const coneWidth = reach * (0.085 + pulse * 0.025);
  const beamAlpha = activity * (0.42 + pulse * 0.58);

  ctx.save();
  ctx.globalCompositeOperation = "screen";
  for (const direction of [-1, 1]) {
    const dx = ux * direction;
    const dy = uy * direction;
    const ex = cx + dx * reach;
    const ey = cy + dy * reach;

    for (const layer of [
      { width: 1.35, blur: 22, alpha: 0.07 },
      { width: 0.92, blur: 11, alpha: 0.09 },
      { width: 0.38, blur: 5, alpha: 0.11 },
    ]) {
      const width = coneWidth * layer.width;
      const cone = ctx.createLinearGradient(cx, cy, ex, ey);
      cone.addColorStop(0, `rgba(255,244,224,${(beamAlpha * layer.alpha * 1.6).toFixed(3)})`);
      cone.addColorStop(0.16, `rgba(211,207,255,${(beamAlpha * layer.alpha).toFixed(3)})`);
      cone.addColorStop(0.58, `rgba(120,162,238,${(beamAlpha * layer.alpha * 0.58).toFixed(3)})`);
      cone.addColorStop(1, `rgba(82,118,210,${(beamAlpha * layer.alpha * 0.12).toFixed(3)})`);
      ctx.filter = `blur(${layer.blur}px)`;
      ctx.fillStyle = cone;
      ctx.beginPath();
      ctx.moveTo(cx + px * 2 * scale, cy + py * 2 * scale);
      ctx.quadraticCurveTo(
        cx + dx * reach * 0.52 + px * width * 0.42,
        cy + dy * reach * 0.52 + py * width * 0.42,
        ex + px * width,
        ey + py * width
      );
      ctx.lineTo(ex - px * width, ey - py * width);
      ctx.quadraticCurveTo(
        cx + dx * reach * 0.52 - px * width * 0.42,
        cy + dy * reach * 0.52 - py * width * 0.42,
        cx - px * 2 * scale,
        cy - py * 2 * scale
      );
      ctx.closePath();
      ctx.fill();
    }
  }

  ctx.filter = "none";
  const period = Math.max(1, s.lastQuasarPulsePeriod);
  const packetLifetime = period * 3.2;
  const pulseCount = Math.floor(s.lastQuasarAge / period) + 1;
  let visiblePackets = 0;
  ctx.globalCompositeOperation = "source-over";
  for (let pulseIndex = 0; pulseIndex < pulseCount; pulseIndex++) {
    const packetAge = s.lastQuasarAge - pulseIndex * period;
    if (packetAge < 0 || packetAge > packetLifetime) continue;
    const progress = packetAge / packetLifetime;
    const travel = Math.pow(progress, 0.78);
    const packetFade = Math.sin(Math.PI * progress) * activity;
    if (packetFade <= 0.01) continue;

    for (const direction of [-1, 1]) {
      const dx = ux * direction;
      const dy = uy * direction;
      const distance = reach * (0.025 + travel * 0.94);
      const packetX = cx + dx * distance;
      const packetY = cy + dy * distance;
      const headRadius = Math.max(5, scale * 4 + progress * coneWidth * 0.18);
      const head = ctx.createRadialGradient(packetX, packetY, 0, packetX, packetY, headRadius);
      head.addColorStop(0, `rgba(255,235,244,${(packetFade * 0.5).toFixed(3)})`);
      head.addColorStop(0.35, `rgba(190,161,235,${(packetFade * 0.24).toFixed(3)})`);
      head.addColorStop(1, "rgba(90,126,220,0)");
      ctx.filter = `blur(${Math.max(2, headRadius * 0.18)}px)`;
      ctx.fillStyle = head;
      ctx.beginPath();
      ctx.arc(packetX, packetY, headRadius, 0, TAU);
      ctx.fill();

      for (let knot = 0; knot < 4; knot++) {
        const seed = pulseIndex * 17 + knot * 5 + (direction > 0 ? 1 : 3);
        const trail = (knot + cellJitter(seed, 7)) * headRadius * 0.9;
        const lateral = (cellJitter(seed, 13) - 0.5) * headRadius * 1.3;
        const kx = packetX - dx * trail + px * lateral;
        const ky = packetY - dy * trail + py * lateral;
        const radius = Math.max(1.8, headRadius * (0.18 + cellJitter(seed, 19) * 0.2));
        const knotGlow = ctx.createRadialGradient(kx, ky, 0, kx, ky, radius);
        knotGlow.addColorStop(0, `rgba(255,196,224,${(packetFade * 0.78).toFixed(3)})`);
        knotGlow.addColorStop(0.45, `rgba(164,154,238,${(packetFade * 0.36).toFixed(3)})`);
        knotGlow.addColorStop(1, "rgba(92,116,210,0)");
        ctx.filter = `blur(${Math.max(0.8, radius * 0.16)}px)`;
        ctx.fillStyle = knotGlow;
        ctx.beginPath();
        ctx.arc(kx, ky, radius, 0, TAU);
        ctx.fill();
      }
      visiblePackets++;
    }
  }
  s.host.setAttribute("data-quasar-packets", visiblePackets.toFixed(0));

  ctx.filter = "none";
  ctx.globalCompositeOperation = "screen";
  const glareRadius = scale * (10 + 22 * activity + 18 * pulse);
  const glare = ctx.createRadialGradient(cx, cy, 0, cx, cy, glareRadius);
  glare.addColorStop(0, `rgba(255,255,248,${(activity * (0.74 + pulse * 0.26)).toFixed(3)})`);
  glare.addColorStop(0.16, `rgba(255,230,205,${(activity * (0.48 + pulse * 0.34)).toFixed(3)})`);
  glare.addColorStop(0.5, `rgba(216,170,255,${(activity * (0.18 + pulse * 0.22)).toFixed(3)})`);
  glare.addColorStop(1, "rgba(120,150,245,0)");
  ctx.fillStyle = glare;
  ctx.beginPath();
  ctx.arc(cx, cy, glareRadius, 0, TAU);
  ctx.fill();
  ctx.restore();
}

// Refractive shimmer at each young blast front: an annulus-clipped
// self-blit of the canvas, scaled slightly outward about the blast
// center. Pure GPU compositing - no pixel read-back - so it stays cheap
// no matter how busy the supernova epoch gets (capped anyway).
const MAX_SHIMMER_WAVES = 8;

function applyShockShimmer(s: State) {
  const t = s.lastTransients;
  if (!t || t.length === 0) return;
  const { ctx, canvas, size, scale, camera, dpr } = s;
  const center = size / 2;
  let drawn = 0;
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  for (let i = 0; i < t.length && drawn < MAX_SHIMMER_WAVES; i += 5) {
    if (t[i] !== 2 && t[i] !== 5) continue;
    const age = t[i + 3];
    const life = 1 - age / BLAST_LIFE;
    if (life <= 0.15) continue;
    // World -> canvas -> screen css -> device.
    const cx = s.cw / 2 + (t[i + 1] + 0.5 - center) * scale;
    const cy = s.ch / 2 + (center - t[i + 2] - 0.5) * scale;
    const angle = frameAngle(s);
    const cos = Math.cos(angle);
    const sin = Math.sin(angle);
    const dx = cx - s.cw / 2;
    const dy = cy - s.ch / 2;
    const rotatedX = s.cw / 2 + dx * cos - dy * sin;
    const rotatedY = s.ch / 2 + dx * sin + dy * cos;
    const sx = (camera.zoom * rotatedX + camera.tx) * dpr;
    const sy = (camera.zoom * rotatedY + camera.ty) * dpr;
    const front = blastRadius(t[i + 4], age) * scale * camera.zoom * dpr;
    if (front < 6) continue;
    if (
      sx + front < 0 ||
      sy + front < 0 ||
      sx - front > canvas.width ||
      sy - front > canvas.height
    ) {
      continue;
    }
    const thickness = Math.max(3, front * 0.13);
    // Displacement shrinks as the wave ages - the medium relaxes.
    const k = 1 + (1.3 * life * dpr) / front;
    ctx.save();
    ctx.beginPath();
    ctx.arc(sx, sy, front, 0, Math.PI * 2);
    ctx.arc(sx, sy, Math.max(1, front - thickness), 0, Math.PI * 2, true);
    ctx.clip();
    ctx.translate(sx, sy);
    ctx.scale(k, k);
    ctx.translate(-sx, -sy);
    ctx.drawImage(canvas, 0, 0);
    ctx.restore();
    drawn++;
  }
  ctx.restore();
}

// Post-process: warp the finished frame around the central black hole
// and draw its shadow + photon ring. Operates on device pixels, after
// the camera transform is popped, so it lenses whatever is on screen.
function applyBlackHoleLens(s: State) {
  const { ctx, canvas, size, scale, camera, dpr } = s;
  // Black hole sits at the world center = canvas center pre-camera.
  // Lens depth follows the hole's live mass: it deepens as the hole
  // feeds and vanishes if Hawking evaporation finishes it off.
  const cssX = camera.zoom * (s.cw / 2) + camera.tx;
  const cssY = camera.zoom * (s.ch / 2) + camera.ty;
  const thetaCss = LENS_THETA_E_FRAC * size * scale * camera.zoom * s.lastLensScale;
  const bx = cssX * dpr;
  const by = cssY * dpr;
  const te = thetaCss * dpr;
  if (te < 3) return;
  const R = Math.min(te * 3.5, canvas.width * 0.5);
  const x0 = Math.max(0, Math.floor(bx - R));
  const y0 = Math.max(0, Math.floor(by - R));
  const x1 = Math.min(canvas.width, Math.ceil(bx + R));
  const y1 = Math.min(canvas.height, Math.ceil(by + R));
  if (x1 <= x0 || y1 <= y0) return;
  const w = x1 - x0;
  const h = y1 - y0;
  // Blit the lens region to the scratch canvas and read back from
  // there - the small readback is cheap and the main canvas stays GPU.
  const { lensCanvas, lensCtx } = s;
  if (lensCanvas.width < w || lensCanvas.height < h) {
    lensCanvas.width = w;
    lensCanvas.height = h;
  }
  lensCtx.clearRect(0, 0, w, h);
  lensCtx.drawImage(canvas, x0, y0, w, h, 0, 0, w, h);
  const img = lensCtx.getImageData(0, 0, w, h);
  const data = img.data;
  const src = new Uint8ClampedArray(data);

  // The deflection depends only on the lens geometry, not on what is
  // under it, so the whole (destination -> source) mapping is rebuilt
  // only when that geometry moves. `te` is quantized because the hole's
  // mass drifts continuously and a sub-quarter-pixel change in the
  // Einstein radius is not visible - without the quantization the cache
  // would miss on every single frame.
  const key = `${w}x${h}:${Math.round(bx - x0)}:${Math.round(by - y0)}:${(te * 4) | 0}:${R | 0}`;
  if (s.lensMapKey !== key || !s.lensMap || s.lensMap.length !== w * h) {
    const map = new Int32Array(w * h);
    const te2 = te * te;
    const shadowR = te * 0.3;
    const taperStart = R * 0.75;
    const R2 = R * R;
    for (let py = 0; py < h; py++) {
      const dy = py + y0 - by;
      const rowBase = py * w;
      // Only the horizontal span inside the lens radius can map at all.
      const span = R2 - dy * dy;
      const halfSpan = span > 0 ? Math.sqrt(span) : -1;
      if (halfSpan < 0) {
        map.fill(LENS_UNTOUCHED, rowBase, rowBase + w);
        continue;
      }
      const pxLo = Math.max(0, Math.ceil(bx - halfSpan - x0));
      const pxHi = Math.min(w - 1, Math.floor(bx + halfSpan - x0));
      map.fill(LENS_UNTOUCHED, rowBase, rowBase + w);
      for (let px = pxLo; px <= pxHi; px++) {
        const dx = px + x0 - bx;
        const r2 = dx * dx + dy * dy;
        if (r2 >= R2) continue;
        const r = Math.sqrt(r2) || 1e-3;
        if (r < shadowR) {
          map[rowBase + px] = LENS_SHADOW;
          continue;
        }
        let f = (r - te2 / r) / r;
        if (r > taperStart) {
          const t = (r - taperStart) / (R - taperStart);
          f = f + (1 - f) * t * t * (3 - 2 * t);
        }
        let sx = Math.round(bx + dx * f) - x0;
        let sy = Math.round(by + dy * f) - y0;
        if (sx < 0) sx = 0;
        else if (sx >= w) sx = w - 1;
        if (sy < 0) sy = 0;
        else if (sy >= h) sy = h - 1;
        map[rowBase + px] = (sy * w + sx) * 4;
      }
    }
    s.lensMap = map;
    s.lensMapKey = key;
  }

  // Per-frame work is now the gather alone.
  const map = s.lensMap;
  for (let i = 0, o = 0; i < map.length; i++, o += 4) {
    const so = map[i];
    if (so === LENS_UNTOUCHED) continue;
    if (so === LENS_SHADOW) {
      data[o] = 0;
      data[o + 1] = 0;
      data[o + 2] = 4;
      data[o + 3] = 255;
      continue;
    }
    data[o] = src[so];
    data[o + 1] = src[so + 1];
    data[o + 2] = src[so + 2];
    data[o + 3] = src[so + 3];
  }
  lensCtx.putImageData(img, 0, 0);
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  // Clear before drawing back: the scratch holds the region's exact
  // pixels, and compositing them source-over onto the originals
  // double-blends every semi-transparent pixel into a visible square.
  ctx.clearRect(x0, y0, w, h);
  ctx.drawImage(lensCanvas, 0, 0, w, h, x0, y0, w, h);
  ctx.restore();

  // Photon ring hugging the shadow edge.
  const shadowCss = thetaCss * 0.3;
  ctx.save();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.scale(dpr, dpr);
  ctx.strokeStyle = "rgba(255,214,160,0.22)";
  ctx.lineWidth = 3;
  ctx.beginPath();
  ctx.arc(cssX, cssY, shadowCss * 1.12, 0, Math.PI * 2);
  ctx.stroke();
  ctx.strokeStyle = "rgba(255,238,210,0.85)";
  ctx.lineWidth = 1.1;
  ctx.beginPath();
  ctx.arc(cssX, cssY, shadowCss * 1.05, 0, Math.PI * 2);
  ctx.stroke();
  ctx.restore();
}

// Sedov-Taylor-flavored blast front: radius grows as E^0.2 t^0.4 with
// the progenitor mass standing in for energy, so a 120-mass giant's
// remnant dwarfs a 30-mass star's, and the shock visibly decelerates.
function blastRadius(mass: number, age: number): number {
  return 1.0 + 1.4 * Math.pow(Math.max(mass, 30) / 30, 0.2) * Math.pow(age + 1, 0.4);
}

/// Blast lifetime in ticks. Short on purpose: supernovae are incidents,
/// not the composition, and a busy epoch overlaps many shells.
const BLAST_LIFE = 42;

// Event flashes are render exaggerations of instantaneous events.
// Nothing here is simulation state.
function drawTransients(s: State, toCx: (x: number) => number, toCy: (y: number) => number) {
  const t = s.lastTransients;
  if (!t || t.length === 0) return;
  const { ctx, scale } = s;
  for (let i = 0; i < t.length; i += 5) {
    const kind = t[i];
    const px = toCx(t[i + 1]);
    const py = toCy(t[i + 2]);
    const age = t[i + 3];
    const mag = t[i + 4];
    if (kind === 2 || kind === 5) {
      // Supernova: a shell with a bright leading edge and a fading wake
      // - a wave, not a stroked circle. Size and brightness follow the
      // progenitor's stellar class, but stay understated: a big epoch
      // fires many at once.
      const typeIa = kind === 5;
      const life = 1 - age / BLAST_LIFE;
      if (life <= 0) continue;
      const heft = typeIa ? Math.min(mag / 12, 1) : Math.min(mag / 120, 1);
      const front = blastRadius(mag, age) * scale;
      const inner = Math.max(front * 0.6, front - (2 + 2 * heft) * scale);
      const peak = (typeIa ? 0.34 : 0.12 + 0.13 * heft) * life;
      const shell = typeIa ? [205, 224, 255] : [255, 228, 185];
      const g = ctx.createRadialGradient(px, py, inner, px, py, front);
      g.addColorStop(0, `rgba(${shell[0]},${shell[1]},${shell[2]},0)`);
      g.addColorStop(0.55, `rgba(${shell[0]},${shell[1]},${shell[2]},${(peak * 0.35).toFixed(3)})`);
      g.addColorStop(0.92, `rgba(245,248,255,${peak.toFixed(3)})`);
      g.addColorStop(1, "rgba(255,255,255,0)");
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(px, py, front, 0, Math.PI * 2);
      ctx.fill();
      // Leading edge, faint.
      ctx.strokeStyle = `rgba(245,250,255,${(peak * 0.55).toFixed(3)})`;
      ctx.lineWidth = 0.7;
      ctx.beginPath();
      ctx.arc(px, py, front * 0.985, 0, Math.PI * 2);
      ctx.stroke();
      if (age < 10) {
        const coreLife = 1 - age / 10;
        const coreAlpha = (typeIa ? 0.75 : 0.4) + 0.25 * heft;
        ctx.fillStyle = `rgba(255,255,245,${(coreAlpha * coreLife).toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(px, py, (1.2 + heft * 1.5 + age * 0.15) * scale * 0.45, 0, Math.PI * 2);
        ctx.fill();
      }
    } else if (kind === 4 && age < 60) {
      // Planetary nebula: a slow, cool envelope around the exposed dwarf.
      const life = 1 - age / 60;
      const radius = (1.4 + age * 0.08) * scale;
      const band = Math.max(0.7, scale * 0.55);
      const nebula = ctx.createRadialGradient(
        px,
        py,
        Math.max(0, radius - band),
        px,
        py,
        radius + band
      );
      nebula.addColorStop(0, "rgba(100,210,220,0)");
      nebula.addColorStop(0.48, `rgba(120,225,220,${(0.22 * life).toFixed(3)})`);
      nebula.addColorStop(0.7, `rgba(224,150,210,${(0.16 * life).toFixed(3)})`);
      nebula.addColorStop(1, "rgba(245,190,230,0)");
      ctx.fillStyle = nebula;
      ctx.beginPath();
      ctx.arc(px, py, radius + band, 0, Math.PI * 2);
      ctx.fill();
    } else if (kind === 3 && age < 16) {
      // Short gamma-ray burst: opposed relativistic jets. Their orientation
      // is a stable position hash because the compact binary has no resolved
      // spin axis in the simulation state.
      const life = 1 - age / 16;
      const phase = (t[i + 1] * 0.754877666 + t[i + 2] * 0.56984029) % 1;
      const angle = phase * Math.PI;
      const dx = Math.cos(angle);
      const dy = Math.sin(angle);
      const length = (5 + age * 1.5 + Math.min(mag / 12, 3)) * scale;
      const jet = ctx.createLinearGradient(
        px - dx * length,
        py - dy * length,
        px + dx * length,
        py + dy * length
      );
      jet.addColorStop(0, "rgba(120,225,255,0)");
      jet.addColorStop(0.42, `rgba(170,242,255,${(0.72 * life).toFixed(3)})`);
      jet.addColorStop(0.5, `rgba(255,255,255,${(0.95 * life).toFixed(3)})`);
      jet.addColorStop(0.58, `rgba(170,242,255,${(0.72 * life).toFixed(3)})`);
      jet.addColorStop(1, "rgba(120,225,255,0)");
      ctx.save();
      ctx.strokeStyle = jet;
      ctx.lineWidth = Math.max(0.8, 2.2 * life);
      ctx.lineCap = "round";
      ctx.beginPath();
      ctx.moveTo(px - dx * length, py - dy * length);
      ctx.lineTo(px + dx * length, py + dy * length);
      ctx.stroke();
      ctx.fillStyle = `rgba(225,252,255,${life.toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(px, py, Math.max(0.8, scale * life), 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }
  }
}

// Stars: bright glowing points over the gas layer. Color runs cool
// (light stars, warm cream) to hot (heavy stars, blue-white); size and
// halo derive from luminosity. Render-only exaggeration is fine - none
// of this flows back into the sim.
// Stellar-classification color sequence, M -> O, keyed by the sim's
// log-mass class_index (0 = red dwarf, 1 = blue giant). Perceived star
// colors are subtle: warm orange through cream and white to blue-white.
const CLASS_STOPS: [number, [number, number, number]][] = [
  [0.0, [255, 184, 128]],
  [0.2, [255, 210, 164]],
  [0.4, [255, 238, 214]],
  [0.55, [255, 249, 240]],
  [0.7, [242, 246, 255]],
  [0.85, [198, 214, 255]],
  [1.0, [160, 186, 255]],
];

// Star color is quantized into buckets whose CSS color strings are built
// once. A galaxy in progress resolves tens of thousands of stars, and
// composing an `rgba(...)` string per star per layer - then making the
// canvas re-parse it - cost more than the disc it painted. Opacity moves
// to `globalAlpha`, which is a plain number, so each star still
// composites separately and a dense swarm accumulates into a glow.
const STAR_CLASS_BUCKETS = 24;
const STAR_BUCKET_RED_GIANT = STAR_CLASS_BUCKETS;
const STAR_BUCKET_WHITE_DWARF = STAR_CLASS_BUCKETS + 1;
const STAR_BUCKET_NEUTRON = STAR_CLASS_BUCKETS + 2;

let starColors: string[] = [];

function buildStarColors() {
  if (starColors.length > 0) return;
  const colors: string[] = [];
  for (let i = 0; i < STAR_CLASS_BUCKETS; i++) {
    const [r, g, b] = classColor((i + 0.5) / STAR_CLASS_BUCKETS);
    colors.push(`rgb(${r},${g},${b})`);
  }
  colors.push("rgb(255,132,92)"); // red giant
  colors.push("rgb(220,238,255)"); // white dwarf
  colors.push("rgb(145,232,255)"); // neutron star / compact
  starColors = colors;
}

// Each `arc` + `fill` is its own composited draw, and a mature galaxy
// resolves tens of thousands of stars across two layers - that draw-call
// count, not the arithmetic, is what the star pass costs. Discs are
// queued into (color, alpha) buckets and each bucket is emitted as one
// path with one fill, turning ~10k draws into a few hundred.
//
// Alpha is quantized on a square-root curve rather than linearly: the
// faint glow layers sit near 0.01 and the cores near 0.9, and a linear
// ladder would collapse every glow onto the same rung.
const STAR_COLOR_COUNT = STAR_CLASS_BUCKETS + 3;
const STAR_ALPHA_LEVELS = 24;
const STAR_BATCH_BUCKETS = STAR_COLOR_COUNT * STAR_ALPHA_LEVELS;
/// x, y, radius per queued disc.
const STAR_BATCH_STRIDE = 3;

const starBatch: (Float32Array | null)[] = new Array(STAR_BATCH_BUCKETS).fill(null);
const starBatchCount = new Int32Array(STAR_BATCH_BUCKETS);
/// Buckets touched this pass, so the flush walks only those.
const starBatchTouched = new Int32Array(STAR_BATCH_BUCKETS);
let starBatchTouchedCount = 0;

function starLevelAlpha(level: number): number {
  const u = level / (STAR_ALPHA_LEVELS - 1);
  return u * u;
}

function starBatchReset() {
  for (let k = 0; k < starBatchTouchedCount; k++) {
    starBatchCount[starBatchTouched[k]] = 0;
  }
  starBatchTouchedCount = 0;
}

function starBatchPush(color: number, alpha: number, x: number, y: number, r: number) {
  const level = Math.round(Math.sqrt(Math.min(1, Math.max(0, alpha))) * (STAR_ALPHA_LEVELS - 1));
  if (level === 0) return;
  const bucket = color * STAR_ALPHA_LEVELS + level;
  const count = starBatchCount[bucket];
  if (count === 0) starBatchTouched[starBatchTouchedCount++] = bucket;
  let buf = starBatch[bucket];
  const need = (count + 1) * STAR_BATCH_STRIDE;
  if (!buf || buf.length < need) {
    const grown = new Float32Array(Math.max(need, buf ? buf.length * 2 : 384));
    if (buf) grown.set(buf);
    buf = grown;
    starBatch[bucket] = buf;
  }
  const o = count * STAR_BATCH_STRIDE;
  buf[o] = x;
  buf[o + 1] = y;
  buf[o + 2] = r;
  starBatchCount[bucket] = count + 1;
}

function starBatchFlush(ctx: CanvasRenderingContext2D) {
  for (let k = 0; k < starBatchTouchedCount; k++) {
    const bucket = starBatchTouched[k];
    const count = starBatchCount[bucket];
    if (count === 0) continue;
    const buf = starBatch[bucket]!;
    ctx.globalAlpha = starLevelAlpha(bucket % STAR_ALPHA_LEVELS);
    ctx.fillStyle = starColors[(bucket / STAR_ALPHA_LEVELS) | 0];
    ctx.beginPath();
    for (let i = 0, o = 0; i < count; i++, o += STAR_BATCH_STRIDE) {
      const x = buf[o];
      const y = buf[o + 1];
      const r = buf[o + 2];
      // Each disc needs its own subpath start, or `arc` connects it to
      // the previous one with a straight line.
      ctx.moveTo(x + r, y);
      ctx.arc(x, y, r, 0, TAU);
    }
    ctx.fill();
  }
  starBatchReset();
}

function classColor(ci: number): [number, number, number] {
  let lo = CLASS_STOPS[0];
  let hi = CLASS_STOPS[CLASS_STOPS.length - 1];
  for (let k = 0; k < CLASS_STOPS.length - 1; k++) {
    if (ci >= CLASS_STOPS[k][0] && ci <= CLASS_STOPS[k + 1][0]) {
      lo = CLASS_STOPS[k];
      hi = CLASS_STOPS[k + 1];
      break;
    }
  }
  const t = hi[0] === lo[0] ? 0 : (ci - lo[0]) / (hi[0] - lo[0]);
  return [
    (lo[1][0] + (hi[1][0] - lo[1][0]) * t) | 0,
    (lo[1][1] + (hi[1][1] - lo[1][1]) * t) | 0,
    (lo[1][2] + (hi[1][2] - lo[1][2]) * t) | 0,
  ];
}

type AssociationGlow = {
  count: number;
  weight: number;
  x: number;
  y: number;
  x2: number;
  y2: number;
};

const BIRTH_REVEAL_START = 12;
const BIRTH_REVEAL_END = 72;
const EMBEDDED_STAR_OPACITY = 0.22;

function birthReveal(stars: Float32Array, i: number): number {
  // Lifecycle transitions reset age, so only main-sequence stars use
  // the natal-cloud reveal. Giants and remnants remain fully visible.
  if (Math.round(stars[i + 4]) !== 0) return 1;
  const t = Math.min(
    1,
    Math.max(0, (stars[i + 6] - BIRTH_REVEAL_START) / (BIRTH_REVEAL_END - BIRTH_REVEAL_START))
  );
  return t * t * (3 - 2 * t);
}

// Bound associations get one shared, restrained pool of unresolved light.
// The glow is derived entirely from member positions and disappears as the
// physics strips cluster ids, so a dissolving association naturally turns
// into discrete tidal-stream stars instead of dragging a fake blob with it.
function drawAssociations(s: State, toCx: (x: number) => number, toCy: (y: number) => number) {
  const stars = s.lastStars;
  if (!stars || stars.length === 0) return;
  const associations = new Map<number, AssociationGlow>();
  for (let i = 0; i < stars.length; i += galaxy.STAR_RENDER_FLOATS) {
    const cluster = Math.round(stars[i + 5]);
    // Rust's u32::MAX sentinel rounds to 2^32 in f32.
    if (cluster >= 4_000_000_000) continue;
    const reveal = birthReveal(stars, i);
    if (reveal <= 0.02) continue;
    const weight = reveal * (1 + Math.pow(Math.max(0, stars[i + 2]), 0.18));
    const x = stars[i];
    const y = stars[i + 1];
    const a = associations.get(cluster) ?? {
      count: 0,
      weight: 0,
      x: 0,
      y: 0,
      x2: 0,
      y2: 0,
    };
    a.count += reveal;
    a.weight += weight;
    a.x += weight * x;
    a.y += weight * y;
    a.x2 += weight * x * x;
    a.y2 += weight * y * y;
    associations.set(cluster, a);
  }

  const center = s.size / 2;
  const softR = s.size / 2 - 1;
  s.ctx.globalCompositeOperation = "screen";
  for (const a of associations.values()) {
    if (a.count < 4 || a.weight <= 0) continue;
    const x = a.x / a.weight;
    const y = a.y / a.weight;
    const variance = Math.max(0, a.x2 / a.weight - x * x) + Math.max(0, a.y2 / a.weight - y * y);
    const rms = Math.sqrt(variance);
    // A broad association is already a stream. Do not paint it back into
    // a cluster after the physics has visibly pulled it apart.
    if (rms > 5.5) continue;
    const fade = radialFade(Math.hypot(x - center, y - center), softR);
    const coherence = Math.exp(-rms * 0.22);
    const richness = Math.min(1, Math.log1p(a.count) / Math.log(40));
    const alpha = 0.055 * coherence * richness * fade;
    if (alpha < 0.006) continue;
    const radius = Math.max(5, s.scale * (1.2 + rms * 1.9));
    const px = toCx(x - 0.5);
    const py = toCy(y - 0.5);
    const glow = s.ctx.createRadialGradient(px, py, 0, px, py, radius);
    glow.addColorStop(0, `rgba(222,226,255,${alpha.toFixed(3)})`);
    glow.addColorStop(0.38, `rgba(172,184,238,${(alpha * 0.48).toFixed(3)})`);
    glow.addColorStop(1, "rgba(126,140,210,0)");
    s.ctx.fillStyle = glow;
    s.ctx.beginPath();
    s.ctx.arc(px, py, radius, 0, Math.PI * 2);
    s.ctx.fill();
  }
  s.ctx.globalCompositeOperation = "source-over";
}

// Stars: three brightness tiers, like a long-exposure field. Most stars
// are bare points of their class color; the bright minority get a tight
// glow; only the rare giants (top of the luminosity range) earn
// diffraction spikes.
function drawStars(
  s: State,
  toCx: (x: number) => number,
  toCy: (y: number) => number,
  layer: "embedded" | "exposed"
) {
  const stars = s.lastStars;
  if (!stars || stars.length === 0) return;
  const { ctx, size } = s;
  const maxLum = 120 * 120;
  const softR = size / 2 - 1;
  const center = size / 2;
  // Additive light: overlapping stars brighten instead of occluding,
  // so a dense swarm (cluster core, elliptical spheroid) reads as a
  // glow rather than a sprinkle of isolated dots.
  ctx.globalCompositeOperation = "screen";
  for (let i = 0; i < stars.length; i += galaxy.STAR_RENDER_FLOATS) {
    const reveal = birthReveal(stars, i);
    const layerOpacity = layer === "embedded" ? (1 - reveal) * EMBEDDED_STAR_OPACITY : reveal;
    if (layerOpacity <= 0.01) continue;
    // Radial fade into the halo; deep-halo stars do not render.
    const rad = Math.hypot(stars[i] - center, stars[i + 1] - center);
    const fade = radialFade(rad, softR) * layerOpacity;
    if (fade <= 0.02) continue;
    const px = toCx(stars[i] - 0.5);
    const py = toCy(stars[i + 1] - 0.5);
    // Fourth root compresses the huge mass-luminosity range into a
    // usable brightness scale.
    const stage = Math.round(stars[i + 4]);
    const redGiant = stage === 5;
    const whiteDwarf = stage === 6;
    const neutronCompact = stage >= 2 && stage <= 4;
    const compact = neutronCompact || whiteDwarf;
    const b = redGiant
      ? 0.78
      : compact
        ? stage === 3
          ? 0.44
          : whiteDwarf
            ? 0.36
            : 0.3
        : Math.pow(Math.min(stars[i + 2], maxLum) / maxLum, 0.25);
    const bucket = redGiant
      ? STAR_BUCKET_RED_GIANT
      : compact
        ? whiteDwarf
          ? STAR_BUCKET_WHITE_DWARF
          : STAR_BUCKET_NEUTRON
        : Math.min(STAR_CLASS_BUCKETS - 1, Math.max(0, (stars[i + 3] * STAR_CLASS_BUCKETS) | 0));
    const core = compact ? 0.7 + 0.24 * b : 0.38 + 1.42 * b;
    const alpha = (0.3 + 0.64 * b) * fade;
    if (b > 0.82) {
      // Giants: diffraction spikes plus a tight glow. Spikes are rare
      // enough - only the heaviest main-sequence stars clear the cut -
      // that they stay per-star strokes rather than joining the batch.
      const spike = core * (1.8 + 3.2 * b);
      ctx.globalAlpha = 0.36 * fade;
      ctx.strokeStyle = starColors[bucket];
      ctx.lineWidth = 0.55;
      ctx.beginPath();
      ctx.moveTo(px - spike, py);
      ctx.lineTo(px + spike, py);
      ctx.moveTo(px, py - spike);
      ctx.lineTo(px, py + spike);
      ctx.stroke();
      ctx.globalAlpha = 1.0;
      starBatchPush(bucket, 0.11 * fade, px, py, core * 2.0);
    } else if (b > 0.45) {
      // Mid-bright: a faint tight glow, no spikes.
      starBatchPush(bucket, 0.08 * fade, px, py, core * 1.65);
    }
    starBatchPush(bucket, alpha, px, py, core);
  }
  starBatchFlush(ctx);
  ctx.globalAlpha = 1.0;
  ctx.globalCompositeOperation = "source-over";
}

export function resetView() {
  if (!state) return;
  state.camera = { tx: 0, ty: 0, zoom: 1 };
  publishView(state);
  if (state.lastMass) drawFrame(state, state.lastMass);
}

export function getCamera(): Camera | null {
  if (!state) return null;
  return { ...state.camera };
}

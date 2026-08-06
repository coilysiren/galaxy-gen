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
const FRAME_RATE_PRESENTATION_MULTIPLIER = 4;

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
  cleanup: () => void;
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
  host.setAttribute("data-frame-rate", s.frameAngularRate.toFixed(8));
}

let state: State | null = null;

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
  publishView(state);
  const mass = galaxyFrontend.massArray();
  // Copy so zoom/pan interactions after the sim stops still have data
  // to redraw from.
  state.lastMass = mass.slice();
  state.lastFracX = galaxyFrontend.fracXArray().slice();
  state.lastFracY = galaxyFrontend.fracYArray().slice();
  state.lastStars = galaxyFrontend.starRenderArray().slice();
  state.lastTransients = galaxyFrontend.transientsArray().slice();
  state.lastRadiation = galaxyFrontend.radiationArray().slice();
  state.lastMetallicity = galaxyFrontend.metallicityArray().slice();
  state.lastLensScale = galaxyFrontend.lensScale();
  state.lastStellarHaloMass = galaxyFrontend.stellarHaloMass();
  drawFrame(state, state.lastMass);
}

function drawFrame(s: State, mass: Uint16Array) {
  const { ctx, size, scale, rMax, camera } = s;

  let maxMass = 1;
  for (let i = 0; i < mass.length; i++) {
    if (mass[i] > maxMass) maxMass = mass[i];
  }
  const invLogMax = 1 / Math.log(maxMass + 1);

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
  const fadeEndSq = softR * FADE_END * (softR * FADE_END);
  const softSq = softR * softR;
  const buckets = GAS_TIERS[0].length;
  const rad = s.lastRadiation;
  const radRes = rad ? Math.round(Math.sqrt(rad.length)) : 0;
  const radScale = radRes / size;
  const fracX = s.lastFracX;
  const fracY = s.lastFracY;
  const metallicity = s.lastMetallicity;
  const metalStrengthAt = (i: number): number => {
    const z = metallicity?.[i] ?? 0;
    return Math.min(1, Math.max(0, (z - 0.0015) / 0.02));
  };

  // Recent supernova shells, for the [OIII] teal tier: gas near an
  // expanding front is shock-ionized and glows teal, lingering after
  // the bright wave itself has faded.
  const waves: { x: number; y: number; front: number; band: number }[] = [];
  const tr = s.lastTransients;
  if (tr) {
    for (let k = 0; k < tr.length && waves.length < 16; k += 5) {
      if (tr[k] !== 2 && tr[k] !== 5) continue;
      const front = blastRadius(tr[k + 4], tr[k + 3]);
      waves.push({
        x: tr[k + 1],
        y: tr[k + 2],
        front,
        band: 2.2 + front * 0.12,
      });
    }
  }

  // Coherent dust predicate: a cell is dusty only when dense, cold, and
  // embedded in a thick neighborhood - isolated dense cells stamping
  // dark specks over the gas was the failure mode this replaces.
  const isDusty = (i: number, col: number, row: number): boolean => {
    if (mass[i] < 78) return false;
    if (metalStrengthAt(i) <= 0) return false;
    let thick = 0;
    if (col > 0 && mass[i - 1] >= 55) thick++;
    if (col < size - 1 && mass[i + 1] >= 55) thick++;
    if (row > 0 && mass[i - size] >= 55) thick++;
    if (row < size - 1 && mass[i + size] >= 55) thick++;
    if (thick < 3) return false;
    if (rad && radRes > 0) {
      const fx = Math.min(radRes - 1, (col * radScale) | 0);
      const fy = Math.min(radRes - 1, (row * radScale) | 0);
      if (rad[fy * radRes + fx] > GAS_WARM_RAD) return false;
    }
    return true;
  };

  // Two gas passes split by a stable per-cell hash: most cells render
  // beneath the stars, a foreground share renders over them so clusters
  // sit INSIDE their clouds instead of on top. Dust comes separately.
  const renderGas = (foreground: boolean) => {
    // Screen blending: overlapping clouds glow into each other but
    // saturate smoothly instead of clipping to white.
    ctx.globalCompositeOperation = "screen";
    for (let i = 0; i < mass.length; i++) {
      const m = mass[i];
      if (m === 0) continue;
      if (cellJitter(i, 4) < 0.7 === foreground) continue;
      const col = i % size;
      const row = (i / size) | 0;
      const gasX = col + (fracX?.[i] ?? 0);
      const gasY = row + (fracY?.[i] ?? 0);
      const rx = gasX - center;
      const ry = gasY - center;
      const radSq = rx * rx + ry * ry;
      if (radSq > fadeEndSq) continue;
      const t = Math.log(m + 1) * invLogMax;
      const bi = Math.min(buckets - 1, Math.floor(t * buckets));
      // Continuous temperature: smoothstep crossfades between adjacent
      // tiers instead of hard flips - a cell in a transition zone draws
      // both sprites at fractional weights.
      const smooth = (a: number, b: number, x: number) => {
        const u = Math.min(1, Math.max(0, (x - a) / (b - a)));
        return u * u * (3 - 2 * u);
      };
      let heat = 0;
      if (rad && radRes > 0) {
        const fx = Math.min(radRes - 1, (col * radScale) | 0);
        const fy = Math.min(radRes - 1, (row * radScale) | 0);
        heat = rad[fy * radRes + fx];
      }
      const tierF =
        smooth(GAS_WARM_RAD - 4, GAS_WARM_RAD + 4, heat) +
        smooth(GAS_HOT_RAD - 8, GAS_HOT_RAD + 8, heat);
      // Shock teal fades with distance from the front rather than
      // switching inside a hard annulus.
      let teal = 0;
      for (const w of waves) {
        const d = Math.abs(Math.hypot(col - w.x, row - w.y) - w.front);
        const wgt = 1 - d / w.band;
        if (wgt > teal) teal = wgt;
      }
      teal = teal > 0 ? teal * teal * (3 - 2 * teal) : 0;
      teal *= 0.3 + 0.7 * metalStrengthAt(i);
      // Fuzz overflows the cell on purpose, with per-cell size and
      // brightness jitter so the field is cloudy rather than uniform.
      const footprint = Math.max(7, (0.5 + t * rMax * 1.4) * 6) * (0.75 + cellJitter(i, 1));
      let brightness = 0.48 + 0.54 * cellJitter(i, 2);
      // Dust darkens by emitting less - absence of glow cannot leave
      // overlay artifacts the way a multiply stamp can.
      if (isDusty(i, col, row)) brightness *= 1 - 0.6 * metalStrengthAt(i);
      const alpha = radialFade(Math.sqrt(radSq), softR) * brightness;
      const dx = toCx(gasX) - footprint / 2;
      const dy = toCy(gasY) - footprint / 2;
      const base = Math.min(2, Math.floor(tierF));
      const frac = Math.min(1, tierF - base);
      const temp = 1 - teal;
      const draw = (tier: number, a: number) => {
        if (a < 0.02) return;
        ctx.globalAlpha = a;
        ctx.drawImage(gasSprites[tier][bi], dx, dy, footprint, footprint);
      };
      draw(base, alpha * temp * (1 - frac));
      draw(Math.min(2, base + 1), alpha * temp * frac);
      draw(3, alpha * teal);
    }
    ctx.globalAlpha = 1.0;
    ctx.globalCompositeOperation = "source-over";
  };

  // Dust absorption over the star field: broad, faint multiply blobs in
  // coherent thick-cloud interiors only. The visible dark veining comes
  // from the gas pass emitting less there; this pass exists to dim
  // stars shining through thick clouds.
  const renderDust = () => {
    ctx.globalCompositeOperation = "multiply";
    for (let i = 0; i < mass.length; i++) {
      const col = i % size;
      const row = (i / size) | 0;
      if (!isDusty(i, col, row)) continue;
      if (cellJitter(i, 5) < 0.5) continue;
      const rx = col - center;
      const ry = row - center;
      if (rx * rx + ry * ry > softSq) continue;
      const footprint = Math.max(10, rMax * 12) * (0.8 + 0.5 * cellJitter(i, 6));
      const gasX = col + (fracX?.[i] ?? 0);
      const gasY = row + (fracY?.[i] ?? 0);
      ctx.globalAlpha = 0.06 + 0.18 * metalStrengthAt(i);
      ctx.drawImage(
        dustSprite!,
        toCx(gasX) - footprint / 2,
        toCy(gasY) - footprint / 2,
        footprint,
        footprint
      );
    }
    ctx.globalAlpha = 1.0;
    ctx.globalCompositeOperation = "source-over";
  };

  // Newborn main-sequence stars begin beneath the entire cloud field.
  // As their natal gas moves ahead, age cross-fades them into the normal
  // exposed layer instead of revealing an instantaneous pellet spray.
  drawStars(s, toCx, toCy, "embedded");
  renderGas(false);
  drawAssociations(s, toCx, toCy);
  drawStars(s, toCx, toCy, "exposed");
  // Dust sits under the foreground gas: it still dims the star field,
  // but the glow layer re-softens it so lanes read as embedded darkness
  // rather than holes punched in the clouds.
  renderDust();
  renderGas(true);
  drawTransients(s, toCx, toCy);

  ctx.restore();

  applyShockShimmer(s);
  applyBlackHoleLens(s);
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
  const te2 = te * te;
  const shadowR = te * 0.3;
  const taperStart = R * 0.75;
  for (let py = 0; py < h; py++) {
    const dy = py + y0 - by;
    for (let px = 0; px < w; px++) {
      const dx = px + x0 - bx;
      const r2 = dx * dx + dy * dy;
      if (r2 >= R * R) continue;
      const o = (py * w + px) * 4;
      const r = Math.sqrt(r2) || 1e-3;
      if (r < shadowR) {
        data[o] = 0;
        data[o + 1] = 0;
        data[o + 2] = 4;
        data[o + 3] = 255;
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
      const so = (sy * w + sx) * 4;
      data[o] = src[so];
      data[o + 1] = src[so + 1];
      data[o + 2] = src[so + 2];
      data[o + 3] = src[so + 3];
    }
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
    const [cr, cg, cb] = redGiant
      ? [255, 132, 92]
      : compact
        ? whiteDwarf
          ? [220, 238, 255]
          : [145, 232, 255]
        : classColor(stars[i + 3]);
    const core = compact ? 0.7 + 0.24 * b : 0.38 + 1.42 * b;
    const alpha = (0.3 + 0.64 * b) * fade;
    if (b > 0.82) {
      // Giants: diffraction spikes plus a tight glow.
      const spike = core * (1.8 + 3.2 * b);
      ctx.strokeStyle = `rgba(${cr},${cg},${cb},${(0.36 * fade).toFixed(3)})`;
      ctx.lineWidth = 0.55;
      ctx.beginPath();
      ctx.moveTo(px - spike, py);
      ctx.lineTo(px + spike, py);
      ctx.moveTo(px, py - spike);
      ctx.lineTo(px, py + spike);
      ctx.stroke();
      ctx.fillStyle = `rgba(${cr},${cg},${cb},${(0.11 * fade).toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(px, py, core * 2.0, 0, Math.PI * 2);
      ctx.fill();
    } else if (b > 0.45) {
      // Mid-bright: a faint tight glow, no spikes.
      ctx.fillStyle = `rgba(${cr},${cg},${cb},${(0.08 * fade).toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(px, py, core * 1.65, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = alpha;
    ctx.fillStyle = `rgb(${cr},${cg},${cb})`;
    ctx.beginPath();
    ctx.arc(px, py, core, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1.0;
  }
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

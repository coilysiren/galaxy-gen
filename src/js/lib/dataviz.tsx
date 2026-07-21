import * as galaxy from "./galaxy";

// Canvas, not SVG: 2500+ DOM attrs/frame hits hundreds of ms.

const CANVAS = 800;
const MARGIN = 20;

const MIN_ZOOM = 1;
const MAX_ZOOM = 50;

// Radial render fade: full brightness inside the soft clip (the disk
// radius), fading to invisible by FADE_END x soft. Matter deeper in the
// halo band exists in the sim but does not render - the sim's hard clip
// sits at 3x soft, far past visibility.
const FADE_END = 1.5;

// The canvas views a world span wider than the grid, centered on the
// disk. 1.1 x size lets the disk own the frame; the near-halo spills
// past the canvas edge and the radial fade handles the rest.
const VIEW_SPAN = 1.1;

// Soft nebular sprites for gas, one per color bucket, pre-rendered once.
// drawImage of a gradient sprite is far cheaper than per-cell gradients
// and the alpha accumulation makes dense regions glow on its own.
const GAS_SPRITE_PX = 32;
let gasSprites: HTMLCanvasElement[] = [];

// Cool blue-grey nebular ramp, dim to bright (the warm palette moved to
// the stars, which render as sharp points - the fuzz/color inversion).
const GAS_COLORS: [number, number, number][] = [
  [64, 76, 102],
  [86, 102, 134],
  [110, 130, 168],
  [140, 162, 200],
  [174, 194, 228],
  [210, 226, 252],
];

function buildGasSprites() {
  if (gasSprites.length > 0) return;
  for (const [r, g, b] of GAS_COLORS) {
    const c = document.createElement("canvas");
    c.width = GAS_SPRITE_PX;
    c.height = GAS_SPRITE_PX;
    const cctx = c.getContext("2d")!;
    const half = GAS_SPRITE_PX / 2;
    const grad = cctx.createRadialGradient(half, half, 0, half, half, half);
    grad.addColorStop(0, `rgba(${r},${g},${b},0.5)`);
    grad.addColorStop(0.4, `rgba(${r},${g},${b},0.2)`);
    grad.addColorStop(1, `rgba(${r},${g},${b},0)`);
    cctx.fillStyle = grad;
    cctx.fillRect(0, 0, GAS_SPRITE_PX, GAS_SPRITE_PX);
    gasSprites.push(c);
  }
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
  size: number;
  scale: number;
  rMax: number;
  camera: Camera;
  simTick: number;
  lastMass: Uint16Array | null;
  lastStars: Float32Array | null;
  lastTransients: Float32Array | null;
  cleanup: () => void;
}

// Mirror camera to data-* attrs so E2E tests can observe pan/zoom.
function publishCamera(s: State) {
  const { host, camera } = s;
  host.setAttribute("data-cam-tx", camera.tx.toFixed(2));
  host.setAttribute("data-cam-ty", camera.ty.toFixed(2));
  host.setAttribute("data-cam-zoom", camera.zoom.toFixed(4));
}

let state: State | null = null;

function clearChildren(node: Element) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

function clampZoom(z: number): number {
  return Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, z));
}

// Clamp pan so the world rectangle always intersects the viewport.
function clampPan(cam: Camera): Camera {
  const min = CANVAS * (1 - cam.zoom);
  const tx = Math.max(min, Math.min(0, cam.tx));
  const ty = Math.max(min, Math.min(0, cam.ty));
  return { ...cam, tx, ty };
}

export function initViz(galaxyFrontend: galaxy.Frontend) {
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
  canvas.width = CANVAS * dpr;
  canvas.height = CANVAS * dpr;
  canvas.style.width = "100%";
  canvas.style.height = "auto";
  canvas.style.display = "block";
  canvas.style.aspectRatio = "1 / 1";
  canvas.style.cursor = "grab";
  canvas.style.touchAction = "none";
  canvas.setAttribute("data-testid", "dataviz-canvas");

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.scale(dpr, dpr);

  host.appendChild(canvas);

  buildGasSprites();
  const size = galaxyFrontend.galaxySize;
  const scale = (CANVAS - MARGIN * 2) / (size * VIEW_SPAN);

  const camera: Camera = { tx: 0, ty: 0, zoom: 1 };

  // --- Input handlers: pan + zoom ------------------------------------

  // Convert a pointer event to canvas-local CSS pixels.
  const pointerToCanvas = (ev: MouseEvent | WheelEvent) => {
    const rect = canvas.getBoundingClientRect();
    // CSS pixels are normalized to the logical CANVAS size.
    const x = ((ev.clientX - rect.left) / rect.width) * CANVAS;
    const y = ((ev.clientY - rect.top) / rect.height) * CANVAS;
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
    state.camera = clampPan({ tx, ty, zoom: newZoom });
    publishCamera(state);
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
    const dx = ((ev.clientX - dragStart.x) / rect.width) * CANVAS;
    const dy = ((ev.clientY - dragStart.y) / rect.height) * CANVAS;
    state.camera = clampPan({
      tx: dragCam.tx + dx,
      ty: dragCam.ty + dy,
      zoom: state.camera.zoom,
    });
    publishCamera(state);
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

  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("pointerdown", onPointerDown);
  canvas.addEventListener("pointermove", onPointerMove);
  canvas.addEventListener("pointerup", onPointerUp);
  canvas.addEventListener("pointercancel", onPointerUp);
  canvas.addEventListener("dblclick", onDblClick);

  const cleanup = () => {
    canvas.removeEventListener("wheel", onWheel);
    canvas.removeEventListener("pointerdown", onPointerDown);
    canvas.removeEventListener("pointermove", onPointerMove);
    canvas.removeEventListener("pointerup", onPointerUp);
    canvas.removeEventListener("pointercancel", onPointerUp);
    canvas.removeEventListener("dblclick", onDblClick);
  };

  state = {
    host,
    canvas,
    ctx,
    size,
    scale,
    rMax: scale * 0.5,
    camera,
    simTick: 0,
    lastMass: null,
    lastStars: null,
    lastTransients: null,
    cleanup,
  };
  publishCamera(state);
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
  state.lastMass = mass.slice();
  state.lastStars = galaxyFrontend.starRenderArray().slice();
  state.lastTransients = galaxyFrontend.transientsArray().slice();
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
  ctx.clearRect(0, 0, CANVAS, CANVAS);

  // Apply the camera: screen = zoom * world + translate.
  ctx.translate(camera.tx, camera.ty);
  ctx.scale(camera.zoom, camera.zoom);

  const center = size / 2;
  const half = CANVAS / 2;
  const toCx = (x: number) => half + (x + 0.5 - center) * scale;
  const toCy = (y: number) => half + (center - y - 0.5) * scale;

  // Gas: soft nebular sprites, alpha-accumulating where dense.
  const softR = size / 2 - 1;
  const fadeEndSq = softR * FADE_END * (softR * FADE_END);
  const softSq = softR * softR;
  const buckets = GAS_COLORS.length;

  for (let i = 0; i < mass.length; i++) {
    const m = mass[i];
    if (m === 0) continue;
    const col = i % size;
    const row = (i / size) | 0;
    const rx = col - center;
    const ry = row - center;
    const radSq = rx * rx + ry * ry;
    if (radSq > fadeEndSq) continue;
    const t = Math.log(m + 1) * invLogMax;
    const bi = Math.min(buckets - 1, Math.floor(t * buckets));
    // Fuzz overflows the cell on purpose - a cell's cloud bleeds well
    // into its neighborhood and the overlaps blend into nebulae.
    const footprint = Math.max(5, (0.5 + t * rMax * 1.4) * 8);
    ctx.globalAlpha = radSq > softSq ? 0.3 : 1.0;
    ctx.drawImage(
      gasSprites[bi],
      toCx(col) - footprint / 2,
      toCy(row) - footprint / 2,
      footprint,
      footprint,
    );
  }
  ctx.globalAlpha = 1.0;

  drawStars(s, toCx, toCy);
  drawTransients(s, toCx, toCy);

  ctx.restore();
}

// Event flashes: an expanding, fading ring for each recent supernova and
// a brief glint for each star birth. Duration and brightness are render
// exaggerations of instantaneous events - nothing here is sim state.
function drawTransients(
  s: State,
  toCx: (x: number) => number,
  toCy: (y: number) => number,
) {
  const t = s.lastTransients;
  if (!t || t.length === 0) return;
  const { ctx, scale } = s;
  for (let i = 0; i < t.length; i += 4) {
    const kind = t[i];
    const px = toCx(t[i + 1]);
    const py = toCy(t[i + 2]);
    const age = t[i + 3];
    if (kind === 2) {
      // Supernova: bright core flash then an expanding shell. Kept faint
      // and short-lived - on a space-black field a busy epoch otherwise
      // drowns the galaxy in rings.
      const life = 1 - age / 55;
      if (life <= 0) continue;
      const ringR = (1.5 + age * 0.35) * scale;
      ctx.strokeStyle = `rgba(255,240,210,${(0.32 * life).toFixed(3)})`;
      ctx.lineWidth = 1.6 * life;
      ctx.beginPath();
      ctx.arc(px, py, ringR, 0, Math.PI * 2);
      ctx.stroke();
      if (age < 12) {
        const coreLife = 1 - age / 12;
        ctx.fillStyle = `rgba(255,255,245,${(0.9 * coreLife).toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(px, py, (2.5 + age * 0.2) * scale * 0.6, 0, Math.PI * 2);
        ctx.fill();
      }
    } else if (kind === 1 && age < 30) {
      // Star birth: soft glint.
      const life = 1 - age / 30;
      ctx.fillStyle = `rgba(200,220,255,${(0.35 * life).toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(px, py, (1.2 + age * 0.05) * scale, 0, Math.PI * 2);
      ctx.fill();
    }
  }
}

// Stars: bright glowing points over the gas layer. Color runs cool
// (light stars, warm cream) to hot (heavy stars, blue-white); size and
// halo derive from luminosity. Render-only exaggeration is fine - none
// of this flows back into the sim.
// Stars: sharp warm points - crisp at a distance, the way stars read in
// a photograph. Light stars are cream, heavy ones blue-white. Only the
// very brightest get a faint tight glow; there is no soft halo (the
// fuzz belongs to the gas now).
function drawStars(
  s: State,
  toCx: (x: number) => number,
  toCy: (y: number) => number,
) {
  const stars = s.lastStars;
  if (!stars || stars.length === 0) return;
  const { ctx, size } = s;
  const maxLum = 1200;
  const softR = size / 2 - 1;
  const center = size / 2;
  for (let i = 0; i < stars.length; i += 4) {
    // Radial fade into the halo; deep-halo stars do not render.
    const rad = Math.hypot(stars[i] - center, stars[i + 1] - center);
    const fade =
      rad <= softR ? 1 : Math.max(0, 1 - (rad / softR - 1) / (FADE_END - 1));
    if (fade <= 0.02) continue;
    ctx.globalAlpha = fade;
    const px = toCx(stars[i] - 0.5);
    const py = toCy(stars[i + 1] - 0.5);
    const lum = Math.min(stars[i + 2], maxLum) / maxLum;
    const heat = stars[i + 3];
    const r = 0.9 + 1.6 * Math.sqrt(lum);
    const cr = (255 - 45 * heat) | 0;
    const cg = (240 - 18 * heat) | 0;
    const cb = (208 + 47 * heat) | 0;
    if (lum > 0.45) {
      ctx.fillStyle = `rgba(${cr},${cg},${cb},0.18)`;
      ctx.beginPath();
      ctx.arc(px, py, r * 1.8, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.fillStyle = `rgb(${cr},${cg},${cb})`;
    ctx.beginPath();
    ctx.arc(px, py, r, 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.globalAlpha = 1.0;
}

export function resetView() {
  if (!state) return;
  state.camera = { tx: 0, ty: 0, zoom: 1 };
  publishCamera(state);
  if (state.lastMass) drawFrame(state, state.lastMass);
}

export function getCamera(): Camera | null {
  if (!state) return null;
  return { ...state.camera };
}

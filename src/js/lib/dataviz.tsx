import * as galaxy from "./galaxy";

// Canvas, not SVG: 2500+ DOM attrs/frame hits hundreds of ms.

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
      grad.addColorStop(0, `rgba(${r},${g},${b},0.16)`);
      grad.addColorStop(0.35, `rgba(${r},${g},${b},0.07)`);
      grad.addColorStop(0.7, `rgba(${r},${g},${b},0.025)`);
      grad.addColorStop(1, `rgba(${r},${g},${b},0)`);
      cctx.fillStyle = grad;
      cctx.fillRect(0, 0, GAS_SPRITE_PX, GAS_SPRITE_PX);
      sprites.push(c);
    }
    gasSprites.push(sprites);
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
  simTick: number;
  lastMass: Uint16Array | null;
  lastStars: Float32Array | null;
  lastTransients: Float32Array | null;
  lastRadiation: Float32Array | null;
  lastLensScale: number;
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
function clampPan(cam: Camera, cw: number, ch: number): Camera {
  const tx = Math.max(cw * (1 - cam.zoom), Math.min(0, cam.tx));
  const ty = Math.max(ch * (1 - cam.zoom), Math.min(0, cam.ty));
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
    typeof window !== "undefined" &&
    new URLSearchParams(window.location.search).has("debug");
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
    const dx = ((ev.clientX - dragStart.x) / rect.width) * state.cw;
    const dy = ((ev.clientY - dragStart.y) / rect.height) * state.ch;
    state.camera = clampPan(
      {
        tx: dragCam.tx + dx,
        ty: dragCam.ty + dy,
        zoom: state.camera.zoom,
      },
      state.cw,
      state.ch,
    );
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
    publishCamera(state);
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
    simTick: 0,
    lastMass: null,
    lastStars: null,
    lastTransients: null,
    lastRadiation: null,
    lastLensScale: 1,
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
  state.lastRadiation = galaxyFrontend.radiationArray().slice();
  state.lastLensScale = galaxyFrontend.lensScale();
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
  ctx.clearRect(0, 0, s.cw, s.ch);

  // Apply the camera: screen = zoom * world + translate.
  ctx.translate(camera.tx, camera.ty);
  ctx.scale(camera.zoom, camera.zoom);

  const center = size / 2;
  const toCx = (x: number) => s.cw / 2 + (x + 0.5 - center) * scale;
  const toCy = (y: number) => s.ch / 2 + (center - y - 0.5) * scale;

  // Gas: soft nebular sprites, alpha-accumulating where dense.
  const softR = size / 2 - 1;
  const fadeEndSq = softR * FADE_END * (softR * FADE_END);
  const softSq = softR * softR;
  const buckets = GAS_TIERS[0].length;
  const rad = s.lastRadiation;
  const radRes = rad ? Math.round(Math.sqrt(rad.length)) : 0;
  const radScale = radRes / size;

  // Screen blending: overlapping clouds glow into each other but
  // saturate smoothly instead of clipping to white the way additive
  // blending does - cores stay violet.
  ctx.globalCompositeOperation = "screen";
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
    // Temperature tier from the radiation field, dithered per cell so
    // the boundaries stay organic.
    let tier = 0;
    if (rad && radRes > 0) {
      const fx = Math.min(radRes - 1, (col * radScale) | 0);
      const fy = Math.min(radRes - 1, (row * radScale) | 0);
      const heat = rad[fy * radRes + fx] + (cellJitter(i, 3) - 0.5) * 6;
      if (heat > GAS_HOT_RAD) tier = 2;
      else if (heat > GAS_WARM_RAD) tier = 1;
    }
    // Fuzz overflows the cell on purpose, with per-cell size and
    // brightness jitter so the field is cloudy rather than uniform.
    const footprint =
      Math.max(8, (0.5 + t * rMax * 1.4) * 10) * (0.75 + cellJitter(i, 1));
    const brightness = 0.45 + 0.75 * cellJitter(i, 2);
    ctx.globalAlpha = (radSq > softSq ? 0.3 : 1.0) * brightness;
    ctx.drawImage(
      gasSprites[tier][bi],
      toCx(col) - footprint / 2,
      toCy(row) - footprint / 2,
      footprint,
      footprint,
    );
  }
  ctx.globalAlpha = 1.0;
  ctx.globalCompositeOperation = "source-over";

  drawStars(s, toCx, toCy);
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
    if (t[i] !== 2) continue;
    const age = t[i + 3];
    const life = 1 - age / BLAST_LIFE;
    if (life <= 0.15) continue;
    // World -> canvas -> screen css -> device.
    const cx = s.cw / 2 + (t[i + 1] + 0.5 - center) * scale;
    const cy = s.ch / 2 + (center - t[i + 2] - 0.5) * scale;
    const sx = (camera.zoom * cx + camera.tx) * dpr;
    const sy = (camera.zoom * cy + camera.ty) * dpr;
    const front = blastRadius(t[i + 4], age) * scale * camera.zoom * dpr;
    if (front < 6) continue;
    if (sx + front < 0 || sy + front < 0 || sx - front > canvas.width || sy - front > canvas.height) {
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

// Event flashes: supernova blast waves and star-birth glints. Duration
// and brightness are render exaggerations of instantaneous events -
// nothing here is sim state.
function drawTransients(
  s: State,
  toCx: (x: number) => number,
  toCy: (y: number) => number,
) {
  const t = s.lastTransients;
  if (!t || t.length === 0) return;
  const { ctx, scale } = s;
  for (let i = 0; i < t.length; i += 5) {
    const kind = t[i];
    const px = toCx(t[i + 1]);
    const py = toCy(t[i + 2]);
    const age = t[i + 3];
    const mag = t[i + 4];
    if (kind === 2) {
      // Supernova: a shell with a bright leading edge and a fading wake
      // - a wave, not a stroked circle. Size and brightness follow the
      // progenitor's stellar class, but stay understated: a big epoch
      // fires many at once.
      const life = 1 - age / BLAST_LIFE;
      if (life <= 0) continue;
      const heft = Math.min(mag / 120, 1);
      const front = blastRadius(mag, age) * scale;
      const inner = Math.max(front * 0.6, front - (2 + 2 * heft) * scale);
      const peak = (0.12 + 0.13 * heft) * life;
      const g = ctx.createRadialGradient(px, py, inner, px, py, front);
      g.addColorStop(0, "rgba(255,236,200,0)");
      g.addColorStop(0.55, `rgba(255,228,185,${(peak * 0.35).toFixed(3)})`);
      g.addColorStop(0.92, `rgba(255,244,222,${peak.toFixed(3)})`);
      g.addColorStop(1, "rgba(255,250,240,0)");
      ctx.fillStyle = g;
      ctx.beginPath();
      ctx.arc(px, py, front, 0, Math.PI * 2);
      ctx.fill();
      // Leading edge, faint.
      ctx.strokeStyle = `rgba(255,246,226,${(peak * 0.55).toFixed(3)})`;
      ctx.lineWidth = 0.7;
      ctx.beginPath();
      ctx.arc(px, py, front * 0.985, 0, Math.PI * 2);
      ctx.stroke();
      if (age < 10) {
        const coreLife = 1 - age / 10;
        const coreAlpha = 0.4 + 0.25 * heft;
        ctx.fillStyle = `rgba(255,255,245,${(coreAlpha * coreLife).toFixed(3)})`;
        ctx.beginPath();
        ctx.arc(px, py, (1.2 + heft * 1.5 + age * 0.15) * scale * 0.45, 0, Math.PI * 2);
        ctx.fill();
      }
    } else if (kind === 1 && age < 18) {
      // Star birth: a quick mint-green sparkle with tiny spikes. No real
      // astro analog - and green on purpose: no star is ever green, so
      // the birth marker cannot be confused with a bright cluster star.
      const life = 1 - age / 18;
      const budget = Math.min(mag / 250, 1);
      const r = (0.5 + 0.6 * budget) * scale * (0.7 + 0.5 * (1 - life));
      const a = 0.85 * life * life;
      ctx.strokeStyle = `rgba(140,240,190,${(a * 0.7).toFixed(3)})`;
      ctx.lineWidth = 0.6;
      const spike = r * 2.6 * life;
      ctx.beginPath();
      ctx.moveTo(px - spike, py);
      ctx.lineTo(px + spike, py);
      ctx.moveTo(px, py - spike);
      ctx.lineTo(px, py + spike);
      ctx.stroke();
      ctx.fillStyle = `rgba(185,255,220,${a.toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(px, py, r, 0, Math.PI * 2);
      ctx.fill();
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

// Stars: three brightness tiers, like a long-exposure field. Most stars
// are bare points of their class color; the bright minority get a tight
// glow; only the rare giants (top of the luminosity range) earn
// diffraction spikes.
function drawStars(
  s: State,
  toCx: (x: number) => number,
  toCy: (y: number) => number,
) {
  const stars = s.lastStars;
  if (!stars || stars.length === 0) return;
  const { ctx, size } = s;
  const maxLum = 120 * 120;
  const softR = size / 2 - 1;
  const center = size / 2;
  for (let i = 0; i < stars.length; i += 4) {
    // Radial fade into the halo; deep-halo stars do not render.
    const rad = Math.hypot(stars[i] - center, stars[i + 1] - center);
    const fade =
      rad <= softR ? 1 : Math.max(0, 1 - (rad / softR - 1) / (FADE_END - 1));
    if (fade <= 0.02) continue;
    const px = toCx(stars[i] - 0.5);
    const py = toCy(stars[i + 1] - 0.5);
    // Fourth root compresses the huge mass-luminosity range into a
    // usable brightness scale.
    const b = Math.pow(Math.min(stars[i + 2], maxLum) / maxLum, 0.25);
    const [cr, cg, cb] = classColor(stars[i + 3]);
    const core = 0.4 + 1.7 * b;
    const alpha = (0.3 + 0.7 * b) * fade;
    if (b > 0.62) {
      // Giants: diffraction spikes plus a tight glow.
      const spike = core * (2.0 + 5.0 * b);
      ctx.strokeStyle = `rgba(${cr},${cg},${cb},${(0.5 * fade).toFixed(3)})`;
      ctx.lineWidth = 0.7;
      ctx.beginPath();
      ctx.moveTo(px - spike, py);
      ctx.lineTo(px + spike, py);
      ctx.moveTo(px, py - spike);
      ctx.lineTo(px, py + spike);
      ctx.stroke();
      ctx.fillStyle = `rgba(${cr},${cg},${cb},${(0.16 * fade).toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(px, py, core * 2.4, 0, Math.PI * 2);
      ctx.fill();
    } else if (b > 0.45) {
      // Mid-bright: a faint tight glow, no spikes.
      ctx.fillStyle = `rgba(${cr},${cg},${cb},${(0.12 * fade).toFixed(3)})`;
      ctx.beginPath();
      ctx.arc(px, py, core * 1.9, 0, Math.PI * 2);
      ctx.fill();
    }
    ctx.globalAlpha = alpha;
    ctx.fillStyle = `rgb(${cr},${cg},${cb})`;
    ctx.beginPath();
    ctx.arc(px, py, core, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1.0;
  }
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

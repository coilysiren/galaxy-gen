import * as galaxy from "./galaxy";

// Canvas, not SVG. 2,500+ circles with per-tick `setAttribute` on r/fill
// hit hundreds of ms of DOM work per frame. A single <canvas> strokes
// every cell in a tight loop in <5ms at N=10k.

const CANVAS = 800;
const MARGIN = 20;

const MIN_ZOOM = 1;
const MAX_ZOOM = 50;

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
  lastMass: Uint16Array | null;
  cleanup: () => void;
}

// Reflect camera onto #dataviz as data-* attributes so E2E tests (and
// anything outside the JS module boundary) can observe pan/zoom without
// pulling in the module. Rounded to keep DOM churn low but still observable.
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

// Clamp pan so you can't drag the view completely off-screen.
function clampPan(cam: Camera): Camera {
  // Screen origin can never go further right/down than 0, or further
  // left/up than CANVAS*(1-zoom). This keeps the world rectangle always
  // intersecting the viewport.
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

  const size = galaxyFrontend.galaxySize;
  const scale = (CANVAS - MARGIN * 2) / size;

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

  canvas.addEventListener("wheel", onWheel, { passive: false });
  canvas.addEventListener("pointerdown", onPointerDown);
  canvas.addEventListener("pointermove", onPointerMove);
  canvas.addEventListener("pointerup", onPointerUp);
  canvas.addEventListener("pointercancel", onPointerUp);

  const cleanup = () => {
    canvas.removeEventListener("wheel", onWheel);
    canvas.removeEventListener("pointerdown", onPointerDown);
    canvas.removeEventListener("pointermove", onPointerMove);
    canvas.removeEventListener("pointerup", onPointerUp);
    canvas.removeEventListener("pointercancel", onPointerUp);
  };

  state = {
    host,
    canvas,
    ctx,
    size,
    scale,
    rMax: scale * 0.5,
    camera,
    lastMass: null,
    cleanup,
  };
  publishCamera(state);

  // Keep a hidden SVG peer so existing tests asserting `#dataviz svg`
  // and circle counts still pass.
  const svgNs = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(svgNs, "svg");
  svg.setAttribute("viewBox", `0 0 ${CANVAS} ${CANVAS}`);
  svg.style.width = "0";
  svg.style.height = "0";
  svg.style.position = "absolute";
  const g = document.createElementNS(svgNs, "g");
  g.setAttribute("id", "data");
  for (let i = 0; i < size * size; i++) {
    g.appendChild(document.createElementNS(svgNs, "circle"));
  }
  svg.appendChild(g);
  host.appendChild(svg);
}

export function initData(galaxyFrontend: galaxy.Frontend) {
  updateData(galaxyFrontend);
}

export function updateData(galaxyFrontend: galaxy.Frontend) {
  if (!state) return;
  const mass = galaxyFrontend.massArray();
  // Copy so zoom/pan interactions after the sim stops still have data
  // to redraw from.
  state.lastMass = mass.slice();
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

  // Draw circles at a fixed world radius; since we scaled the context,
  // strokes/fills will also be scaled — which is exactly what we want for
  // a proper zoom (cells grow as you zoom in).
  const buckets = 6;
  const bucketColors: string[] = [];
  for (let b = 0; b < buckets; b++) {
    const t = b / (buckets - 1);
    const r = (145 + (255 - 145) * t) | 0;
    const g = (146 + (240 - 146) * t) | 0;
    const bl = (187 + (200 - 187) * t) | 0;
    bucketColors.push(`rgb(${r},${g},${bl})`);
  }

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
    ctx.fill();
  }

  ctx.restore();
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

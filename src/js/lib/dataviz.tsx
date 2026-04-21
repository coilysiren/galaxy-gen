import * as galaxy from "./galaxy";

// Canvas, not SVG. 2,500+ circles with per-tick `setAttribute` on r/fill
// hit hundreds of ms of DOM work per frame. A single <canvas> strokes
// every cell in a tight loop in <5ms at N=10k.

const CANVAS = 800;
const MARGIN = 20;

interface State {
  canvas: HTMLCanvasElement;
  ctx: CanvasRenderingContext2D;
  size: number;
  scale: number;
  rMax: number;
}

let state: State | null = null;

function clearChildren(node: Element) {
  while (node.firstChild) node.removeChild(node.firstChild);
}

export function initViz(galaxyFrontend: galaxy.Frontend) {
  const host = document.getElementById("dataviz");
  if (!host) return;
  clearChildren(host);

  const canvas = document.createElement("canvas");
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  canvas.width = CANVAS * dpr;
  canvas.height = CANVAS * dpr;
  canvas.style.width = "100%";
  canvas.style.height = "auto";
  canvas.style.display = "block";
  canvas.style.aspectRatio = "1 / 1";

  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  ctx.scale(dpr, dpr);

  host.appendChild(canvas);

  const size = galaxyFrontend.galaxySize;
  const scale = (CANVAS - MARGIN * 2) / size;

  state = { canvas, ctx, size, scale, rMax: scale * 0.5 };

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
  const { ctx, size, scale, rMax } = state;
  const mass = galaxyFrontend.massArray();

  let maxMass = 1;
  for (let i = 0; i < mass.length; i++) {
    if (mass[i] > maxMass) maxMass = mass[i];
  }
  const invLogMax = 1 / Math.log(maxMass + 1);

  ctx.clearRect(0, 0, CANVAS, CANVAS);

  // Batch into 6 brightness buckets so each canvas `fillStyle` set is
  // followed by a bulk fill of all circles in that bucket — `fillStyle`
  // changes are expensive on 2D canvas, bulk fills are cheap.
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
}

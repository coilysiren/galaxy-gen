import * as d3 from "d3";
import * as galaxy from "./galaxy";

// Logical canvas side length in px. The SVG uses a viewBox so the actual
// rendered size scales to the panel width while coordinates stay stable.
const CANVAS = 800;
const MARGIN = 20;

// Keep a single DOM element per cell and mutate cx/cy/r/opacity on tick.
// Rebuilding the whole <g> every tick (the old code) triggered a full
// style + layout recalc for every cell every frame — dominated render
// cost once the sim ran continuously.

export function initViz(galaxyFrontend: galaxy.Frontend) {
  d3.select("#dataviz svg").remove();

  const svg = d3
    .select("#dataviz")
    .append("svg")
    .attr("viewBox", `0 0 ${CANVAS} ${CANVAS}`)
    .attr("preserveAspectRatio", "xMidYMid meet")
    .style("display", "block")
    .style("width", "100%")
    .style("height", "auto");

  svg.append("g").attr("id", "data");

  // Prime the grid so we only run enter() once. Every subsequent tick
  // just mutates attributes on the already-created circles.
  const mass = galaxyFrontend.massArray();
  const size = galaxyFrontend.galaxySize;
  const scale = (CANVAS - MARGIN * 2) / size;

  svg
    .select<SVGGElement>("#data")
    .selectAll<SVGCircleElement, number>("circle")
    .data(new Array(mass.length).fill(0), (_d, i) => i)
    .enter()
    .append("circle")
    .attr("cx", (_d, i) => MARGIN + (((i % size) + 0.5) as number) * scale)
    .attr(
      "cy",
      (_d, i) =>
        MARGIN +
        ((size - 1 - (((i / size) | 0) as number) + 0.5) as number) * scale
    )
    .attr("r", 0)
    .style("fill", "#9192bb");
}

/** Populate the just-seeded galaxy. Same path as `updateData`. */
export function initData(galaxyFrontend: galaxy.Frontend) {
  updateData(galaxyFrontend);
}

/** Fast-path per-tick update. Reads `massArray()` directly and mutates
 *  existing circles' radius + fill colour. Heavier cells get brighter,
 *  warmer stars; empty cells get r=0 (invisible but kept in the DOM). */
export function updateData(galaxyFrontend: galaxy.Frontend) {
  const mass = galaxyFrontend.massArray();
  const size = galaxyFrontend.galaxySize;
  const scale = (CANVAS - MARGIN * 2) / size;
  const rMax = scale * 0.5;

  // Compute max mass to scale visuals dynamically. O(N) but trivial vs
  // the gravity O(N²) we just finished.
  let maxMass = 1;
  for (let i = 0; i < mass.length; i++) {
    if (mass[i] > maxMass) maxMass = mass[i];
  }
  const invLogMax = 1 / Math.log(maxMass + 1);

  const node = document.getElementById("dataviz");
  if (!node) return;
  const g = node.querySelector("svg > #data");
  if (!g) return;

  const circles = g.children;
  const n = Math.min(circles.length, mass.length);

  for (let i = 0; i < n; i++) {
    const c = circles[i] as SVGCircleElement;
    const m = mass[i];
    if (m === 0) {
      if (c.getAttribute("r") !== "0") c.setAttribute("r", "0");
      continue;
    }
    const t = Math.log(m + 1) * invLogMax; // 0..1 in log-mass space
    const r = Math.max(0.5, Math.min(rMax, 0.5 + t * rMax * 1.4));
    c.setAttribute("r", r.toFixed(2));

    // Fill: plum (#9192bb) → hot white-yellow as mass grows.
    //   t=0   → rgb(145,146,187)  (plum-400)
    //   t=1   → rgb(255,240,200)  (warm white)
    const rC = ((145 + (255 - 145) * t) | 0).toString();
    const gC = ((146 + (240 - 146) * t) | 0).toString();
    const bC = ((187 + (200 - 187) * t) | 0).toString();
    c.setAttribute("fill", `rgb(${rC},${gC},${bC})`);
  }
}

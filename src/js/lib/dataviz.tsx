import * as d3 from "d3";
import * as galaxy from "./galaxy";

// Logical canvas side length in px. The SVG uses a viewBox so the actual
// rendered size scales to the panel width while coordinates stay stable.
const CANVAS = 800;
const MARGIN = 20;

function cellScale(galaxyFrontend: galaxy.Frontend) {
  const usable = CANVAS - MARGIN * 2;
  return usable / galaxyFrontend.galaxySize;
}

export function initViz(galaxyFrontend: galaxy.Frontend) {
  d3.select("#dataviz svg").remove();

  d3.select("#dataviz")
    .append("svg")
    .attr("viewBox", `0 0 ${CANVAS} ${CANVAS}`)
    .attr("preserveAspectRatio", "xMidYMid meet")
    .style("display", "block")
    .style("width", "100%")
    .style("height", "auto");
}

export function initData(galaxyFrontend: galaxy.Frontend) {
  const scale = cellScale(galaxyFrontend);
  const size = galaxyFrontend.galaxySize;

  d3.select("#dataviz svg #data").remove();

  const svg = d3.select("#dataviz svg");
  svg
    .append("g")
    .attr("id", "data")
    .selectAll("circle")
    .data(galaxyFrontend.cells())
    .join("circle")
    .attr("cx", (c: galaxy.Cell) => MARGIN + (c.x + 0.5) * scale)
    .attr("cy", (c: galaxy.Cell) => MARGIN + (size - 1 - c.y + 0.5) * scale)
    .attr("r", (c: galaxy.Cell) => {
      const logMass = Math.log(c.mass);
      if (!Number.isFinite(logMass) || logMass <= 0) return 0;
      return Math.min(logMass * 0.6, scale * 0.45);
    })
    .style("fill", "#9192bb");
}

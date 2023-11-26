import React from "react";
import * as d3 from "d3";
import * as galaxy from "./galaxy";

const margin = { top: 40, right: 40, bottom: 40, left: 40 };

function getSizeModifier(galaxyFrontend: galaxy.Frontend) {
  // TODO: flexible scaling
  return Math.sqrt(galaxyFrontend.galaxySize * 10);
}

export function initViz(galaxyFrontend: galaxy.Frontend) {
  const sizeModifier = getSizeModifier(galaxyFrontend);

  // remove old svg
  d3.select("#dataviz svg").remove();

  // append the svg object to the body of the page
  const svg = d3
    .select("#dataviz")
    .append("svg")
    .style("overflow", "visible")
    .append("g")
    .attr("id", "axis")
    .attr("transform", `translate(${margin.left}, ${margin.top})`);

  // Add X axis
  const x = d3
    .scaleLinear()
    .domain([0, galaxyFrontend.galaxySize])
    .range([0, galaxyFrontend.galaxySize * sizeModifier]);

  // Add Y axis
  const y = d3
    .scaleLinear()
    .domain([0, galaxyFrontend.galaxySize])
    .range([galaxyFrontend.galaxySize * sizeModifier, 0]);
}

export function initData(galaxyFrontend: galaxy.Frontend) {
  const sizeModifier = getSizeModifier(galaxyFrontend);

  // remove old data
  d3.select("#dataviz svg #data").remove();

  // append the svg object to the body of the page
  const svg = d3.select("#dataviz svg");
  svg
    .append("g")
    .attr("id", "data")
    .selectAll("dot")
    .data(galaxyFrontend.cells())
    .join("circle")
    .attr("cx", function (c: galaxy.Cell) {
      return Math.round(c.x * sizeModifier + margin.left);
    })
    .attr("cy", function (c: galaxy.Cell) {
      return Math.round(c.y * sizeModifier + margin.top);
    })
    .attr("r", function (c: galaxy.Cell) {
      return Math.log(c.mass) > 0 ? Math.log(c.mass) : 0;
    })
    .style("fill", "#69b3a2");
}

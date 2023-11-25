import React from "react";
import * as d3 from "d3";
import * as galaxy from "./galaxy";

const margin = { top: 40, right: 40, bottom: 40, left: 40 };

function getSizeModifier(galaxyFrontend: galaxy.Frontend) {
  return Math.sqrt(galaxyFrontend.galaxySize);
}

export function initViz(galaxyFrontend: galaxy.Frontend) {
  const sizeModifier = getSizeModifier(galaxyFrontend);
  const width = galaxyFrontend.galaxySize + margin.left + margin.right;
  const height = galaxyFrontend.galaxySize + margin.top + margin.bottom;

  // remove old svg
  d3.select("#dataviz svg").remove();

  // append the svg object to the body of the page
  const svg = d3
    .select("#dataviz")
    .append("svg")
    .attr("width", width * sizeModifier + margin.left + margin.right)
    .attr("height", height * sizeModifier + margin.top + margin.bottom)
    .append("g")
    .attr("transform", `translate(${margin.left}, ${margin.top})`);

  // Add X axis
  const x = d3
    .scaleLinear()
    .domain([0, galaxyFrontend.galaxySize])
    .range([0, galaxyFrontend.galaxySize * sizeModifier]);
  svg
    .append("g")
    .attr(
      "transform",
      `translate(0, ${galaxyFrontend.galaxySize * sizeModifier})`
    )
    .call(d3.axisBottom(x));

  // Add Y axis
  const y = d3
    .scaleLinear()
    .domain([0, galaxyFrontend.galaxySize])
    .range([galaxyFrontend.galaxySize * sizeModifier, 0]);
  svg.append("g").call(d3.axisLeft(y));
}

export function initData(galaxyFrontend: galaxy.Frontend) {
  const sizeModifier = getSizeModifier(galaxyFrontend);

  // remove old data
  d3.select("#dataviz svg circle").remove();

  // append the svg object to the body of the page
  const svg = d3.select("#dataviz svg");
  svg
    .append("g")
    .selectAll("dot")
    .data(galaxyFrontend.cells())
    .join("circle")
    .attr("cx", function (c: galaxy.Cell) {
      return c.x * sizeModifier + margin.left;
    })
    .attr("cy", function (c: galaxy.Cell) {
      return c.y * sizeModifier + margin.top;
    })
    .attr("r", function (c: galaxy.Cell) {
      return c.mass;
    })
    .style("fill", "#69b3a2");
}

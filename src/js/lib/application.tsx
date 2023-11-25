import React from "react";
import "./styles.css";
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

export function Interface() {
  const [galaxySize, setGalaxySize] = React.useState(100);
  const [galaxySeedMass, setGalaxySeedMass] = React.useState(5);
  const [minStarMass, setMinStarMass] = React.useState(1000);
  let wasmModule: any = null;
  let galaxyFrontend: galaxy.Frontend = null;

  wasm.then((module) => {
    console.log("wasm module loaded: ", module);
    wasmModule = module;
  });

  const handleGalaxySizeChange = (
    event: React.ChangeEvent<HTMLInputElement>
  ) => {
    const value = parseInt(event.target.value);
    setGalaxySize(Number.isNaN(value) ? 0 : value);
  };

  const galaxySizeInput = (
    <div className="input-group mb-3">
      <span className="input-group-text" id="basic-addon1">
        Galaxy Size:
      </span>
      <input
        type="text"
        className="form-control"
        name="galaxySize"
        value={galaxySize.toString()}
        onChange={handleGalaxySizeChange}
      />
    </div>
  );

  const handleGalaxySeedMassChange = (
    event: React.ChangeEvent<HTMLInputElement>
  ) => {
    const value = parseInt(event.target.value);
    setGalaxySeedMass(Number.isNaN(value) ? 0 : value);
  };

  const galaxySeedMassInput = (
    <div className="input-group mb-3">
      <span className="input-group-text" id="basic-addon1">
        Seed Mass:
      </span>
      <input
        type="text"
        className="form-control"
        name="galaxySeedMass"
        value={galaxySeedMass.toString()}
        onChange={handleGalaxySeedMassChange}
      />
    </div>
  );

  const handleMinStarMassChange = (
    event: React.ChangeEvent<HTMLInputElement>
  ) => {
    const value = parseInt(event.target.value);
    setMinStarMass(Number.isNaN(value) ? 0 : value);
  };

  const minStarMassInput = (
    <div className="input-group mb-3">
      <span className="input-group-text" id="basic-addon1">
        Min Star Mass:
      </span>
      <input
        type="text"
        className="form-control"
        name="minStarMass"
        value={minStarMass.toString()}
        onChange={handleMinStarMassChange}
      />
    </div>
  );

  const handleInitClick = () => {
    if (wasmModule === null) {
      console.error("wasm not yet loaded");
    } else {
      console.log("initializing galaxy");
      galaxyFrontend = new galaxy.Frontend(galaxySize, minStarMass);
      dataviz.initViz(galaxyFrontend);
    }
  };

  const initButton = (
    <button type="button" className="btn btn-primary" onClick={handleInitClick}>
      init new galaxy
    </button>
  );

  const handleSeedClick = () => {
    if (galaxyFrontend === null) {
      console.error("galaxy not yet initialized");
    } else {
      console.log("seeding galaxy");
      galaxyFrontend.seed(galaxySeedMass);
      dataviz.initData(galaxyFrontend);
    }
  };

  const seedButton = (
    <button type="button" className="btn btn-primary" onClick={handleSeedClick}>
      seed the galaxy
    </button>
  );

  const handleTickClick = () => {
    galaxyFrontend.tick(minStarMass);
  };

  const tickButton = (
    <button type="button" className="btn btn-primary" onClick={handleTickClick}>
      advance time
    </button>
  );

  return (
    <div className="container">
      <h1>Galaxy Generator</h1>
      <h2>
        <small className="text-muted">
          ( rust =&gt; wasm =&gt; js ) galaxy generation simulation
        </small>
      </h2>
      {galaxySizeInput}
      {galaxySeedMassInput}
      {minStarMassInput}
      <div className="d-flex justify-content-between">
        {initButton}
        {seedButton}
        {tickButton}
      </div>
      <div>
        <div id="dataviz"></div>
      </div>
    </div>
  );
}

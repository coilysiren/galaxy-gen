import React from "react";
import "./styles.css";
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

export function Interface() {
  const [galaxySize, setGalaxySize] = React.useState(50);
  const [galaxySeedMass, setGalaxySeedMass] = React.useState(25);
  const [timeModifier, setTimeModifier] = React.useState(0.01);
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

  const handletimeModifierChange = (
    event: React.ChangeEvent<HTMLInputElement>
  ) => {
    const value = parseInt(event.target.value);
    setTimeModifier(Number.isNaN(value) ? 0 : value);
  };

  const timeModifierInput = (
    <div className="input-group mb-3">
      <span className="input-group-text" id="basic-addon1">
        Time Modifier:
      </span>
      <input
        type="text"
        className="form-control"
        name="timeModifier"
        value={timeModifier.toString()}
        onChange={handletimeModifierChange}
      />
    </div>
  );

  const handleInitClick = () => {
    if (wasmModule === null) {
      console.error("wasm not yet loaded");
    } else {
      console.log("initializing galaxy");
      galaxyFrontend = new galaxy.Frontend(galaxySize, timeModifier);
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
      console.log("adding mass to the galaxy");
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
    console.log("advancing time");
    galaxyFrontend.tick(timeModifier);
    dataviz.initData(galaxyFrontend);
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
      {timeModifierInput}
      <div className="d-flex justify-content-between">
        {initButton}
        {seedButton}
        {tickButton}
      </div>
      <div id="dataviz"></div>
    </div>
  );
}

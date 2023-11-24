import React from 'react';
import "./styles.css"
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

export  function Interface() {
  const [galaxySize, setGalaxySize] = React.useState(100);
  const [galaxySeedMass, setGalaxySeedMass] = React.useState(3);
  const [galaxyGravityReach, setGalaxyGravityReach] = React.useState(10);
  let wasmModule: any = null;
  let galaxyFrontend: galaxy.Frontend = null;

  wasm.then((module) => {
    console.log("wasm module loaded: ", module)
    wasmModule = module;
  });

  const handleGalaxySizeChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setGalaxySize(parseInt(event.target.value));
  };

  const galaxySizeInput = (
    <div className="input-group mb-3">
      <span className="input-group-text" id="basic-addon1">Galaxy Size:</span>
      <input
        type="text" className="form-control" placeholder="Username" name="galaxySize"
        value={galaxySize.toString()} onChange={handleGalaxySizeChange}
      />
    </div>
  )

  const handleGalaxySeedMassChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setGalaxySeedMass(parseInt(event.target.value));
  };

  const galaxySeedMassInput = (
    <div className="input-group mb-3">
      <span className="input-group-text" id="basic-addon1">Seed Mass:</span>
      <input
        type="text" className="form-control" placeholder="Username" name="galaxySeedMass"
        value={galaxySeedMass.toString()} onChange={handleGalaxySeedMassChange}
      />
    </div>
  )

  const handleGalaxyGravityReachChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setGalaxySeedMass(parseInt(event.target.value));
  };

  const galaxyGravityReachInput = (
    <div className="input-group mb-3">
      <span className="input-group-text" id="basic-addon1">Gravity Reach:</span>
      <input
        type="text" className="form-control" placeholder="Username" name="galaxyGravityReach"
        value={galaxyGravityReach.toString()} onChange={handleGalaxyGravityReachChange}
      />
    </div>
  )

  const handleInitClick = () => {
    if (wasmModule === null) {
      console.error("wasm not yet loaded");
    } else {
      console.log("initializing galaxy");
      galaxyFrontend = new galaxy.Frontend(galaxySize);
    }
  };

  const initButton = (
    <button type="button" className="btn btn-primary" onClick={handleInitClick}>
      init new galaxy
    </button>
  )

  const handleSeedClick = () => {
    if (galaxyFrontend === null) {
      console.error("galaxy not yet initialized");
    } else {
      console.log("seeding galaxy");
      galaxyFrontend.seed(galaxySeedMass);
    }
  };

  const seedButton = (
    <button type="button" className="btn btn-primary" onClick={handleSeedClick}>
      seed the galaxy
    </button>
  )

  const handleTickClick = () => {
    galaxyFrontend.tick(galaxyGravityReach);
  };

  const tickButton = (
    <button type="button" className="btn btn-primary" onClick={handleTickClick}>
      advance time
    </button>
  )

  return (
    <div className="container">
      <h1>Galaxy Generator</h1>
      <h2><small className="text-muted">( rust =&gt; wasm =&gt; js ) galaxy generation simulation</small></h2>
      {galaxySizeInput}
      {galaxySeedMassInput}
      {galaxyGravityReachInput}
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

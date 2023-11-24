import React from 'react';
import "./styles.css"
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

export  function Interface() {
  const [galaxySize, setGalaxySize] = React.useState(100);
  const [wasmModule, setWasmModule] = React.useState(null);
  let galaxyFrontend: galaxy.Frontend = null;

  // Fetch and instantiate the Wasm module
  React.useEffect(() => {
    const initWasm = async () => {
      try {
        const wasmModule = await import("galaxy_gen_backend");
        const wasmGalaxy = new wasmModule.Galaxy(galaxySize, 0);
        setWasmModule(module);
      } catch (err) {
        console.error('Error loading Wasm module:', err);
      }
    };
    initWasm();
  }, []);

  const handleChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setGalaxySize(parseInt(event.target.value));
  };

  const galaxySizeInput = (
    <div className="input-group mb-3">
      <span className="input-group-text" id="basic-addon1">Galaxy Size:</span>
      <input
        type="text" className="form-control" placeholder="Username" name="galaxySize"
        value={galaxySize.toString()} onChange={handleChange}
      />
    </div>
  )

  const handleSeedClick = () => {
    if (wasmModule !== null) {
    } else {
      console.error("wasm not yet loaded");
    }
    // galaxyFrontend = new galaxy.Frontend(galaxySize);
    // galaxyFrontend.seed();
    galaxyFrontend = new galaxy.Frontend(wasmModule, galaxySize);
    galaxyFrontend.seed();
    // dataviz.DataViz(galaxyFrontend);
  };

  const seedButton = (
    <button type="button" className="btn btn-primary" onClick={handleSeedClick}>
      seed new galaxy
    </button>
  )

  const handleTickClick = () => {
    // galaxyFrontend.tick();
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
      <div className="d-flex justify-content-between">
        {seedButton}
        {tickButton}
      </div>
      <div>
        <div id="dataviz"></div>
      </div>
    </div>
  );
}

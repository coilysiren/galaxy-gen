import React from "react";
import "./styles.css";
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

export function Interface() {
  const [galaxySize, setGalaxySize] = React.useState(50);
  const [galaxySeedMass, setGalaxySeedMass] = React.useState(25);
  const [timeModifier, setTimeModifier] = React.useState(0.01);
  const [wasmReady, setWasmReady] = React.useState(false);
  const [initialized, setInitialized] = React.useState(false);
  const [tickCount, setTickCount] = React.useState(0);

  const wasmModuleRef = React.useRef<any>(null);
  const galaxyFrontendRef = React.useRef<galaxy.Frontend | null>(null);

  React.useEffect(() => {
    wasm.then((module) => {
      wasmModuleRef.current = module;
      setWasmReady(true);
      if (typeof window !== "undefined") {
        (window as any).__galaxyGen = (window as any).__galaxyGen || {};
        (window as any).__galaxyGen.wasmReady = true;
      }
    });
  }, []);

  const exposeForTests = () => {
    if (typeof window !== "undefined") {
      (window as any).__galaxyGen = (window as any).__galaxyGen || {};
      (window as any).__galaxyGen.frontend = galaxyFrontendRef.current;
      (window as any).__galaxyGen.tickCount = tickCount;
    }
  };

  const handleIntChange = (setter: (n: number) => void) => {
    return (event: React.ChangeEvent<HTMLInputElement>) => {
      const value = parseInt(event.target.value, 10);
      setter(Number.isNaN(value) ? 0 : value);
    };
  };

  const handleFloatChange = (setter: (n: number) => void) => {
    return (event: React.ChangeEvent<HTMLInputElement>) => {
      const value = parseFloat(event.target.value);
      setter(Number.isNaN(value) ? 0 : value);
    };
  };

  const handleInitClick = () => {
    if (!wasmModuleRef.current) {
      console.error("wasm not yet loaded");
      return;
    }
    galaxyFrontendRef.current = new galaxy.Frontend(galaxySize, timeModifier);
    dataviz.initViz(galaxyFrontendRef.current);
    setInitialized(true);
    setTickCount(0);
    exposeForTests();
  };

  const handleSeedClick = () => {
    if (!galaxyFrontendRef.current) {
      console.error("galaxy not yet initialized");
      return;
    }
    galaxyFrontendRef.current.seed(galaxySeedMass);
    dataviz.initData(galaxyFrontendRef.current);
    exposeForTests();
  };

  const handleTickClick = () => {
    if (!galaxyFrontendRef.current) {
      console.error("galaxy not yet initialized");
      return;
    }
    galaxyFrontendRef.current.tick(timeModifier);
    dataviz.initData(galaxyFrontendRef.current);
    setTickCount((n) => n + 1);
    exposeForTests();
  };

  return (
    <div data-testid="app" data-wasm-ready={wasmReady ? "true" : "false"} className="min-h-screen">
      <nav className="nav-strip py-3 px-6 text-[#eeeeee] uppercase text-xs font-bold flex items-center justify-between">
        <span>./galaxy-gen</span>
        <span className="text-[color:var(--color-plum-400)]">rust → wasm → js</span>
      </nav>

      <main className="max-w-6xl mx-auto px-4 sm:px-6 py-10">
        <header className="mb-8">
          <h1 className="text-4xl md:text-5xl tracking-[0.1em]">Galaxy Generator</h1>
          <p className="mt-3 text-[color:var(--color-plum-400)] tracking-[0.08em]">
            Gravitational sim computed in Rust, rendered with D3 in the browser.
          </p>
        </header>

        <section className="panel p-6 md:p-8 mb-8">
          <div className="grid gap-4 md:grid-cols-3">
            <label className="block">
              <span className="input-label block mb-1">Galaxy Size</span>
              <input
                type="text"
                className="input-field"
                name="galaxySize"
                data-testid="input-galaxy-size"
                value={galaxySize.toString()}
                onChange={handleIntChange(setGalaxySize)}
              />
            </label>
            <label className="block">
              <span className="input-label block mb-1">Seed Mass</span>
              <input
                type="text"
                className="input-field"
                name="galaxySeedMass"
                data-testid="input-seed-mass"
                value={galaxySeedMass.toString()}
                onChange={handleIntChange(setGalaxySeedMass)}
              />
            </label>
            <label className="block">
              <span className="input-label block mb-1">Time Modifier</span>
              <input
                type="text"
                className="input-field"
                name="timeModifier"
                data-testid="input-time-modifier"
                value={timeModifier.toString()}
                onChange={handleFloatChange(setTimeModifier)}
              />
            </label>
          </div>

          <div className="mt-6 flex flex-wrap gap-3">
            <button
              type="button"
              className="btn-plum"
              data-testid="btn-init"
              onClick={handleInitClick}
              disabled={!wasmReady}
            >
              init new galaxy
            </button>
            <button
              type="button"
              className="btn-plum"
              data-testid="btn-seed"
              onClick={handleSeedClick}
              disabled={!initialized}
            >
              seed the galaxy
            </button>
            <button
              type="button"
              className="btn-plum"
              data-testid="btn-tick"
              onClick={handleTickClick}
              disabled={!initialized}
            >
              advance time
            </button>
            <div className="ml-auto input-label self-center">ticks: {tickCount}</div>
          </div>

          {!wasmReady && (
            <p className="mt-4 text-xs tracking-widest uppercase text-[color:var(--color-plum-400)]">
              loading wasm…
            </p>
          )}
        </section>

        <section className="panel p-4 md:p-6">
          <div id="dataviz" />
        </section>

        <footer className="mt-10 text-center text-xs tracking-[0.15em] uppercase text-[color:var(--color-plum-400)]">
          coilysiren · galaxy-gen
        </footer>
      </main>
    </div>
  );
}

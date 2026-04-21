import React from "react";
import "./styles.css";
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

const DEFAULT_DT = 0.5;
const DT_STEP = 1.25;
const DT_MIN = 0.01;
const DT_MAX = 10;

export function Interface() {
  const [galaxySize, setGalaxySize] = React.useState(50);
  const [galaxySeedMass, setGalaxySeedMass] = React.useState(25);
  const [timeModifier, setTimeModifier] = React.useState(DEFAULT_DT);
  const [wasmReady, setWasmReady] = React.useState(false);
  const [initialized, setInitialized] = React.useState(false);
  const [tickCount, setTickCount] = React.useState(0);
  const [running, setRunning] = React.useState(false);
  const [fps, setFps] = React.useState(0);
  const [tickMs, setTickMs] = React.useState(0);

  const wasmModuleRef = React.useRef<any>(null);
  const galaxyFrontendRef = React.useRef<galaxy.Frontend | null>(null);
  const runningRef = React.useRef(false);
  const rafRef = React.useRef<number | null>(null);
  const timeModRef = React.useRef(timeModifier);
  const fpsSamplesRef = React.useRef<number[]>([]);

  React.useEffect(() => {
    timeModRef.current = timeModifier;
  }, [timeModifier]);

  React.useEffect(() => {
    wasm.then((module) => {
      wasmModuleRef.current = module;
      setWasmReady(true);
      if (typeof window !== "undefined") {
        (window as any).__galaxyGen = (window as any).__galaxyGen || {};
        (window as any).__galaxyGen.wasmReady = true;
        (window as any).__galaxyGen.dataviz = dataviz;
      }
    });
    return () => {
      if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
    };
  }, []);

  const exposeForTests = () => {
    if (typeof window !== "undefined") {
      (window as any).__galaxyGen = (window as any).__galaxyGen || {};
      (window as any).__galaxyGen.frontend = galaxyFrontendRef.current;
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

  const stopLoop = React.useCallback(() => {
    runningRef.current = false;
    setRunning(false);
    if (rafRef.current != null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
  }, []);

  const handleInitClick = () => {
    const module = wasmModuleRef.current;
    if (!module) {
      console.error("wasm not yet loaded");
      return;
    }
    stopLoop();
    galaxyFrontendRef.current = new galaxy.Frontend(galaxySize);
    dataviz.initViz(galaxyFrontendRef.current);
    dataviz.initData(galaxyFrontendRef.current);
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
    const t0 = performance.now();
    galaxyFrontendRef.current.tick(timeModifier);
    const elapsed = performance.now() - t0;
    setTickMs(elapsed);
    dataviz.updateData(galaxyFrontendRef.current);
    setTickCount((n) => n + 1);
    exposeForTests();
  };

  const handleRunToggle = () => {
    if (!galaxyFrontendRef.current) return;
    if (runningRef.current) {
      stopLoop();
      return;
    }
    fpsSamplesRef.current.length = 0;
    runningRef.current = true;
    setRunning(true);

    const tick = () => {
      if (!runningRef.current || !galaxyFrontendRef.current) return;

      const t0 = performance.now();
      galaxyFrontendRef.current.tick(timeModRef.current);
      const tickElapsed = performance.now() - t0;

      dataviz.updateData(galaxyFrontendRef.current);

      fpsSamplesRef.current.push(performance.now());
      const cutoff = performance.now() - 1000;
      while (
        fpsSamplesRef.current.length > 0 &&
        fpsSamplesRef.current[0] < cutoff
      ) {
        fpsSamplesRef.current.shift();
      }
      setFps(fpsSamplesRef.current.length);
      setTickMs(tickElapsed);
      setTickCount((n) => n + 1);

      rafRef.current = requestAnimationFrame(tick);
    };

    rafRef.current = requestAnimationFrame(tick);
  };

  const clampDt = (value: number) => Math.min(DT_MAX, Math.max(DT_MIN, value));

  const adjustDt = React.useCallback((factor: number) => {
    setTimeModifier((prev) => {
      const next = clampDt(prev * factor);
      // Round to 3 decimals so the display stays tidy.
      return Math.round(next * 1000) / 1000;
    });
  }, []);

  const resetDt = React.useCallback(() => {
    setTimeModifier(DEFAULT_DT);
  }, []);

  const handleRunToggleRef = React.useRef(handleRunToggle);
  React.useEffect(() => {
    handleRunToggleRef.current = handleRunToggle;
  });

  React.useEffect(() => {
    const isEditable = (el: EventTarget | null): boolean => {
      if (!(el instanceof HTMLElement)) return false;
      const tag = el.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") {
        return true;
      }
      if (el.isContentEditable) return true;
      return false;
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (isEditable(e.target)) return;

      switch (e.key) {
        case " ":
        case "Spacebar":
          if (galaxyFrontendRef.current) {
            e.preventDefault();
            handleRunToggleRef.current();
          }
          break;
        case "ArrowUp":
          e.preventDefault();
          adjustDt(DT_STEP);
          break;
        case "ArrowDown":
          e.preventDefault();
          adjustDt(1 / DT_STEP);
          break;
        case "r":
        case "R":
          e.preventDefault();
          resetDt();
          break;
        default:
          break;
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [adjustDt, resetDt]);

  return (
    <div data-testid="app" data-wasm-ready={wasmReady ? "true" : "false"} className="min-h-screen">
      <nav className="nav-strip flex items-center justify-between px-6 py-3 text-xs font-bold uppercase text-[#eeeeee]">
        <span>./galaxy-gen</span>
        <span className="text-[color:var(--color-plum-400)]">rust → wasm → js</span>
      </nav>

      <main className="mx-auto max-w-6xl px-4 py-10 sm:px-6">
        <header className="mb-8">
          <h1 className="text-4xl tracking-[0.1em] md:text-5xl">Galaxy Generator</h1>
          <p className="mt-3 tracking-[0.08em] text-[color:var(--color-plum-400)]">
            Gravitational sim computed in Rust, rendered with D3 in the browser.
          </p>
        </header>

        <section className="panel mb-8 p-6 md:p-8">
          <div className="grid gap-4 md:grid-cols-3">
            <label className="block">
              <span className="input-label mb-1 block">Galaxy Size</span>
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
              <span className="input-label mb-1 block">Seed Mass</span>
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
              <span className="input-label mb-1 block">Time Modifier</span>
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

          <div className="mt-6 flex flex-wrap items-center gap-3">
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
              disabled={!initialized || running}
            >
              advance time
            </button>
            <button
              type="button"
              className="btn-plum"
              data-testid="btn-run"
              onClick={handleRunToggle}
              disabled={!initialized}
              style={
                running
                  ? {
                      background: "var(--color-plum-900)",
                      borderColor: "var(--color-plum-400)",
                    }
                  : undefined
              }
            >
              {running ? "pause" : "run"}
            </button>
            <div className="input-label ml-auto flex items-center gap-4 self-center">
              <span data-testid="stat-dt">dt: {timeModifier.toFixed(3)}</span>
              <span data-testid="stat-ticks">ticks: {tickCount}</span>
              <span>tick: {tickMs.toFixed(1)} ms</span>
              <span>fps: {fps}</span>
            </div>
          </div>

          <p
            className="mt-4 text-[0.7rem] tracking-widest uppercase text-[color:var(--color-plum-400)]"
            data-testid="keyboard-hints"
          >
            keys: <kbd>space</kbd> play/pause · <kbd>↑</kbd>/<kbd>↓</kbd> dt ×{DT_STEP}/÷{DT_STEP} ·{" "}
            <kbd>r</kbd> reset dt
          </p>

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

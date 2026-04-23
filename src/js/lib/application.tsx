import React from "react";
import "./styles.css";
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

const DEFAULT_DT = 0.5;
const DT_STEP = 1.25;
const DT_MIN = 0.01;
const DT_MAX = 10;

const DEFAULT_GALAXY_SIZE = 50;
const DEFAULT_SEED_MASS = 25;

// A u64 max is 2^64 - 1. We draw fresh randomize-button seeds uniformly
// over the full u64 range so they look properly opaque.
const U64_MAX = (1n << 64n) - 1n;

/**
 * Generate a fresh random u64 seed as a bigint. Uses `crypto.getRandomValues`
 * so the seeds aren't correlated with `Math.random()`'s shared state.
 */
function randomU64Seed(): bigint {
  if (typeof globalThis.crypto !== "undefined") {
    const buf = new Uint32Array(2);
    globalThis.crypto.getRandomValues(buf);
    return (BigInt(buf[0]) << 32n) | BigInt(buf[1]);
  }
  // Fallback for non-secure contexts.
  const hi = BigInt(Math.floor(Math.random() * 0x1_0000_0000));
  const lo = BigInt(Math.floor(Math.random() * 0x1_0000_0000));
  return (hi << 32n) | lo;
}

/** Parse a string as a u64 seed. Accepts decimal only. */
function parseSeed(s: string): bigint | null {
  if (!/^[0-9]+$/.test(s.trim())) return null;
  try {
    const n = BigInt(s.trim());
    if (n < 0n || n > U64_MAX) return null;
    return n;
  } catch {
    return null;
  }
}

interface InitialParams {
  galaxySize: number;
  seedMass: number;
  timeModifier: number;
  seed: string;
}

function readInitialParams(): InitialParams {
  const defaults: InitialParams = {
    galaxySize: DEFAULT_GALAXY_SIZE,
    seedMass: DEFAULT_SEED_MASS,
    timeModifier: DEFAULT_DT,
    seed: "",
  };
  if (typeof window === "undefined") return defaults;
  const params = new URLSearchParams(window.location.search);
  const sizeRaw = params.get("size");
  const massRaw = params.get("mass");
  const dtRaw = params.get("dt");
  const seedRaw = params.get("seed");
  const sizeN = sizeRaw != null ? parseInt(sizeRaw, 10) : NaN;
  const massN = massRaw != null ? parseInt(massRaw, 10) : NaN;
  const dtN = dtRaw != null ? parseFloat(dtRaw) : NaN;
  return {
    galaxySize: Number.isFinite(sizeN) && sizeN > 0 ? sizeN : defaults.galaxySize,
    seedMass: Number.isFinite(massN) && massN >= 0 ? massN : defaults.seedMass,
    timeModifier: Number.isFinite(dtN) && dtN > 0 ? dtN : defaults.timeModifier,
    seed: seedRaw != null && parseSeed(seedRaw) != null ? seedRaw.trim() : "",
  };
}

/**
 * Push the current init parameters into the URL via `history.replaceState`
 * so the page is shareable. We use `replaceState` (not `pushState`) so
 * re-initializing doesn't pile up history entries.
 */
function writeUrlParams(p: {
  galaxySize: number;
  seedMass: number;
  timeModifier: number;
  seed: string;
}): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams();
  params.set("seed", p.seed);
  params.set("size", p.galaxySize.toString());
  params.set("mass", p.seedMass.toString());
  params.set("dt", p.timeModifier.toString());
  const next = `${window.location.pathname}?${params.toString()}${window.location.hash}`;
  window.history.replaceState(null, "", next);
}

export function Interface() {
  const initial = React.useMemo(() => readInitialParams(), []);

  const [galaxySize, setGalaxySize] = React.useState(initial.galaxySize);
  const [galaxySeedMass, setGalaxySeedMass] = React.useState(initial.seedMass);
  const [timeModifier, setTimeModifier] = React.useState(initial.timeModifier);
  // Seed is stored as a string so the input control is forgiving. The
  // parse happens at Init / Seed time. Empty string = "use a fresh
  // random seed next time we seed".
  const [seed, setSeed] = React.useState<string>(initial.seed);
  const [initialCondition, setInitialCondition] = React.useState<galaxy.InitialCondition>(
    galaxy.InitialCondition.Uniform
  );
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

  // Worker-driven tick loop. The worker owns its own Galaxy WASM
  // instance and posts back mass snapshots; the main thread uses them
  // to drive the renderer and update the visible Frontend cache.
  const workerRef = React.useRef<galaxy.TickWorker | null>(null);
  const latestSnapshotRef = React.useRef<{
    mass: Uint16Array;
    tickMs: number;
    tickId: number;
  } | null>(null);
  const renderedTickIdRef = React.useRef<number>(-1);

  React.useEffect(() => {
    wasm.then((module) => {
      wasmModuleRef.current = module;
      setWasmReady(true);
      if (typeof window !== "undefined") {
        (window as any).__galaxyGen = (window as any).__galaxyGen || {};
        (window as any).__galaxyGen.wasmReady = true;
        (window as any).__galaxyGen.dataviz = dataviz;
        // Parity tests need the raw wasm module + Frontend constructor
        // so they can spin up an independent galaxy on a different backend.
        (window as any).__galaxyGen.wasm = module;
        (window as any).__galaxyGen.Frontend = galaxy.Frontend;
      }
    });
    return () => {
      if (rafRef.current != null) cancelAnimationFrame(rafRef.current);
      if (workerRef.current) {
        workerRef.current.terminate();
        workerRef.current = null;
      }
    };
  }, []);

  const exposeForTests = () => {
    if (typeof window !== "undefined") {
      (window as any).__galaxyGen = (window as any).__galaxyGen || {};
      (window as any).__galaxyGen.frontend = galaxyFrontendRef.current;
      (window as any).__galaxyGen.worker = workerRef.current;
      (window as any).__galaxyGen.workerSupported =
        typeof Worker !== "undefined";
    }
  };

  const handleIntChange = (setter: (n: number) => void) => {
    return (event: React.ChangeEvent<HTMLInputElement>) => {
      const value = parseInt(event.target.value, 10);
      setter(Number.isNaN(value) ? 0 : value);
    };
  };

  // Stops the run loop. If a worker is driving it, waits for the worker
  // to return final state and rehydrates the main-thread Frontend so
  // subsequent step/seed operations pick up exactly where it left off.
  const stopLoop = React.useCallback(async () => {
    if (!runningRef.current) return;
    runningRef.current = false;
    setRunning(false);
    if (rafRef.current != null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    const worker = workerRef.current;
    if (worker) {
      const state = await worker.stop();
      if (galaxyFrontendRef.current && state) {
        galaxyFrontendRef.current.restoreState(
          state.mass,
          state.velX,
          state.velY,
          state.fracX,
          state.fracY,
        );
        dataviz.updateData(galaxyFrontendRef.current);
      }
    }
  }, []);

  const handleInitClick = () => {
    const module = wasmModuleRef.current;
    if (!module) {
      console.error("wasm not yet loaded");
      return;
    }
    // Tear down any in-flight worker synchronously — init should be
    // immediate and we don't need the worker's final state.
    runningRef.current = false;
    setRunning(false);
    if (rafRef.current != null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
    if (workerRef.current) {
      workerRef.current.terminate();
      workerRef.current = null;
    }
    latestSnapshotRef.current = null;
    renderedTickIdRef.current = -1;
    // If the seed field is empty or invalid, fill one in so the URL
    // always has a shareable value after Init.
    let effectiveSeed = seed;
    if (parseSeed(effectiveSeed) == null) {
      effectiveSeed = randomU64Seed().toString();
      setSeed(effectiveSeed);
    }
    galaxyFrontendRef.current = new galaxy.Frontend(galaxySize);
    dataviz.initViz(galaxyFrontendRef.current);
    dataviz.initData(galaxyFrontendRef.current);
    setInitialized(true);
    setTickCount(0);
    writeUrlParams({
      galaxySize,
      seedMass: galaxySeedMass,
      timeModifier,
      seed: effectiveSeed,
    });
    exposeForTests();
  };

  const handleSeedClick = () => {
    if (!galaxyFrontendRef.current) {
      console.error("galaxy not yet initialized");
      return;
    }
    const parsed = parseSeed(seed);
    if (parsed != null) {
      galaxyFrontendRef.current.seedWith(galaxySeedMass, parsed);
    } else {
      galaxyFrontendRef.current.seed(galaxySeedMass, initialCondition);
    }
    dataviz.initData(galaxyFrontendRef.current);
    writeUrlParams({
      galaxySize,
      seedMass: galaxySeedMass,
      timeModifier,
      seed,
    });
    exposeForTests();
  };

  const handleTickClick = async () => {
    if (!galaxyFrontendRef.current) {
      console.error("galaxy not yet initialized");
      return;
    }
    const t0 = performance.now();
    // Route through tickAsync so the single-step button exercises the
    // selected backend (WebGPU force calc lands via tick_with_accel).
    await galaxyFrontendRef.current.tickAsync(timeModifier);
    const elapsed = performance.now() - t0;
    setTickMs(elapsed);
    dataviz.updateData(galaxyFrontendRef.current);
    setTickCount((n) => n + 1);
    exposeForTests();
  };

  const handleResetView = () => {
    dataviz.resetView();
  };

  // Render loop — driven by requestAnimationFrame on the main thread,
  // but *physics is in the worker*. This loop just paints whatever the
  // latest worker snapshot is. If no new snapshot arrived since last
  // frame we skip redrawing — cheap and avoids flicker.
  const renderLoop = React.useCallback(() => {
    if (!runningRef.current || !galaxyFrontendRef.current) return;
    const snap = latestSnapshotRef.current;
    if (snap && snap.tickId !== renderedTickIdRef.current) {
      renderedTickIdRef.current = snap.tickId;
      galaxyFrontendRef.current.setOverrideMass(snap.mass);
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
      setTickMs(snap.tickMs);
      setTickCount(snap.tickId);
    }
    rafRef.current = requestAnimationFrame(renderLoop);
  }, []);

  const handleRunToggle = async () => {
    if (!galaxyFrontendRef.current) return;
    if (runningRef.current) {
      await stopLoop();
      return;
    }
    fpsSamplesRef.current = [];
    latestSnapshotRef.current = null;
    renderedTickIdRef.current = -1;

    // Spin up (or reuse) the worker and hand it the current sim state.
    if (!workerRef.current) {
      if (typeof Worker === "undefined") {
        console.error(
          "Web Worker unsupported in this browser; physics run loop unavailable.",
        );
        return;
      }
      workerRef.current = new galaxy.TickWorker(
        (mass, tickMs, tickId) => {
          latestSnapshotRef.current = { mass, tickMs, tickId };
        },
      );
    }
    // snapshotState() reads mass/vel/frac out of WASM as fresh typed
    // arrays; those buffers are transferred to the worker (zero copy).
    const snapshot = galaxyFrontendRef.current.snapshotState();
    workerRef.current.init(snapshot);
    workerRef.current.start(timeModRef.current);
    exposeForTests();

    runningRef.current = true;
    setRunning(true);
    rafRef.current = requestAnimationFrame(renderLoop);
  };

  // Keep the worker's dt in sync with the UI while running.
  React.useEffect(() => {
    if (workerRef.current && runningRef.current) {
      workerRef.current.setTimeModifier(timeModifier);
    }
  }, [timeModifier]);

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
          <div className="grid gap-4 md:grid-cols-2">
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
            <label className="block md:col-span-2">
              <span className="input-label mb-1 block">Initial Condition</span>
              <select
                className="input-field"
                name="initialCondition"
                data-testid="select-initial-condition"
                value={initialCondition}
                onChange={(event) =>
                  setInitialCondition(parseInt(event.target.value, 10) as galaxy.InitialCondition)
                }
              >
                <option value={galaxy.InitialCondition.Uniform}>
                  uniform (random mass, no velocity)
                </option>
                <option value={galaxy.InitialCondition.Rotation}>
                  rotation (disk with angular velocity)
                </option>
                <option value={galaxy.InitialCondition.Bang}>bang (central explosion)</option>
                <option value={galaxy.InitialCondition.Collision}>
                  collision (two clusters on intercept)
                </option>
              </select>
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
            <button
              type="button"
              className="btn-plum"
              data-testid="btn-reset-view"
              onClick={handleResetView}
              disabled={!initialized}
              title="Reset pan/zoom of the viewport"
            >
              reset view
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

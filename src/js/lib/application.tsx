import React from "react";
import "./styles.css";
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

const DEFAULT_DT = 0.5;
const DT_STEP = 1.25;
const DT_MIN = 0.01;
const DT_MAX = 10;

const DEFAULT_GALAXY_SIZE = 250;
const DEFAULT_SEED_MASS = 25;

const U64_MAX = (1n << 64n) - 1n;

/** Fresh random u64 seed via `crypto.getRandomValues`. */
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

/** Push init params to URL via replaceState (avoids history pileup). */
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
  // Seed mass has no UI input; it flows from the `?mass=` URL param only.
  const [galaxySeedMass] = React.useState(initial.seedMass);
  const [timeModifier, setTimeModifier] = React.useState(initial.timeModifier);
  // Seed stays a string; parse at Init/Seed time. Empty means fresh random.
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
  const [starCount, setStarCount] = React.useState(0);
  const [snCount, setSnCount] = React.useState(0);

  const wasmModuleRef = React.useRef<any>(null);
  const galaxyFrontendRef = React.useRef<galaxy.Frontend | null>(null);
  const runningRef = React.useRef(false);
  const rafRef = React.useRef<number | null>(null);
  const timeModRef = React.useRef(timeModifier);
  const fpsSamplesRef = React.useRef<number[]>([]);

  React.useEffect(() => {
    timeModRef.current = timeModifier;
  }, [timeModifier]);

  // Worker owns its Galaxy and posts mass snapshots back for the renderer.
  const workerRef = React.useRef<galaxy.TickWorker | null>(null);
  const latestSnapshotRef = React.useRef<{
    mass: Uint16Array;
    tickMs: number;
    tickId: number;
    stars: Float32Array;
    transients: Float32Array;
    snCount: number;
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
        // Parity tests use these to spin up a galaxy on another backend.
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

  // Stops the run loop, awaiting worker state to rehydrate the Frontend.
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
          state.stars,
          state.field,
          state.meta,
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
    // Init is immediate; tear down worker without awaiting final state.
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
    // Always have a shareable seed on the URL after init.
    let effectiveSeed = seed;
    if (parseSeed(effectiveSeed) == null) {
      effectiveSeed = randomU64Seed().toString();
      setSeed(effectiveSeed);
    }
    const parsed = parseSeed(effectiveSeed);
    const next = new galaxy.Frontend(galaxySize);
    // Reproducible path covers every initial condition.
    if (parsed != null) {
      next.seedWith(galaxySeedMass, parsed, initialCondition);
    } else {
      next.seed(galaxySeedMass, initialCondition);
    }
    galaxyFrontendRef.current = next;
    dataviz.initViz(next);
    dataviz.initData(next);
    setInitialized(true);
    setTickCount(0);
    setStarCount(0);
    setSnCount(0);
    writeUrlParams({
      galaxySize,
      seedMass: galaxySeedMass,
      timeModifier,
      seed: effectiveSeed,
    });
    exposeForTests();
  };

  const handleTickClick = async () => {
    if (!galaxyFrontendRef.current) {
      console.error("galaxy not yet initialized");
      return;
    }
    const t0 = performance.now();
    // Single-step routes via tickAsync so WebGPU path is exercised.
    await galaxyFrontendRef.current.tickAsync(timeModifier);
    const elapsed = performance.now() - t0;
    setTickMs(elapsed);
    setTickCount((n) => {
      const next = n + 1;
      dataviz.updateData(galaxyFrontendRef.current!, next);
      return next;
    });
    setStarCount(galaxyFrontendRef.current.starCount());
    setSnCount(galaxyFrontendRef.current.supernovaCount());
    exposeForTests();
  };

  // RAF render loop; physics is in the worker. Skip redraw if no new snapshot.
  const renderLoop = React.useCallback(function loop() {
    if (!runningRef.current || !galaxyFrontendRef.current) return;
    const snap = latestSnapshotRef.current;
    if (snap && snap.tickId !== renderedTickIdRef.current) {
      renderedTickIdRef.current = snap.tickId;
      galaxyFrontendRef.current.setOverrideMass(snap.mass);
      galaxyFrontendRef.current.setOverrideStars(snap.stars);
      galaxyFrontendRef.current.setOverrideTransients(snap.transients);
      dataviz.updateData(galaxyFrontendRef.current, snap.tickId);
      setStarCount(snap.stars.length / 4);
      setSnCount(snap.snCount);

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
    rafRef.current = requestAnimationFrame(loop);
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
        (mass, tickMs, tickId, stars, transients, snCount) => {
          latestSnapshotRef.current = {
            mass,
            tickMs,
            tickId,
            stars,
            transients,
            snCount,
          };
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
      <main className="mx-auto max-w-none px-3 py-4 lg:grid lg:grid-cols-[280px_minmax(0,1fr)] lg:items-start lg:gap-4">
        <aside className="mb-4 lg:sticky lg:top-4 lg:mb-0">
          <section className="panel p-5">
            <header className="mb-5">
              <h1 className="text-2xl tracking-[0.1em]">Galaxy Generator</h1>
              <p className="mt-2 text-xs tracking-[0.08em] text-[color:var(--color-plum-400)]">
                Gravitational sim computed in Rust, rendered in the browser.
              </p>
            </header>
            <div className="grid gap-4">
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
                    uniform (rotating disk)
                  </option>
                  <option value={galaxy.InitialCondition.Bang}>bang (central explosion)</option>
                </select>
              </label>
            </div>

            <div className="mt-5 flex flex-wrap gap-3">
              <button
                type="button"
                className="btn-plum"
                data-testid="btn-init"
                onClick={handleInitClick}
                disabled={!wasmReady}
              >
                generate galaxy
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
                data-testid="btn-tick"
                onClick={handleTickClick}
                disabled={!initialized || running}
              >
                advance time
              </button>
            </div>

            <div className="input-label mt-5 grid grid-cols-2 gap-x-4 gap-y-1">
              <span data-testid="stat-dt">dt: {timeModifier.toFixed(3)}</span>
              <span data-testid="stat-ticks">ticks: {tickCount}</span>
              <span>tick: {tickMs.toFixed(1)} ms</span>
              <span>fps: {fps}</span>
              <span data-testid="stat-stars">stars: {starCount}</span>
              <span data-testid="stat-sn">sn: {snCount}</span>
            </div>

            <p
              className="mt-5 text-[0.7rem] leading-relaxed tracking-widest uppercase text-[color:var(--color-plum-400)]"
              data-testid="keyboard-hints"
            >
              keys: <kbd>space</kbd> play/pause · <kbd>↑</kbd>/<kbd>↓</kbd> dt ×{DT_STEP}/÷
              {DT_STEP} · <kbd>r</kbd> reset dt
              <br />
              mouse: drag pan · wheel zoom · double-click reset view
            </p>

            {!wasmReady && (
              <p className="mt-4 text-xs tracking-widest uppercase text-[color:var(--color-plum-400)]">
                loading wasm…
              </p>
            )}
          </section>
        </aside>

        <section>
          <div id="dataviz" />
        </section>
      </main>
    </div>
  );
}

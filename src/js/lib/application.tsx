import React from "react";
import "./styles.css";
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

/// Fixed sim time-step per tick. Was user-tunable (?dt= plus arrow-key
/// scaling); retired as a config surface - the physics tuning assumes
/// this value anyway.
const DT = 0.5;

const DEFAULT_GALAXY_SIZE = 250;
/// Fixed seed-mass intensity. Was the ?mass= URL knob; retired.
const SEED_MASS = 25;

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
  seed: string;
  /// `?lock=1`: generate reuses the seed instead of cycling to a fresh
  /// one on every press.
  seedLocked: boolean;
  /// `?debug=1`: dev surfaces - camera interaction plus perf stats.
  debug: boolean;
}

function readInitialParams(): InitialParams {
  const defaults: InitialParams = {
    galaxySize: DEFAULT_GALAXY_SIZE,
    seed: "",
    seedLocked: false,
    debug: false,
  };
  if (typeof window === "undefined") return defaults;
  const params = new URLSearchParams(window.location.search);
  const sizeRaw = params.get("size");
  const seedRaw = params.get("seed");
  const sizeN = sizeRaw != null ? parseInt(sizeRaw, 10) : NaN;
  const lockRaw = params.get("lock");
  return {
    galaxySize: Number.isFinite(sizeN) && sizeN > 0 ? sizeN : defaults.galaxySize,
    seed: seedRaw != null && parseSeed(seedRaw) != null ? seedRaw.trim() : "",
    seedLocked: lockRaw != null && lockRaw !== "0" && lockRaw !== "false",
    debug: params.has("debug"),
  };
}

/** Push init params to URL via replaceState (avoids history pileup). */
function writeUrlParams(p: {
  galaxySize: number;
  seed: string;
  seedLocked: boolean;
}): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams();
  params.set("seed", p.seed);
  params.set("size", p.galaxySize.toString());
  if (p.seedLocked) params.set("lock", "1");
  const next = `${window.location.pathname}?${params.toString()}${window.location.hash}`;
  window.history.replaceState(null, "", next);
}

export function Interface() {
  const initial = React.useMemo(() => readInitialParams(), []);

  const [galaxySize, setGalaxySize] = React.useState(initial.galaxySize);
  // Seed stays a string; parse at Init/Seed time. Empty means fresh random.
  const [seed, setSeed] = React.useState<string>(initial.seed);
  // Generate cycles the seed unless ?lock=1 pins it. A URL-provided seed
  // is honored for the FIRST generate either way, so shared links
  // reproduce.
  const seedLocked = initial.seedLocked;
  const debug = initial.debug;
  const hasGeneratedRef = React.useRef(false);
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
  const [birthCount, setBirthCount] = React.useState(0);
  const [captureCount, setCaptureCount] = React.useState(0);
  const [bhFactor, setBhFactor] = React.useState(1);
  const [gasPct, setGasPct] = React.useState(100);
  // Seed-time baselines for the popsci ratios.
  const initialGasRef = React.useRef(1);
  const initialBhRef = React.useRef(1);

  const wasmModuleRef = React.useRef<any>(null);
  const galaxyFrontendRef = React.useRef<galaxy.Frontend | null>(null);
  const runningRef = React.useRef(false);
  const rafRef = React.useRef<number | null>(null);
  const fpsSamplesRef = React.useRef<number[]>([]);

  // Worker owns its Galaxy and posts mass snapshots back for the renderer.
  const workerRef = React.useRef<galaxy.TickWorker | null>(null);
  const latestSnapshotRef = React.useRef<{
    mass: Uint16Array;
    tickMs: number;
    tickId: number;
    stars: Float32Array;
    transients: Float32Array;
    radiation: Float32Array;
    snCount: number;
    birthCount: number;
    captureCount: number;
    bhMass: number;
    gasTotal: number;
    lensScale: number;
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
    // Always have a shareable seed on the URL after init. Reuse the
    // current seed only when locked or on the first generate of a
    // seed-bearing URL; otherwise every press rolls a fresh galaxy.
    let effectiveSeed = seed;
    const reuse =
      parseSeed(effectiveSeed) != null && (seedLocked || !hasGeneratedRef.current);
    if (!reuse) {
      effectiveSeed = randomU64Seed().toString();
      setSeed(effectiveSeed);
    }
    hasGeneratedRef.current = true;
    const parsed = parseSeed(effectiveSeed);
    const next = new galaxy.Frontend(galaxySize);
    // Reproducible path covers every initial condition.
    if (parsed != null) {
      next.seedWith(SEED_MASS, parsed, initialCondition);
    } else {
      next.seed(SEED_MASS, initialCondition);
    }
    galaxyFrontendRef.current = next;
    dataviz.initViz(next);
    dataviz.initData(next);
    setInitialized(true);
    setTickCount(0);
    setStarCount(0);
    setSnCount(0);
    setBirthCount(0);
    setCaptureCount(0);
    setBhFactor(1);
    setGasPct(100);
    initialGasRef.current = Math.max(1, next.gasTotal());
    initialBhRef.current = Math.max(1, next.bhMass());
    writeUrlParams({
      galaxySize,
      seed: effectiveSeed,
      seedLocked,
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
    await galaxyFrontendRef.current.tickAsync(DT);
    const elapsed = performance.now() - t0;
    setTickMs(elapsed);
    setTickCount((n) => {
      const next = n + 1;
      dataviz.updateData(galaxyFrontendRef.current!, next);
      return next;
    });
    const fe = galaxyFrontendRef.current;
    setStarCount(fe.starCount());
    setSnCount(fe.supernovaCount());
    setBirthCount(fe.birthCount());
    setCaptureCount(fe.captureCount());
    setBhFactor(fe.bhMass() / initialBhRef.current);
    setGasPct((100 * fe.gasTotal()) / initialGasRef.current);
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
      galaxyFrontendRef.current.setOverrideRadiation(snap.radiation);
      galaxyFrontendRef.current.setOverrideLensScale(snap.lensScale);
      dataviz.updateData(galaxyFrontendRef.current, snap.tickId);
      setStarCount(snap.stars.length / 4);
      setSnCount(snap.snCount);
      setBirthCount(snap.birthCount);
      setCaptureCount(snap.captureCount);
      setBhFactor(snap.bhMass / initialBhRef.current);
      setGasPct((100 * snap.gasTotal) / initialGasRef.current);

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
        (
          mass,
          tickMs,
          tickId,
          stars,
          transients,
          radiation,
          snCount,
          birthCount,
          captureCount,
          bhMass,
          gasTotal,
          lensScale,
        ) => {
          latestSnapshotRef.current = {
            mass,
            tickMs,
            tickId,
            stars,
            transients,
            radiation,
            snCount,
            birthCount,
            captureCount,
            bhMass,
            gasTotal,
            lensScale,
          };
        },
      );
    }
    // snapshotState() reads mass/vel/frac out of WASM as fresh typed
    // arrays; those buffers are transferred to the worker (zero copy).
    const snapshot = galaxyFrontendRef.current.snapshotState();
    workerRef.current.init(snapshot);
    workerRef.current.start(DT);
    exposeForTests();

    runningRef.current = true;
    setRunning(true);
    rafRef.current = requestAnimationFrame(renderLoop);
  };

  return (
    <div data-testid="app" data-wasm-ready={wasmReady ? "true" : "false"} className="min-h-screen">
      <main>
        <aside className="fixed left-3 top-3 z-10 w-72 max-w-[calc(100vw-1.5rem)] max-h-[calc(100vh-1.5rem)] overflow-y-auto">
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

            <div className="mt-5 space-y-2">
              <button
                type="button"
                className="btn-plum w-full"
                data-testid="btn-init"
                onClick={handleInitClick}
                disabled={!wasmReady}
              >
                generate
              </button>
              <button
                type="button"
                className="btn-plum w-full"
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
                {running ? "pause" : "play"}
              </button>
              <button
                type="button"
                className="btn-plum w-full"
                data-testid="btn-tick"
                onClick={handleTickClick}
                disabled={!initialized || running}
              >
                step
              </button>
            </div>

            <div className="input-label mt-5 space-y-1">
              <div className="flex justify-between" data-testid="stat-ticks">
                <span>ticks:</span>
                <span> {tickCount}</span>
              </div>
              <div className="flex justify-between" data-testid="stat-stars">
                <span>stars</span>
                <span>{starCount.toLocaleString()}</span>
              </div>
              <div className="flex justify-between" data-testid="stat-sn">
                <span>supernovae</span>
                <span>{snCount.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span>star births</span>
                <span>{birthCount.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span>eaten by black hole</span>
                <span>{captureCount.toLocaleString()}</span>
              </div>
              <div className="flex justify-between">
                <span>black hole</span>
                <span>×{bhFactor.toFixed(2)}</span>
              </div>
              <div className="flex justify-between">
                <span>gas reservoir</span>
                <span>{gasPct.toFixed(0)}%</span>
              </div>
              {debug && (
                <>
                  <div className="flex justify-between">
                    <span>tick</span>
                    <span>{tickMs.toFixed(1)} ms</span>
                  </div>
                  <div className="flex justify-between">
                    <span>fps</span>
                    <span>{fps}</span>
                  </div>
                </>
              )}
            </div>

            {!wasmReady && (
              <p className="mt-4 text-xs tracking-widest uppercase text-[color:var(--color-plum-400)]">
                loading wasm…
              </p>
            )}
          </section>
        </aside>

        <section className="fixed inset-0 z-0">
          <div id="dataviz" className="h-full w-full" />
        </section>
      </main>
    </div>
  );
}

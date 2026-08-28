import React from "react";
import "./styles.css";
import * as dataviz from "./dataviz";
import * as galaxy from "./galaxy";
import * as recorder from "./recorder";

const wasm = import("galaxy_gen_backend/galaxy_gen_backend");

/// Fixed sim time-step. Retired as a config surface - the physics
/// tuning assumes this value. See docs/ui-controls.md.
const DT = 0.5;

/// 500x500 = 250k cells. Raised from 250 once gas composited per
/// screen-space block instead of per cell. See docs/journal/perf-rewrite.md.
const DEFAULT_GALAXY_SIZE = 500;
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

/** URL slug <-> Scenario. The slug names the full start => end pair. */
const SCENARIO_SLUGS: Array<[string, galaxy.Scenario]> = [
  ["bang-ring", galaxy.Scenario.BangRing],
  ["bang-spiral", galaxy.Scenario.BangSpiral],
  ["irregular-spiral", galaxy.Scenario.IrregularSpiral],
  ["irregular-elliptical", galaxy.Scenario.IrregularElliptical],
];

function scenarioFromSlug(slug: string | null): galaxy.Scenario | null {
  const hit = SCENARIO_SLUGS.find(([s]) => s === slug);
  return hit ? hit[1] : null;
}

function scenarioToSlug(sc: galaxy.Scenario): string {
  const hit = SCENARIO_SLUGS.find(([, v]) => v === sc);
  return hit ? hit[0] : "irregular-spiral";
}

interface InitialParams {
  galaxySize: number;
  seed: string;
  /// `?scenario=<slug>`: start => end pair; part of the permalink.
  scenario: galaxy.Scenario;
  /// `?lock=1`: generate reuses the seed instead of cycling to a fresh
  /// one on every press.
  seedLocked: boolean;
  /// `?debug=1`: dev surfaces - camera interaction, single-step, and the
  /// full lifecycle counter table.
  debug: boolean;
  /// `?ui=0`: start with the panel hidden, so a permalink or recording
  /// is addressable chrome-free. See docs/ui-controls.md.
  uiHidden: boolean;
  /// `?t=N`: with a seed, auto-generate and fast-forward to this tick -
  /// (seed, size, t) is a complete address for a moment in time.
  warpTicks: number;
}

function readInitialParams(): InitialParams {
  const defaults: InitialParams = {
    galaxySize: DEFAULT_GALAXY_SIZE,
    seed: "",
    scenario: galaxy.Scenario.IrregularSpiral,
    seedLocked: false,
    debug: false,
    uiHidden: false,
    warpTicks: 0,
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
    scenario: scenarioFromSlug(params.get("scenario")) ?? defaults.scenario,
    seedLocked: lockRaw != null && lockRaw !== "0" && lockRaw !== "false",
    debug: params.has("debug"),
    uiHidden: params.get("ui") === "0",
    warpTicks: (() => {
      const t = parseInt(params.get("t") ?? "", 10);
      return Number.isFinite(t) && t > 0 ? t : 0;
    })(),
  };
}

/** Patch only the `t` param on the current URL (pause / step / warp). */
function patchUrlTick(t: number): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams(window.location.search);
  params.set("t", String(Math.round(t)));
  window.history.replaceState(
    null,
    "",
    `${window.location.pathname}?${params.toString()}${window.location.hash}`
  );
}

/** Patch only the `ui` param, so a hidden-chrome view is shareable. */
function patchUrlUi(hidden: boolean): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams(window.location.search);
  if (hidden) {
    params.set("ui", "0");
  } else {
    params.delete("ui");
  }
  window.history.replaceState(
    null,
    "",
    `${window.location.pathname}?${params.toString()}${window.location.hash}`
  );
}

/** Push init params to URL via replaceState (avoids history pileup). */
function writeUrlParams(p: {
  galaxySize: number;
  seed: string;
  scenario: galaxy.Scenario;
  seedLocked: boolean;
  debug: boolean;
  uiHidden: boolean;
}): void {
  if (typeof window === "undefined") return;
  const params = new URLSearchParams();
  params.set("seed", p.seed);
  params.set("size", p.galaxySize.toString());
  params.set("scenario", scenarioToSlug(p.scenario));
  if (p.seedLocked) params.set("lock", "1");
  // Generate rebuilds the query from scratch, so view-state params have
  // to be re-applied or a generate silently drops them.
  if (p.debug) params.set("debug", "1");
  if (p.uiHidden) params.set("ui", "0");
  const next = `${window.location.pathname}?${params.toString()}${window.location.hash}`;
  window.history.replaceState(null, "", next);
}

export function Interface() {
  const initial = React.useMemo(() => readInitialParams(), []);

  const [galaxySize, setGalaxySize] = React.useState(initial.galaxySize);
  // Seed stays a string; parse at Init/Seed time. Empty means fresh random.
  const [seed, setSeed] = React.useState<string>(initial.seed);
  // Generate cycles the seed unless ?lock=1 pins it; a URL seed is
  // honored for the FIRST generate either way, so shared links reproduce.
  const seedLocked = initial.seedLocked;
  const debug = initial.debug;
  // Chrome visibility. Mirrored to `?ui=0` so a clean frame is a
  // shareable address and a recording can be captured without chrome.
  const [uiHidden, setUiHidden] = React.useState(initial.uiHidden);
  const hasGeneratedRef = React.useRef(false);
  const [scenario, setScenario] = React.useState<galaxy.Scenario>(initial.scenario);
  const [wasmReady, setWasmReady] = React.useState(false);
  const [initialized, setInitialized] = React.useState(false);
  const [tickCount, setTickCount] = React.useState(0);
  const [warping, setWarping] = React.useState(false);
  // ?t= permalink warp, consumed by the first generate.
  const pendingWarpRef = React.useRef(initial.warpTicks);
  const [running, setRunning] = React.useState(false);
  const [fps, setFps] = React.useState(0);
  const [tickMs, setTickMs] = React.useState(0);
  const [starCount, setStarCount] = React.useState(0);
  const [snCount, setSnCount] = React.useState(0);
  const [associationCount, setAssociationCount] = React.useState(0);
  const [captureCount, setCaptureCount] = React.useState(0);
  const [neutronStarCount, setNeutronStarCount] = React.useState(0);
  const [redGiantCount, setRedGiantCount] = React.useState(0);
  const [whiteDwarfCount, setWhiteDwarfCount] = React.useState(0);
  const [planetaryNebulaCount, setPlanetaryNebulaCount] = React.useState(0);
  const [typeIaCount, setTypeIaCount] = React.useState(0);
  const [grbCount, setGrbCount] = React.useState(0);
  const [phaseMixedCount, setPhaseMixedCount] = React.useState(0);
  const [bhFactor, setBhFactor] = React.useState(1);
  const [gasPct, setGasPct] = React.useState(100);
  const [quasarActivity, setQuasarActivity] = React.useState(0);
  const [recorderStatus, setRecorderStatus] = React.useState(recorder.getStatus());
  const [recordFormat, setRecordFormat] = React.useState<recorder.RecorderFormat>("gif");
  // Null until the WebCodecs probe answers. The mp4 pill stays disabled
  // rather than absent, so its unavailability is legible.
  const [mp4Available, setMp4Available] = React.useState<boolean | null>(null);
  React.useEffect(() => {
    let live = true;
    void recorder.isMp4Available().then((ok) => {
      if (live) setMp4Available(ok);
    });
    return () => {
      live = false;
    };
  }, []);
  // Seed-time baselines for the popsci ratios.
  const initialBhRef = React.useRef(1);

  // Feed every completed draw to the recorder. It ignores frames unless
  // a recording is active, so this stays wired for the page's lifetime.
  React.useEffect(() => {
    dataviz.setFrameListener((canvas, simTick) => recorder.capture(canvas, simTick));
    const unsubscribe = recorder.subscribe(setRecorderStatus);
    return () => {
      dataviz.setFrameListener(null);
      unsubscribe();
    };
  }, []);

  // Pointer activity wakes the chrome toggle. Without this a viewer who
  // hides the UI has no visible affordance to bring it back.
  React.useEffect(() => {
    if (typeof document === "undefined") return;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const wake = () => {
      document.body.classList.add("ui-toggle-wake");
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => document.body.classList.remove("ui-toggle-wake"), 2000);
    };
    window.addEventListener("pointermove", wake, { passive: true });
    return () => {
      window.removeEventListener("pointermove", wake);
      if (timer) clearTimeout(timer);
      document.body.classList.remove("ui-toggle-wake");
    };
  }, []);

  const wasmModuleRef = React.useRef<any>(null);
  const galaxyFrontendRef = React.useRef<galaxy.Frontend | null>(null);
  const runningRef = React.useRef(false);
  const rafRef = React.useRef<number | null>(null);
  const fpsSamplesRef = React.useRef<number[]>([]);

  // Worker owns its Galaxy and posts mass snapshots back for the renderer.
  const workerRef = React.useRef<galaxy.TickWorker | null>(null);
  const latestSnapshotRef = React.useRef<{
    mass: Uint16Array;
    fracX: Float32Array;
    fracY: Float32Array;
    tickMs: number;
    tickId: number;
    stars: Float32Array;
    transients: Float32Array;
    radiation: Float32Array;
    metallicity: Float32Array;
    snCount: number;
    associationCount: number;
    captureCount: number;
    neutronStarCount: number;
    redGiantCount: number;
    whiteDwarfCount: number;
    planetaryNebulaCount: number;
    typeIaCount: number;
    grbCount: number;
    phaseMixedCount: number;
    stellarHaloMass: number;
    bhMass: number;
    gasColdFraction: number;
    lensScale: number;
    quasarActivity: number;
    quasarPulse: number;
    quasarAge: number;
    quasarPulsePeriod: number;
    quasarAxis: number;
    quasarEpisodes: number;
  } | null>(null);
  const renderedTickIdRef = React.useRef<number>(-1);

  // Permalink auto-load: a URL with seed + t generates and warps on
  // arrival, no click needed.
  const autoLoadedRef = React.useRef(false);
  React.useEffect(() => {
    if (!wasmReady || autoLoadedRef.current) return;
    autoLoadedRef.current = true;
    if (initial.warpTicks > 0 && initial.seed) {
      handleInitClick();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wasmReady]);

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
      (window as any).__galaxyGen.workerSupported = typeof Worker !== "undefined";
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
          state.meta
        );
        const tick = galaxyFrontendRef.current.tickCount();
        setTickCount(tick);
        dataviz.updateData(galaxyFrontendRef.current, tick);
        patchUrlTick(tick);
      }
    }
  }, []);

  /// `forceReuse` is the reset path: same universe at tick zero. Generate
  /// passes false and keeps cycle-unless-locked.
  const regenerate = (forceReuse: boolean) => {
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
    // Always leave a shareable seed on the URL. Reuse it only when locked
    // or on the first generate of a seed-bearing URL.
    let effectiveSeed = seed;
    const reuse =
      parseSeed(effectiveSeed) != null && (forceReuse || seedLocked || !hasGeneratedRef.current);
    if (!reuse) {
      effectiveSeed = randomU64Seed().toString();
      setSeed(effectiveSeed);
    }
    hasGeneratedRef.current = true;
    const parsed = parseSeed(effectiveSeed);
    const next = new galaxy.Frontend(galaxySize);
    // Reproducible path covers every scenario.
    if (parsed != null) {
      next.seedWith(SEED_MASS, parsed, scenario);
    } else {
      next.seed(SEED_MASS, scenario);
    }
    galaxyFrontendRef.current = next;
    // Same seed as the physics, so the backdrop travels with the
    // permalink instead of rerolling per page load.
    dataviz.initViz(next, scenario, parsed);
    dataviz.initData(next);
    setInitialized(true);
    setTickCount(0);
    setStarCount(0);
    setSnCount(0);
    setAssociationCount(0);
    setCaptureCount(0);
    setNeutronStarCount(0);
    setRedGiantCount(0);
    setWhiteDwarfCount(0);
    setPlanetaryNebulaCount(0);
    setTypeIaCount(0);
    setGrbCount(0);
    setPhaseMixedCount(0);
    setBhFactor(1);
    setGasPct(100);
    setQuasarActivity(0);
    initialBhRef.current = Math.max(1, next.bhMass());
    const warp = pendingWarpRef.current;
    pendingWarpRef.current = 0;
    if (warp > 0) void warpTo(warp);
    writeUrlParams({
      galaxySize,
      seed: effectiveSeed,
      scenario,
      seedLocked,
      debug,
      uiHidden,
    });
    exposeForTests();
  };

  const handleInitClick = () => regenerate(false);

  /// Reset: same seed, same scenario, back to tick zero. Distinct from
  /// generate, which rolls a new universe unless the seed is locked.
  const handleResetClick = () => regenerate(true);

  const handleUiToggle = () => {
    setUiHidden((hidden) => {
      const next = !hidden;
      patchUrlUi(next);
      return next;
    });
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
    const fe = galaxyFrontendRef.current;
    const tick = fe.tickCount();
    setTickCount(tick);
    dataviz.updateData(fe, tick);
    refreshStats(fe);
    patchUrlTick(tick);
    exposeForTests();
  };

  const refreshStats = (fe: galaxy.Frontend) => {
    setStarCount(fe.starCount());
    setSnCount(fe.supernovaCount());
    setAssociationCount(fe.associationCount());
    setCaptureCount(fe.captureCount());
    setNeutronStarCount(fe.neutronStarCount());
    setRedGiantCount(fe.redGiantCount());
    setWhiteDwarfCount(fe.whiteDwarfCount());
    setPlanetaryNebulaCount(fe.planetaryNebulaCount());
    setTypeIaCount(fe.typeIaCount());
    setGrbCount(fe.grbCount());
    setPhaseMixedCount(fe.phaseMixedCount());
    setBhFactor(fe.bhMass() / initialBhRef.current);
    setGasPct(100 * fe.gasColdFraction());
    setQuasarActivity(fe.quasarActivity());
  };

  // Fast-forward to a target tick in chunks that yield to the event
  // loop, so a permalink warp keeps the tab responsive.
  const warpTo = async (target: number) => {
    const fe = galaxyFrontendRef.current;
    if (!fe) return;
    setWarping(true);
    const CHUNK = 40;
    while (fe.tickCount() < target) {
      const n = Math.min(CHUNK, target - fe.tickCount());
      for (let k = 0; k < n; k++) fe.tick(DT);
      setTickCount(fe.tickCount());
      await new Promise((r) => setTimeout(r, 0));
    }
    const tick = fe.tickCount();
    dataviz.updateData(fe, tick);
    setTickCount(tick);
    refreshStats(fe);
    patchUrlTick(tick);
    setWarping(false);
  };

  // RAF render loop; physics is in the worker. Skip redraw if no new snapshot.
  const renderLoop = React.useCallback(function loop() {
    if (!runningRef.current || !galaxyFrontendRef.current) return;
    const snap = latestSnapshotRef.current;
    if (snap && snap.tickId !== renderedTickIdRef.current) {
      renderedTickIdRef.current = snap.tickId;
      galaxyFrontendRef.current.setOverrideMass(snap.mass);
      galaxyFrontendRef.current.setOverrideGasOffsets(snap.fracX, snap.fracY);
      galaxyFrontendRef.current.setOverrideStars(snap.stars);
      galaxyFrontendRef.current.setOverrideTransients(snap.transients);
      galaxyFrontendRef.current.setOverrideRadiation(snap.radiation);
      galaxyFrontendRef.current.setOverrideMetallicity(snap.metallicity);
      galaxyFrontendRef.current.setOverrideLensScale(snap.lensScale);
      galaxyFrontendRef.current.setOverrideStellarHaloMass(snap.stellarHaloMass);
      galaxyFrontendRef.current.setOverrideQuasar(
        snap.quasarActivity,
        snap.quasarPulse,
        snap.quasarAge,
        snap.quasarPulsePeriod,
        snap.quasarAxis,
        snap.quasarEpisodes
      );
      dataviz.updateData(galaxyFrontendRef.current, snap.tickId);
      setStarCount(snap.stars.length / galaxy.STAR_RENDER_FLOATS);
      setSnCount(snap.snCount);
      setAssociationCount(snap.associationCount);
      setCaptureCount(snap.captureCount);
      setNeutronStarCount(snap.neutronStarCount);
      setRedGiantCount(snap.redGiantCount);
      setWhiteDwarfCount(snap.whiteDwarfCount);
      setPlanetaryNebulaCount(snap.planetaryNebulaCount);
      setTypeIaCount(snap.typeIaCount);
      setGrbCount(snap.grbCount);
      setPhaseMixedCount(snap.phaseMixedCount);
      setBhFactor(snap.bhMass / initialBhRef.current);
      setGasPct(100 * snap.gasColdFraction);
      setQuasarActivity(snap.quasarActivity);

      fpsSamplesRef.current.push(performance.now());
      const cutoff = performance.now() - 1000;
      while (fpsSamplesRef.current.length > 0 && fpsSamplesRef.current[0] < cutoff) {
        fpsSamplesRef.current.shift();
      }
      setFps(fpsSamplesRef.current.length);
      setTickMs(snap.tickMs);
      setTickCount(snap.tickId);
    }
    rafRef.current = requestAnimationFrame(loop);
  }, []);

  /// Start capture, or stop and hand the GIF to the browser. It samples
  /// the render funnel, so a capture spans whatever the run does next.
  const handleRecordToggle = async () => {
    if (recorderStatus.recording) {
      const blob = await recorder.stop();
      if (blob) recorder.download(blob, recorder.fileName());
      return;
    }
    if (!galaxyFrontendRef.current) return;
    // Name the file after the permalink that reproduces the run.
    recorder.start(`galaxy-${seed}-${scenarioToSlug(scenario)}-${galaxySize}`, {
      format: recordFormat,
    });
  };

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
        console.error("Web Worker unsupported in this browser; physics run loop unavailable.");
        return;
      }
      workerRef.current = new galaxy.TickWorker(
        (
          mass,
          fracX,
          fracY,
          tickMs,
          tickId,
          stars,
          transients,
          radiation,
          metallicity,
          snCount,
          associationCount,
          captureCount,
          neutronStarCount,
          redGiantCount,
          whiteDwarfCount,
          planetaryNebulaCount,
          typeIaCount,
          grbCount,
          phaseMixedCount,
          stellarHaloMass,
          bhMass,
          gasColdFraction,
          lensScale,
          quasarActivity,
          quasarPulse,
          quasarAge,
          quasarPulsePeriod,
          quasarAxis,
          quasarEpisodes
        ) => {
          latestSnapshotRef.current = {
            mass,
            fracX,
            fracY,
            tickMs,
            tickId,
            stars,
            transients,
            radiation,
            metallicity,
            snCount,
            associationCount,
            captureCount,
            neutronStarCount,
            redGiantCount,
            whiteDwarfCount,
            planetaryNebulaCount,
            typeIaCount,
            grbCount,
            phaseMixedCount,
            stellarHaloMass,
            bhMass,
            gasColdFraction,
            lensScale,
            quasarActivity,
            quasarPulse,
            quasarAge,
            quasarPulsePeriod,
            quasarAxis,
            quasarEpisodes,
          };
        }
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
      {/* Chrome toggle. Always mounted and above every layer, so the
          hidden state is always reversible without a reload. */}
      <button
        type="button"
        className="ui-toggle"
        data-testid="btn-ui-toggle"
        aria-pressed={uiHidden}
        aria-label={uiHidden ? "show controls" : "hide controls"}
        title={uiHidden ? "show controls" : "hide controls"}
        onClick={handleUiToggle}
      >
        {uiHidden ? "◴" : "◵"}
      </button>

      {/* Mobile stacks column-reverse: canvas on top, controls as a
          horizontal bar beneath it, so the two never overlap. From `sm`
          up the panel floats over a full-bleed canvas as before. */}
      <main className="fixed inset-0 flex flex-col-reverse sm:block">
        <aside
          className={`${
            uiHidden ? "hidden" : ""
          } z-10 shrink-0 max-h-[45vh] overflow-y-auto sm:fixed sm:left-3 sm:top-3 sm:w-72 sm:max-w-[calc(100vw-1.5rem)] sm:max-h-[calc(100vh-1.5rem)]`}
          data-testid="controls"
        >
          <section className="panel p-4 sm:p-5">
            <header className="mb-3 sm:mb-5">
              <h1 className="text-lg tracking-[0.1em] sm:text-2xl">Galaxy Generator</h1>
              <p className="mt-2 hidden text-xs tracking-[0.08em] text-[color:var(--color-plum-400)] sm:block">
                Gravitational sim computed in Rust, rendered in the browser.
              </p>
            </header>
            <div className="grid grid-cols-2 gap-3 sm:grid-cols-1 sm:gap-4">
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
                <span className="input-label mb-1 block">Scenario</span>
                <select
                  className="input-field"
                  name="scenario"
                  data-testid="select-scenario"
                  value={scenario}
                  onChange={(event) =>
                    setScenario(parseInt(event.target.value, 10) as galaxy.Scenario)
                  }
                >
                  <option value={galaxy.Scenario.BangRing}>bang → ring</option>
                  <option value={galaxy.Scenario.BangSpiral}>bang → spiral</option>
                  <option value={galaxy.Scenario.IrregularSpiral}>irregular → spiral</option>
                  <option value={galaxy.Scenario.IrregularElliptical}>
                    irregular → elliptical
                  </option>
                </select>
              </label>
            </div>

            <div className="mt-4 grid grid-cols-2 gap-2 sm:mt-5 sm:grid-cols-1">
              <button
                type="button"
                className="btn-plum w-full"
                data-testid="btn-init"
                onClick={handleInitClick}
                disabled={!wasmReady || warping}
              >
                {warping ? "warping…" : "generate"}
              </button>
              <button
                type="button"
                className="btn-plum w-full"
                data-testid="btn-run"
                onClick={handleRunToggle}
                disabled={!initialized || warping}
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
                data-testid="btn-reset"
                onClick={handleResetClick}
                disabled={!initialized || warping}
              >
                reset
              </button>
              {/* Single-step is a dev affordance, not a viewer control.
                  It lives with the camera and the counter table. */}
              {debug && (
                <button
                  type="button"
                  className="btn-plum w-full"
                  data-testid="btn-tick"
                  onClick={handleTickClick}
                  disabled={!initialized || running || warping}
                >
                  step
                </button>
              )}
              <button
                type="button"
                className="btn-plum w-full"
                data-testid="btn-record"
                onClick={handleRecordToggle}
                disabled={!initialized || warping || recorderStatus.encoding}
                style={
                  recorderStatus.recording
                    ? {
                        background: "var(--color-plum-900)",
                        borderColor: "var(--color-plum-400)",
                      }
                    : undefined
                }
              >
                {recorderStatus.encoding
                  ? "encoding…"
                  : recorderStatus.recording
                    ? `stop (${recorderStatus.frames}/${recorderStatus.maxFrames})`
                    : `record ${recordFormat}`}
              </button>
              {/* Two formats, so a segmented pair reads faster than a
                  dropdown and shows both states at once. Locked during
                  a capture: the container is fixed once encoding starts. */}
              <div
                className="format-pills col-span-2 sm:col-span-1"
                role="group"
                aria-label="recording format"
              >
                {(["gif", "mp4"] as recorder.RecorderFormat[]).map((fmt) => {
                  const unavailable = fmt === "mp4" && mp4Available === false;
                  return (
                    <button
                      key={fmt}
                      type="button"
                      className="format-pill"
                      data-testid={`btn-format-${fmt}`}
                      aria-pressed={recordFormat === fmt}
                      data-active={recordFormat === fmt ? "true" : "false"}
                      disabled={unavailable || recorderStatus.recording || recorderStatus.encoding}
                      title={
                        unavailable ? "mp4 needs WebCodecs, unavailable in this browser" : undefined
                      }
                      onClick={() => setRecordFormat(fmt)}
                    >
                      {fmt}
                    </button>
                  );
                })}
              </div>
            </div>

            {/* Instrumentation, not viewer-facing; the sim tick is the one
                actionable number and is captioned under the canvas. */}
            {debug && (
              <table className="input-label mt-5 w-full border-separate border-spacing-y-1">
                <tbody>
                  <tr>
                    <td>resolved stars</td>
                    <td className="text-right" data-testid="stat-stars">
                      {starCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>supernovae</td>
                    <td className="text-right" data-testid="stat-sn">
                      {(snCount + typeIaCount).toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>planetary nebulae</td>
                    <td className="text-right" data-testid="stat-planetary-nebulae">
                      {planetaryNebulaCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>phase-mixed stars</td>
                    <td className="text-right" data-testid="stat-phase-mixed">
                      {phaseMixedCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>black hole</td>
                    <td className="text-right">×{bhFactor.toFixed(2)}</td>
                  </tr>
                  {quasarActivity > 0 && (
                    <tr>
                      <td>quasar</td>
                      <td className="text-right" data-testid="stat-quasar">
                        {(quasarActivity * 100).toFixed(0)}%
                      </td>
                    </tr>
                  )}
                  <tr>
                    <td>gas reservoir</td>
                    <td className="text-right">{gasPct.toFixed(0)}%</td>
                  </tr>
                  <tr>
                    <td>red giants</td>
                    <td className="text-right" data-testid="stat-red-giants">
                      {redGiantCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>white dwarfs</td>
                    <td className="text-right" data-testid="stat-white-dwarfs">
                      {whiteDwarfCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>neutron stars</td>
                    <td className="text-right" data-testid="stat-neutron-stars">
                      {neutronStarCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>core-collapse supernovae</td>
                    <td className="text-right" data-testid="stat-core-collapse">
                      {snCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>type ia supernovae</td>
                    <td className="text-right" data-testid="stat-type-ia">
                      {typeIaCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>short gamma-ray bursts</td>
                    <td className="text-right" data-testid="stat-grb">
                      {grbCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>associations formed</td>
                    <td className="text-right" data-testid="stat-associations">
                      {associationCount.toLocaleString()}
                    </td>
                  </tr>
                  <tr>
                    <td>eaten by black hole</td>
                    <td className="text-right">{captureCount.toLocaleString()}</td>
                  </tr>
                  <tr>
                    <td>tick ms</td>
                    <td className="text-right" data-testid="stat-tick-ms">
                      {tickMs.toFixed(1)}
                    </td>
                  </tr>
                  <tr>
                    <td>fps</td>
                    <td className="text-right" data-testid="stat-fps">
                      {fps}
                    </td>
                  </tr>
                </tbody>
              </table>
            )}

            {!wasmReady && (
              <p className="mt-4 text-xs tracking-widest uppercase text-[color:var(--color-plum-400)]">
                loading wasm…
              </p>
            )}
          </section>
        </aside>

        {/* Mobile: takes the remaining column above the control bar.
            Desktop: full-bleed behind the floating panel, as before. */}
        <section className="relative min-h-0 flex-1 sm:fixed sm:inset-0 sm:z-0">
          <div id="dataviz" className="h-full w-full" />
          {/* The one viewer-facing number: `?t=` makes it an address. */}
          {!uiHidden && (
            <p className="tick-caption">
              tick <span data-testid="stat-ticks">{tickCount}</span>
            </p>
          )}
        </section>
      </main>
    </div>
  );
}

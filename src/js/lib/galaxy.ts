import * as wasm from "galaxy_gen_backend/galaxy_gen_backend";
import { WebGPUForceBackend, isWebGPUAvailable as _isWebGPUAvailable } from "./webgpu";

export interface Cell {
  mass: number;
  x: number;
  y: number;
}

/** Mirror of the Rust `InitialCondition` enum — kept in sync manually
 *  (wasm-bindgen exposes numeric discriminants). */
export enum InitialCondition {
  Uniform = 0,
  Rotation = 1,
  Bang = 2,
  Collision = 3,
}

export type ComputeBackend = "cpu" | "webgpu";

export const isWebGPUAvailable = _isWebGPUAvailable;

/**
 * Thin JS wrapper over the Rust/WASM Galaxy. `massArray()` returns a
 * `Uint16Array` (a single memcpy from WASM linear memory, courtesy of
 * wasm-bindgen). Positions are derived from cell index, so only masses
 * cross the boundary each tick.
 *
 * The main-thread Galaxy remains the source of truth for one-off
 * operations (init / seed / step). A Web Worker with its own
 * independent Galaxy runs the continuous tick loop — see `tick-worker.ts`
 * and the `TickWorker` class below. State is transferred in and out of
 * the worker via `snapshotState()` / `restoreState()` so the run loop can
 * resume from exactly where the main thread left off (and vice versa).
 */
export class Frontend {
  private galaxy: wasm.Galaxy;
  public galaxySize: number;
  // When a worker is driving the sim, we cache its latest mass snapshot
  // here so the renderer (which reads `massArray()` each frame) sees
  // fresh data without having to re-enter WASM on the main thread.
  private overrideMass: Uint16Array | null = null;

  // Backend selection. CPU path goes through `Galaxy.tick`; WebGPU path
  // computes forces in WGSL and hands accelerations to `Galaxy.tick_with_accel`.
  private backend: ComputeBackend = "cpu";
  private gpuBackend: WebGPUForceBackend | null = null;

  constructor(galaxySize: number) {
    this.galaxy = new wasm.Galaxy(galaxySize, 0);
    this.galaxySize = galaxySize;
  }

  /**
   * Enable the WebGPU backend. Throws if WebGPU is unavailable or device
   * init fails; the frontend stays on the previous backend in that case
   * so the caller can fall back cleanly.
   */
  public async enableWebGPU(): Promise<void> {
    const result = await WebGPUForceBackend.create();
    if (!result.ok || !result.backend) {
      throw new Error(result.reason ?? "webgpu unavailable");
    }
    this.gpuBackend = result.backend;
    this.backend = "webgpu";
  }

  public useCPU(): void {
    this.backend = "cpu";
    if (this.gpuBackend) {
      this.gpuBackend.destroy();
      this.gpuBackend = null;
    }
  }

  public currentBackend(): ComputeBackend {
    return this.backend;
  }

  public seed(additionalMass: number, mode: InitialCondition = InitialCondition.Uniform): void {
    this.overrideMass = null;
    const next = this.galaxy.seed_with_mode(
      additionalMass,
      mode as unknown as wasm.InitialCondition
    );
    this.galaxy.free();
    this.galaxy = next;
  }

  /**
   * Reproducible seed: given the same `(additionalMass, seed)` pair the
   * resulting mass field is byte-identical to any prior/future call with
   * the same inputs. This is what makes `?seed=…` URL sharing meaningful.
   * `seed` is a u64 on the Rust side; JS numbers are doubles, but we
   * pass it through as a BigInt so integer seeds up to 2^64-1 round-trip
   * cleanly.
   */
  public seedWith(additionalMass: number, seed: bigint): void {
    this.overrideMass = null;
    const next = this.galaxy.seed_with(additionalMass, seed);
    this.galaxy.free();
    this.galaxy = next;
  }

  public tick(timeModifier: number): void {
    this.overrideMass = null;
    const next = this.galaxy.tick(timeModifier);
    this.galaxy.free();
    this.galaxy = next;
  }

  /**
   * Async tick routed through the currently selected backend. CPU falls
   * through to the sync `tick()`. WebGPU computes accelerations on the
   * GPU and hands them to the Rust integrator via `tick_with_accel`. On
   * GPU failure we log a warning, fall back to CPU permanently, and
   * still advance the sim via the sync tick path.
   */
  public async tickAsync(timeModifier: number): Promise<void> {
    if (this.backend === "cpu" || !this.gpuBackend) {
      this.tick(timeModifier);
      return;
    }
    try {
      this.overrideMass = null;
      const mass = this.galaxy.mass();
      const { acc_x, acc_y } = await this.gpuBackend.computeAccelerations(mass, this.galaxySize);
      const next = this.galaxy.tick_with_accel(timeModifier, acc_x, acc_y);
      this.galaxy.free();
      this.galaxy = next;
    } catch (err) {
      console.warn("[webgpu] tick failed, falling back to CPU:", err);
      this.useCPU();
      this.tick(timeModifier);
    }
  }

  /** Fast path for the renderer — one memcpy, no per-cell object churn. */
  public massArray(): Uint16Array {
    return this.overrideMass ?? this.galaxy.mass();
  }

  /** Legacy API. Allocates a Cell[]; avoid on the hot path. */
  public cells(): Cell[] {
    const mass = this.massArray();
    const size = this.galaxySize;
    const out: Cell[] = new Array(mass.length);
    for (let i = 0; i < mass.length; i++) {
      out[i] = { mass: mass[i], x: i % size, y: (i / size) | 0 };
    }
    return out;
  }

  // --- Worker integration -------------------------------------------------

  /**
   * Full sim-state snapshot (mass + velocity + sub-grid position). Used
   * to hydrate a worker-side Galaxy before the run loop starts.
   */
  public snapshotState(): {
    size: number;
    mass: Uint16Array;
    velX: Float32Array;
    velY: Float32Array;
    fracX: Float32Array;
    fracY: Float32Array;
  } {
    return {
      size: this.galaxySize,
      mass: this.galaxy.mass(),
      velX: this.galaxy.vel_x(),
      velY: this.galaxy.vel_y(),
      fracX: this.galaxy.frac_x(),
      fracY: this.galaxy.frac_y(),
    };
  }

  /**
   * Rehydrate the main-thread Galaxy from worker state. Called when the
   * run loop pauses, so subsequent manual ticks pick up exactly where
   * the worker left off (velocity and fractional position preserved).
   */
  public restoreState(
    mass: Uint16Array,
    velX: Float32Array,
    velY: Float32Array,
    fracX: Float32Array,
    fracY: Float32Array,
  ): void {
    const next = wasm.Galaxy.from_state(
      this.galaxySize,
      mass,
      velX,
      velY,
      fracX,
      fracY,
    );
    this.galaxy.free();
    this.galaxy = next;
    this.overrideMass = null;
  }

  /**
   * Point the renderer at a mass buffer produced by the worker. This
   * lets us keep rendering at animation-frame cadence without pulling
   * mass out of WASM on the main thread each frame.
   */
  public setOverrideMass(mass: Uint16Array): void {
    this.overrideMass = mass;
  }
}

/**
 * Thin main-thread proxy over the physics Web Worker. The worker holds
 * its own Galaxy WASM instance; the main thread only receives mass
 * snapshots (via transferable ArrayBuffers, zero-copy).
 */
export class TickWorker {
  private worker: Worker;
  private onSnapshot: (mass: Uint16Array, tickMs: number, tickId: number) => void;
  private stopResolver: ((state: StoppedState | null) => void) | null = null;

  constructor(
    onSnapshot: (mass: Uint16Array, tickMs: number, tickId: number) => void,
  ) {
    if (typeof Worker === "undefined") {
      throw new Error(
        "Web Workers are not supported in this environment; TickWorker cannot be constructed.",
      );
    }
    this.worker = new Worker(new URL("./tick-worker.ts", import.meta.url), {
      type: "module",
    });
    this.onSnapshot = onSnapshot;
    this.worker.onmessage = (ev: MessageEvent) => this.handleMessage(ev);
  }

  private handleMessage(ev: MessageEvent) {
    const msg = ev.data;
    if (!msg || typeof msg.type !== "string") return;
    if (msg.type === "snapshot") {
      this.onSnapshot(msg.mass, msg.tickMs, msg.tickId);
    } else if (msg.type === "stopped") {
      if (!this.stopResolver) return;
      const resolver = this.stopResolver;
      this.stopResolver = null;
      // Worker only omits state if it was never initialized — surface that
      // to the caller as `null` so main thread knows to skip restoreState.
      if (msg.mass) {
        resolver({
          mass: msg.mass,
          velX: msg.velX,
          velY: msg.velY,
          fracX: msg.fracX,
          fracY: msg.fracY,
        });
      } else {
        resolver(null);
      }
    }
  }

  /**
   * Hydrate the worker-side Galaxy from a Frontend snapshot. Transfers
   * the typed-array buffers so no data is copied.
   */
  public init(snapshot: {
    size: number;
    mass: Uint16Array;
    velX: Float32Array;
    velY: Float32Array;
    fracX: Float32Array;
    fracY: Float32Array;
  }): void {
    this.worker.postMessage(
      {
        type: "init",
        size: snapshot.size,
        mass: snapshot.mass,
        velX: snapshot.velX,
        velY: snapshot.velY,
        fracX: snapshot.fracX,
        fracY: snapshot.fracY,
      },
      [
        snapshot.mass.buffer,
        snapshot.velX.buffer,
        snapshot.velY.buffer,
        snapshot.fracX.buffer,
        snapshot.fracY.buffer,
      ],
    );
  }

  public start(timeModifier: number): void {
    this.worker.postMessage({ type: "start", timeModifier });
  }

  public setTimeModifier(timeModifier: number): void {
    this.worker.postMessage({ type: "setTimeModifier", timeModifier });
  }

  /**
   * Stop the loop and resolve with the final state so the main thread
   * can rehydrate its Frontend (preserving velocity / fractional pos).
   * Resolves with `null` if the worker had no galaxy (i.e. stop called
   * before init ever completed).
   *
   * Callers must not invoke `stop()` twice concurrently — the UI layer
   * already guards this via `runningRef`.
   */
  public stop(): Promise<StoppedState | null> {
    if (this.stopResolver) {
      return Promise.reject(
        new Error("TickWorker.stop() is already in flight"),
      );
    }
    return new Promise<StoppedState | null>((resolve) => {
      this.stopResolver = resolve;
      this.worker.postMessage({ type: "stop" });
    });
  }

  public terminate(): void {
    this.worker.terminate();
  }
}

export interface StoppedState {
  mass: Uint16Array;
  velX: Float32Array;
  velY: Float32Array;
  fracX: Float32Array;
  fracY: Float32Array;
}

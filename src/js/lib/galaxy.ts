import * as wasm from "galaxy_gen_backend/galaxy_gen_backend";
import { WebGPUForceBackend, isWebGPUAvailable as _isWebGPUAvailable } from "./webgpu";

export interface Cell {
  mass: number;
  x: number;
  y: number;
}

/** Mirror of Rust `Scenario`. Kept in sync manually. A scenario is a
 * hardcoded `start => end-shape` pair - the seeder plus the physics
 * constants that steer the run toward its promised shape at t ~= 1000. */
export enum Scenario {
  BangRing = 0,
  BangSpiral = 1,
  IrregularSpiral = 2,
  IrregularElliptical = 3,
}

export type ComputeBackend = "cpu" | "webgpu";

export const isWebGPUAvailable = _isWebGPUAvailable;

/** JS wrapper over WASM Galaxy. See `TickWorker` for the worker integration. */
export class Frontend {
  private galaxy: wasm.Galaxy;
  public galaxySize: number;
  // Worker-driven mass snapshot, read by the renderer without re-entering WASM.
  private overrideMass: Uint16Array | null = null;
  private overrideFracX: Float32Array | null = null;
  private overrideFracY: Float32Array | null = null;
  private overrideStars: Float32Array | null = null;
  private overrideTransients: Float32Array | null = null;
  private overrideRadiation: Float32Array | null = null;
  private overrideLensScale: number | null = null;
  private overrideStellarHaloMass: number | null = null;
  // CPU uses `Galaxy.tick`; WebGPU uses `tick_with_accel` after WGSL forces.
  private backend: ComputeBackend = "cpu";
  private gpuBackend: WebGPUForceBackend | null = null;

  constructor(galaxySize: number) {
    this.galaxy = new wasm.Galaxy(galaxySize, 0);
    this.galaxySize = galaxySize;
  }

  /** Enable WebGPU. Throws on init failure, leaving previous backend. */
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

  public seed(additionalMass: number, mode: Scenario = Scenario.IrregularSpiral): void {
    this.overrideMass = null;
    this.overrideFracX = null;
    this.overrideFracY = null;
    this.overrideStellarHaloMass = null;
    const next = this.galaxy.seed_with_mode(additionalMass, mode as unknown as wasm.Scenario);
    this.galaxy.free();
    this.galaxy = next;
  }

  /** Reproducible seed for any mode. BigInt so u64 seeds round-trip cleanly. */
  public seedWith(
    additionalMass: number,
    seed: bigint,
    mode: Scenario = Scenario.IrregularSpiral
  ): void {
    this.overrideMass = null;
    this.overrideFracX = null;
    this.overrideFracY = null;
    this.overrideStellarHaloMass = null;
    const next = this.galaxy.seed_with_mode_seeded(
      additionalMass,
      mode as unknown as wasm.Scenario,
      seed
    );
    this.galaxy.free();
    this.galaxy = next;
  }

  public tick(timeModifier: number): void {
    this.overrideMass = null;
    this.overrideFracX = null;
    this.overrideFracY = null;
    this.overrideStars = null;
    this.overrideStellarHaloMass = null;
    const next = this.galaxy.tick(timeModifier);
    this.galaxy.free();
    this.galaxy = next;
  }

  /** Async tick. Routes via backend. GPU failure falls back to CPU permanently. */
  public async tickAsync(timeModifier: number): Promise<void> {
    if (this.backend === "cpu" || !this.gpuBackend) {
      this.tick(timeModifier);
      return;
    }
    try {
      this.overrideMass = null;
      this.overrideFracX = null;
      this.overrideFracY = null;
      this.overrideStellarHaloMass = null;
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

  /** Sub-cell gas position used to render continuous, physical motion. */
  public fracXArray(): Float32Array {
    return this.overrideFracX ?? this.galaxy.frac_x();
  }

  public fracYArray(): Float32Array {
    return this.overrideFracY ?? this.galaxy.frac_y();
  }

  public starCount(): number {
    if (this.overrideStars) return this.overrideStars.length / 5;
    return this.galaxy.star_count();
  }

  /** Renderer packing: [x, y, luminosity, colorIndex, stage] per star. */
  public starRenderArray(): Float32Array {
    return this.overrideStars ?? this.galaxy.star_render_data();
  }

  /** Renderer transients: [kind, x, y, ticksAgo, magnitude] per event. */
  public transientsArray(): Float32Array {
    return this.overrideTransients ?? this.galaxy.render_transients();
  }

  /** Coarse radiation field for gas temperature tinting. */
  public radiationArray(): Float32Array {
    return this.overrideRadiation ?? this.galaxy.radiation_field();
  }

  public setOverrideRadiation(radiation: Float32Array): void {
    this.overrideRadiation = radiation;
  }

  public supernovaCount(): number {
    return Number(this.galaxy.events_executed(2));
  }

  public birthCount(): number {
    return Number(this.galaxy.events_executed(1));
  }

  public captureCount(): number {
    return Number(this.galaxy.events_executed(5));
  }

  public neutronStarCount(): number {
    return this.galaxy.neutron_star_count();
  }

  public grbCount(): number {
    return Number(this.galaxy.events_executed(7));
  }

  public phaseMixedCount(): number {
    return Number(this.galaxy.phase_mixed_count());
  }

  public stellarHaloMass(): number {
    return this.overrideStellarHaloMass ?? this.galaxy.stellar_halo_mass_value();
  }

  public bhMass(): number {
    return this.galaxy.bh_mass_value();
  }

  /** Authoritative sim tick - the frame reference. */
  public tickCount(): number {
    return this.galaxy.sim_tick();
  }

  public gasTotal(): number {
    const mass = this.massArray();
    let total = 0;
    for (let i = 0; i < mass.length; i++) total += mass[i];
    return total;
  }

  /** Cold disk share of cold + hot halo gas, in [0, 1]. */
  public gasColdFraction(): number {
    return this.galaxy.gas_cold_fraction();
  }

  /** Lens depth relative to the seeded black hole; 0 = evaporated. */
  public lensScale(): number {
    return this.overrideLensScale ?? this.galaxy.bh_lens_scale();
  }

  public setOverrideLensScale(v: number): void {
    this.overrideLensScale = v;
  }

  public setOverrideStellarHaloMass(v: number): void {
    this.overrideStellarHaloMass = v;
  }

  /** Debug/test spawn; production stars come from StarBirth events. */
  public spawnStar(x: number, y: number, vx: number, vy: number, mass: number): number {
    this.overrideStars = null;
    return this.galaxy.spawn_star(x, y, vx, vy, mass);
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

  /** Full sim-state snapshot to hydrate a worker-side Galaxy. The stars /
   *  field / meta buffers are opaque: JS holds them and hands them back. */
  public snapshotState(): SimState {
    return {
      size: this.galaxySize,
      mass: this.galaxy.mass(),
      velX: this.galaxy.vel_x(),
      velY: this.galaxy.vel_y(),
      fracX: this.galaxy.frac_x(),
      fracY: this.galaxy.frac_y(),
      stars: this.galaxy.sim_state_stars(),
      field: this.galaxy.sim_state_field(),
      meta: this.galaxy.sim_state_meta(),
    };
  }

  /** Rehydrate main-thread Galaxy from worker state on pause. Restore
   *  order matters: stars, then field, then meta. */
  public restoreState(
    mass: Uint16Array,
    velX: Float32Array,
    velY: Float32Array,
    fracX: Float32Array,
    fracY: Float32Array,
    stars?: Float32Array,
    field?: Float32Array,
    meta?: Uint32Array
  ): void {
    const next = wasm.Galaxy.from_state(this.galaxySize, mass, velX, velY, fracX, fracY);
    if (stars) next.restore_sim_state_stars(stars);
    if (field) next.restore_sim_state_field(field);
    if (meta) next.restore_sim_state_meta(meta);
    this.galaxy.free();
    this.galaxy = next;
    this.overrideMass = null;
    this.overrideFracX = null;
    this.overrideFracY = null;
    this.overrideStars = null;
    this.overrideStellarHaloMass = null;
  }

  /** Point renderer at worker-produced mass buffer. Skips WASM round-trip. */
  public setOverrideMass(mass: Uint16Array): void {
    this.overrideMass = mass;
  }

  public setOverrideGasOffsets(fracX: Float32Array, fracY: Float32Array): void {
    this.overrideFracX = fracX;
    this.overrideFracY = fracY;
  }

  /** Point renderer at worker-produced star render buffer. */
  public setOverrideStars(stars: Float32Array): void {
    this.overrideStars = stars;
  }

  /** Point renderer at worker-produced transient buffer. */
  public setOverrideTransients(transients: Float32Array): void {
    this.overrideTransients = transients;
  }
}

export interface SimState {
  size: number;
  mass: Uint16Array;
  velX: Float32Array;
  velY: Float32Array;
  fracX: Float32Array;
  fracY: Float32Array;
  stars: Float32Array;
  field: Float32Array;
  meta: Uint32Array;
}

/** Main-thread proxy over the physics Web Worker. */
export class TickWorker {
  private worker: Worker;
  private onSnapshot: (
    mass: Uint16Array,
    fracX: Float32Array,
    fracY: Float32Array,
    tickMs: number,
    tickId: number,
    stars: Float32Array,
    transients: Float32Array,
    radiation: Float32Array,
    snCount: number,
    birthCount: number,
    captureCount: number,
    neutronStarCount: number,
    grbCount: number,
    phaseMixedCount: number,
    stellarHaloMass: number,
    bhMass: number,
    gasColdFraction: number,
    lensScale: number
  ) => void;
  private stopResolver: ((state: StoppedState | null) => void) | null = null;

  constructor(
    onSnapshot: (
      mass: Uint16Array,
      fracX: Float32Array,
      fracY: Float32Array,
      tickMs: number,
      tickId: number,
      stars: Float32Array,
      transients: Float32Array,
      radiation: Float32Array,
      snCount: number,
      birthCount: number,
      captureCount: number,
      neutronStarCount: number,
      grbCount: number,
      phaseMixedCount: number,
      stellarHaloMass: number,
      bhMass: number,
      gasColdFraction: number,
      lensScale: number
    ) => void
  ) {
    if (typeof Worker === "undefined") {
      throw new Error(
        "Web Workers are not supported in this environment; TickWorker cannot be constructed."
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
      this.onSnapshot(
        msg.mass,
        msg.fracX,
        msg.fracY,
        msg.tickMs,
        msg.tickId,
        msg.stars,
        msg.transients,
        msg.radiation,
        msg.snCount,
        msg.birthCount,
        msg.captureCount,
        msg.neutronStarCount,
        msg.grbCount,
        msg.phaseMixedCount,
        msg.stellarHaloMass,
        msg.bhMass,
        msg.gasColdFraction,
        msg.lensScale
      );
    } else if (msg.type === "stopped") {
      if (!this.stopResolver) return;
      const resolver = this.stopResolver;
      this.stopResolver = null;
      // Worker omits state if never initialized; surface as null.
      if (msg.mass) {
        resolver({
          mass: msg.mass,
          velX: msg.velX,
          velY: msg.velY,
          fracX: msg.fracX,
          fracY: msg.fracY,
          stars: msg.stars,
          field: msg.field,
          meta: msg.meta,
        });
      } else {
        resolver(null);
      }
    }
  }

  /** Hydrate worker-side Galaxy. Transfers buffers (zero-copy). */
  public init(snapshot: SimState): void {
    this.worker.postMessage(
      {
        type: "init",
        size: snapshot.size,
        mass: snapshot.mass,
        velX: snapshot.velX,
        velY: snapshot.velY,
        fracX: snapshot.fracX,
        fracY: snapshot.fracY,
        stars: snapshot.stars,
        field: snapshot.field,
        meta: snapshot.meta,
      },
      [
        snapshot.mass.buffer,
        snapshot.velX.buffer,
        snapshot.velY.buffer,
        snapshot.fracX.buffer,
        snapshot.fracY.buffer,
        snapshot.stars.buffer,
        snapshot.field.buffer,
        snapshot.meta.buffer,
      ]
    );
  }

  public start(timeModifier: number): void {
    this.worker.postMessage({ type: "start", timeModifier });
  }

  public setTimeModifier(timeModifier: number): void {
    this.worker.postMessage({ type: "setTimeModifier", timeModifier });
  }

  /** Stop the loop and resolve with final state. Null if worker uninitialized. */
  public stop(): Promise<StoppedState | null> {
    if (this.stopResolver) {
      return Promise.reject(new Error("TickWorker.stop() is already in flight"));
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
  stars: Float32Array;
  field: Float32Array;
  meta: Uint32Array;
}

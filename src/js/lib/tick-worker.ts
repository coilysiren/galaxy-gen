// Physics tick loop worker. See docs/tick-worker.md for the message protocol.

// WASM import is async; buffer inbound messages until the module resolves.
type WasmModule = typeof import("galaxy_gen_backend/galaxy_gen_backend");
let wasmMod: WasmModule | null = null;
const pending: InMsg[] = [];

interface InitMsg {
  type: "init";
  size: number;
  mass: Uint16Array;
  velX: Float32Array;
  velY: Float32Array;
  fracX: Float32Array;
  fracY: Float32Array;
  // Opaque sim-state buffers (stars, coarse field, scheduler/event meta):
  // restored verbatim, never interpreted here.
  stars: Float32Array;
  field: Float32Array;
  meta: Uint32Array;
}

interface StartMsg {
  type: "start";
  timeModifier: number;
}

interface SetTimeModifierMsg {
  type: "setTimeModifier";
  timeModifier: number;
}

interface StopMsg {
  type: "stop";
}

type InMsg = InitMsg | StartMsg | SetTimeModifierMsg | StopMsg;

// Instance of the dynamically imported `Galaxy` WASM class.
type GalaxyInstance = InstanceType<WasmModule["Galaxy"]>;
let galaxy: GalaxyInstance | null = null;
let timeModifier = 0.5;
let running = false;
let scheduled = false;

// Tick-rate ceiling, and the paint rate with it - the main thread draws
// once per snapshot. Why 20: docs/journal/perf-rewrite.md parts two and four.
const MAX_TICKS_PER_SEC = 20;
const MIN_TICK_INTERVAL_MS = 1000 / MAX_TICKS_PER_SEC;

function scheduleLoop(delayMs = 0) {
  if (scheduled) return;
  scheduled = true;
  // Yield between ticks so stop / setTimeModifier aren't starved.
  setTimeout(runOneTick, delayMs);
}

function runOneTick() {
  scheduled = false;
  if (!running || !galaxy) return;

  const t0 = performance.now();
  const next = galaxy.tick(timeModifier);
  galaxy.free();
  galaxy = next;
  const tickMs = performance.now() - t0;

  // WASM getters allocate transferable JS arrays. Gas offsets preserve
  // continuous sub-cell motion on the main-thread canvas.
  const mass: Uint16Array = galaxy.mass();
  const fracX: Float32Array = galaxy.frac_x();
  const fracY: Float32Array = galaxy.frac_y();
  const stars: Float32Array = galaxy.star_render_data();
  const transients: Float32Array = galaxy.render_transients();
  const radiation: Float32Array = galaxy.radiation_field();
  const metallicity: Float32Array = galaxy.gas_metallicity();
  const payload = {
    type: "snapshot" as const,
    mass,
    fracX,
    fracY,
    tickMs,
    // The sim's own tick counter - continuous across pause/resume, so
    // the UI's frame reference never resets mid-run.
    tickId: galaxy.sim_tick(),
    stars,
    transients,
    radiation,
    metallicity,
    snCount: Number(galaxy.events_executed(2)),
    associationCount: galaxy.stellar_association_count(),
    captureCount: Number(galaxy.events_executed(5)),
    neutronStarCount: galaxy.neutron_star_count(),
    redGiantCount: galaxy.red_giant_count(),
    whiteDwarfCount: galaxy.white_dwarf_count(),
    planetaryNebulaCount: Number(galaxy.events_executed(8)),
    typeIaCount: Number(galaxy.events_executed(9)),
    grbCount: Number(galaxy.events_executed(7)),
    phaseMixedCount: Number(galaxy.phase_mixed_count()),
    stellarHaloMass: galaxy.stellar_halo_mass_value(),
    bhMass: galaxy.bh_mass_value(),
    gasColdFraction: galaxy.gas_cold_fraction(),
    lensScale: galaxy.bh_lens_scale(),
    quasarActivity: galaxy.quasar_activity(),
    quasarPulse: galaxy.quasar_pulse_strength(),
    quasarAge: galaxy.quasar_age_value(),
    quasarPulsePeriod: galaxy.quasar_pulse_period_value(),
    quasarAxis: galaxy.quasar_axis_value(),
    quasarEpisodes: galaxy.quasar_episode_count(),
  };
  (self as unknown as Worker).postMessage(payload, [
    mass.buffer,
    fracX.buffer,
    fracY.buffer,
    stars.buffer,
    transients.buffer,
    radiation.buffer,
    metallicity.buffer,
  ]);

  scheduleLoop(Math.max(0, MIN_TICK_INTERVAL_MS - tickMs));
}

function handleInit(msg: InitMsg) {
  if (!wasmMod) return;
  if (galaxy) {
    galaxy.free();
    galaxy = null;
  }
  galaxy = wasmMod.Galaxy.from_state(msg.size, msg.mass, msg.velX, msg.velY, msg.fracX, msg.fracY);
  // Restore order matters: stars, then field, then meta.
  galaxy.restore_sim_state_stars(msg.stars);
  galaxy.restore_sim_state_field(msg.field);
  galaxy.restore_sim_state_meta(msg.meta);
}

function handleStart(msg: StartMsg) {
  if (!galaxy) return;
  timeModifier = msg.timeModifier;
  if (running) return;
  running = true;
  scheduleLoop();
}

function handleSetTimeModifier(msg: SetTimeModifierMsg) {
  timeModifier = msg.timeModifier;
}

function handleStop() {
  running = false;
  if (!galaxy) {
    (self as unknown as Worker).postMessage({ type: "stopped" });
    return;
  }
  const mass: Uint16Array = galaxy.mass();
  const velX: Float32Array = galaxy.vel_x();
  const velY: Float32Array = galaxy.vel_y();
  const fracX: Float32Array = galaxy.frac_x();
  const fracY: Float32Array = galaxy.frac_y();
  const stars: Float32Array = galaxy.sim_state_stars();
  const field: Float32Array = galaxy.sim_state_field();
  const meta: Uint32Array = galaxy.sim_state_meta();
  const payload = {
    type: "stopped" as const,
    mass,
    velX,
    velY,
    fracX,
    fracY,
    stars,
    field,
    meta,
  };
  (self as unknown as Worker).postMessage(payload, [
    mass.buffer,
    velX.buffer,
    velY.buffer,
    fracX.buffer,
    fracY.buffer,
    stars.buffer,
    field.buffer,
    meta.buffer,
  ]);
}

function dispatch(msg: InMsg) {
  switch (msg.type) {
    case "init":
      handleInit(msg);
      break;
    case "start":
      handleStart(msg);
      break;
    case "setTimeModifier":
      handleSetTimeModifier(msg);
      break;
    case "stop":
      handleStop();
      break;
  }
}

self.onmessage = (ev: MessageEvent<InMsg>) => {
  if (!wasmMod) {
    pending.push(ev.data);
    return;
  }
  dispatch(ev.data);
};

import("galaxy_gen_backend/galaxy_gen_backend").then((mod) => {
  wasmMod = mod;
  while (pending.length > 0) {
    const msg = pending.shift()!;
    dispatch(msg);
  }
});

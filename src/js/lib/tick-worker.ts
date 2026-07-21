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
let tickId = 0;
let scheduled = false;

// Tick-rate ceiling. Uncapped, small grids run thousands of ticks/sec and
// the sim evolves faster than anyone can watch.
const MAX_TICKS_PER_SEC = 30;
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

  // `galaxy.mass()` allocates a JS-heap Uint16Array; safe to transfer.
  const mass: Uint16Array = galaxy.mass();
  const stars: Float32Array = galaxy.star_render_data();
  const transients: Float32Array = galaxy.render_transients();
  const radiation: Float32Array = galaxy.radiation_field();
  let gasTotal = 0;
  for (let i = 0; i < mass.length; i++) gasTotal += mass[i];
  tickId += 1;
  const payload = {
    type: "snapshot" as const,
    mass,
    tickMs,
    tickId,
    stars,
    transients,
    radiation,
    snCount: Number(galaxy.events_executed(2)),
    birthCount: Number(galaxy.events_executed(1)),
    captureCount: Number(galaxy.events_executed(5)),
    bhMass: galaxy.bh_mass_value(),
    gasTotal,
    lensScale: galaxy.bh_lens_scale(),
  };
  (self as unknown as Worker).postMessage(payload, [
    mass.buffer,
    stars.buffer,
    transients.buffer,
    radiation.buffer,
  ]);

  scheduleLoop(Math.max(0, MIN_TICK_INTERVAL_MS - tickMs));
}

function handleInit(msg: InitMsg) {
  if (!wasmMod) return;
  if (galaxy) {
    galaxy.free();
    galaxy = null;
  }
  galaxy = wasmMod.Galaxy.from_state(
    msg.size,
    msg.mass,
    msg.velX,
    msg.velY,
    msg.fracX,
    msg.fracY,
  );
  // Restore order matters: stars, then field, then meta.
  galaxy.restore_sim_state_stars(msg.stars);
  galaxy.restore_sim_state_field(msg.field);
  galaxy.restore_sim_state_meta(msg.meta);
  tickId = 0;
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

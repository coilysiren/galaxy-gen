/**
 * Web Worker entry for the physics tick loop.
 *
 * Owns an independent `Galaxy` WASM instance and runs `tick` in a loop
 * off the main thread so the browser stays responsive during large sims.
 *
 * Message protocol (main → worker):
 *   { type: "init", size, mass, velX, velY, fracX, fracY }
 *       Hydrate a new Galaxy from state transferred from the main thread.
 *   { type: "start", timeModifier }
 *       Begin looping: tick → postMessage(snapshot) → schedule next.
 *   { type: "setTimeModifier", timeModifier }
 *       Live-update dt without stopping.
 *   { type: "stop" }
 *       Halt the loop; worker sends back final full state for rehydration.
 *
 * Message protocol (worker → main):
 *   { type: "snapshot", mass, tickMs, tickId }
 *       Per-tick mass snapshot (Uint16Array, transferred).
 *   { type: "stopped", mass, velX, velY, fracX, fracY }
 *       Final state after a stop, so main thread can rehydrate its
 *       own Galaxy without losing velocity / sub-grid position.
 */

// The WASM import is async (webpack's `asyncWebAssembly: true`). We
// register the message handler synchronously and buffer any inbound
// messages until the module resolves, so the main thread never has to
// wait for worker readiness.
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

function scheduleLoop() {
  if (scheduled) return;
  scheduled = true;
  // setTimeout 0 yields between ticks so the worker can process
  // incoming messages (stop / setTimeModifier) and avoid starving
  // the message loop on a heavy tick.
  setTimeout(runOneTick, 0);
}

function runOneTick() {
  scheduled = false;
  if (!running || !galaxy) return;

  const t0 = performance.now();
  const next = galaxy.tick(timeModifier);
  galaxy.free();
  galaxy = next;
  const tickMs = performance.now() - t0;

  // Copy mass into a fresh buffer we can transfer. `galaxy.mass()`
  // already allocates a Uint16Array backed by its own memory on the
  // JS heap, so it's safe to transfer without corrupting WASM memory.
  const mass: Uint16Array = galaxy.mass();
  tickId += 1;
  const payload = {
    type: "snapshot" as const,
    mass,
    tickMs,
    tickId,
  };
  (self as unknown as Worker).postMessage(payload, [mass.buffer]);

  scheduleLoop();
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
  const payload = {
    type: "stopped" as const,
    mass,
    velX,
    velY,
    fracX,
    fracY,
  };
  (self as unknown as Worker).postMessage(payload, [
    mass.buffer,
    velX.buffer,
    velY.buffer,
    fracX.buffer,
    fracY.buffer,
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

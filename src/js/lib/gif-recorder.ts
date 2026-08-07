/// Client-side GIF capture of a progressing run.
///
/// The recorder subscribes to the one funnel every render path already
/// goes through (`dataviz.updateData` -> `notifyFrame`), so a captured
/// frame is always a finished draw at a known sim tick - no sampling on
/// an independent timer, which would race the render and duplicate or
/// tear frames.
///
/// Two costs shape the design. At size 500 a mature run already spends
/// its whole frame budget drawing (~54ms median), so the capture path
/// must not add a full-resolution `getImageData` per frame. And raw
/// frames are far too big to hold: 960x540 RGBA is ~2MB, so a 200-frame
/// run would buffer 400MB. So each capture immediately downsamples to
/// the target width and queues only that, and the queue is drained into
/// palette-quantized GIF frames between animation frames.

import { GIFEncoder, quantize, applyPalette } from "gifenc";

export interface RecorderOptions {
  /// Capture one frame per this many sim ticks. The sim advances far
  /// faster than a watchable GIF, so frames are decimated rather than
  /// taken one-per-draw.
  ticksPerFrame: number;
  /// Hard cap; recording auto-stops here so a forgotten session cannot
  /// grow without bound.
  maxFrames: number;
  /// Output width in pixels. Height follows the canvas aspect ratio.
  width: number;
  /// GIF playback rate.
  frameRate: number;
}

export const DEFAULT_OPTIONS: RecorderOptions = {
  ticksPerFrame: 10,
  maxFrames: 240,
  width: 640,
  frameRate: 12,
};

/// Beyond this many frames waiting to be quantized, `capture` encodes
/// inline instead of queueing. That trades a visible hitch for a bound
/// on memory, and it never silently drops a frame - a GIF missing
/// arbitrary frames is worse than one that stuttered while recording.
const QUEUE_LIMIT = 8;

export interface RecorderStatus {
  recording: boolean;
  frames: number;
  maxFrames: number;
  encoding: boolean;
}

interface Pending {
  data: Uint8ClampedArray;
  width: number;
  height: number;
}

type StatusListener = (status: RecorderStatus) => void;

let options: RecorderOptions = { ...DEFAULT_OPTIONS };
let encoder: ReturnType<typeof GIFEncoder> | null = null;
let scratch: HTMLCanvasElement | null = null;
let recording = false;
let finishing = false;
let frames = 0;
let lastCapturedTick: number | null = null;
let queue: Pending[] = [];
let draining = false;
let listeners: StatusListener[] = [];
/// Names the artifact after what produced it, so a downloaded GIF is
/// still traceable to a reproducible `?seed=...` URL.
let label = "galaxy";

function status(): RecorderStatus {
  return { recording, frames, maxFrames: options.maxFrames, encoding: finishing };
}

function emit() {
  const snapshot = status();
  for (const fn of listeners) fn(snapshot);
}

export function subscribe(fn: StatusListener): () => void {
  listeners.push(fn);
  fn(status());
  return () => {
    listeners = listeners.filter((l) => l !== fn);
  };
}

export function getStatus(): RecorderStatus {
  return status();
}

export function isRecording(): boolean {
  return recording;
}

export function start(runLabel: string, overrides: Partial<RecorderOptions> = {}) {
  if (recording || finishing) return;
  options = { ...DEFAULT_OPTIONS, ...overrides };
  encoder = GIFEncoder();
  scratch = document.createElement("canvas");
  recording = true;
  frames = 0;
  lastCapturedTick = null;
  queue = [];
  label = runLabel;
  emit();
}

/// Downsample the finished canvas and queue it. Called from the render
/// funnel, so it stays cheap: one `drawImage` plus one `getImageData` at
/// the reduced size. Quantization happens off this path.
export function capture(canvas: HTMLCanvasElement, simTick: number) {
  if (!recording || !scratch) return;
  if (lastCapturedTick != null && simTick - lastCapturedTick < options.ticksPerFrame) return;
  if (canvas.width === 0 || canvas.height === 0) return;
  lastCapturedTick = simTick;

  const width = Math.max(2, Math.min(options.width, canvas.width));
  // Even dimensions keep the scaled output free of half-pixel seams.
  const height = Math.max(2, Math.round((width * canvas.height) / canvas.width) & ~1);
  if (scratch.width !== width || scratch.height !== height) {
    scratch.width = width;
    scratch.height = height;
  }
  const ctx = scratch.getContext("2d", { willReadFrequently: true });
  if (!ctx) return;
  ctx.drawImage(canvas, 0, 0, width, height);
  const pending: Pending = { data: ctx.getImageData(0, 0, width, height).data, width, height };

  // Backpressure: encode inline rather than let the queue grow.
  if (queue.length >= QUEUE_LIMIT) {
    encodeOne(pending);
    return;
  }
  queue.push(pending);
  scheduleDrain();
}

function scheduleDrain() {
  if (draining || queue.length === 0) return;
  draining = true;
  const drain = () => {
    const next = queue.shift();
    if (next) encodeOne(next);
    draining = false;
    if (queue.length > 0) scheduleDrain();
  };
  // Prefer idle time, but never wait longer than a frame or two - the
  // queue has to clear faster than captures arrive.
  const ric = (window as unknown as { requestIdleCallback?: (cb: () => void, o?: object) => void })
    .requestIdleCallback;
  if (typeof ric === "function") ric(drain, { timeout: 100 });
  else window.setTimeout(drain, 0);
}

function encodeOne(frame: Pending) {
  if (!encoder) return;
  // A per-frame palette costs bytes but holds up far better than one
  // global palette: a run's color range shifts hard as gas drains and
  // stars ignite, and a fixed palette bands the late frames.
  const palette = quantize(frame.data, 256, { format: "rgb444" });
  const indexed = applyPalette(frame.data, palette, "rgb444");
  encoder.writeFrame(indexed, frame.width, frame.height, {
    palette,
    delay: Math.round(1000 / options.frameRate),
  });
  frames += 1;
  if (frames >= options.maxFrames && recording) {
    void stop();
    return;
  }
  emit();
}

/// Finish the GIF and hand it back as a downloadable blob. Drains
/// whatever is still queued first so the tail of the run is not lost.
export async function stop(): Promise<Blob | null> {
  if (!recording || !encoder) return null;
  recording = false;
  finishing = true;
  emit();

  while (queue.length > 0) {
    const next = queue.shift();
    if (next) encodeOne(next);
  }

  const active = encoder;
  encoder = null;
  scratch = null;
  finishing = false;

  if (frames === 0) {
    emit();
    return null;
  }
  active.finish();
  const blob = new Blob([active.bytesView() as unknown as BlobPart], { type: "image/gif" });
  emit();
  return blob;
}

/// Abandon a recording without producing a file.
export function cancel() {
  recording = false;
  finishing = false;
  encoder = null;
  scratch = null;
  queue = [];
  frames = 0;
  lastCapturedTick = null;
  emit();
}

export function fileName(): string {
  return `${label}.gif`;
}

export function download(blob: Blob, name: string) {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = name;
  document.body.appendChild(a);
  a.click();
  a.remove();
  // Revoking immediately can cancel the download in some browsers.
  window.setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

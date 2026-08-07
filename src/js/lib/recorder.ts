/// Client-side capture of a progressing run, as GIF or MP4.
///
/// The recorder subscribes to the one funnel every render path already
/// goes through (`dataviz.updateData` -> `notifyFrame`), so a captured
/// frame is always a finished draw at a known sim tick - no sampling on
/// an independent timer, which would race the render and duplicate or
/// tear frames. Decimation is by sim tick, not wall clock, so a capture
/// of a `?seed=` run is reproducible frame for frame.
///
/// Two costs shape the design. At size 500 a mature run already spends
/// its whole frame budget drawing (~54ms median), so the capture path
/// must not add a full-resolution `getImageData` per frame. And raw
/// frames are far too big to hold: 960x540 RGBA is ~2MB, so a 200-frame
/// run would buffer 400MB. So each capture immediately downsamples into
/// a scratch canvas and hands that on.
///
/// From there the two formats diverge. GIF keeps the original queue:
/// `getImageData` off the scratch canvas, drained into palette-quantized
/// frames between animation frames. MP4 hands the scratch canvas
/// straight to Mediabunny's `CanvasSource`, which owns its own
/// WebCodecs encoder and backpressure, so there is no second queue and
/// no pixel readback on the main thread at all.
///
/// MP4 needs WebCodecs. Where it is missing the format silently reverts
/// to GIF rather than failing a capture the viewer already started.
/// See docs/recording.md.

import { GIFEncoder, quantize, applyPalette } from "gifenc";
import { Output, Mp4OutputFormat, BufferTarget, CanvasSource, QUALITY_HIGH } from "mediabunny";

export type RecorderFormat = "gif" | "mp4";

export interface RecorderOptions {
  /// Capture one frame per this many sim ticks - the sim advances far
  /// faster than a watchable clip, so frames are decimated.
  ticksPerFrame: number;
  /// Hard cap; recording auto-stops here so a forgotten session cannot
  /// grow without bound.
  maxFrames: number;
  /// Output width in pixels. Height follows the canvas aspect ratio.
  width: number;
  /// Playback rate of the encoded file.
  frameRate: number;
  /// Container and codec for this capture.
  format: RecorderFormat;
}

export const DEFAULT_OPTIONS: RecorderOptions = {
  ticksPerFrame: 10,
  maxFrames: 240,
  width: 640,
  frameRate: 12,
  format: "gif",
};

/// Past this many pending frames, `capture` encodes inline: bounded
/// memory over a dropped frame. GIF path only. See docs/recording.md.
const QUEUE_LIMIT = 8;

/// H.264 rejects odd dimensions, so both axes round down to even. GIF
/// does not care, and matching the two keeps one sizing rule.
function evenDown(n: number): number {
  return Math.max(2, Math.floor(n) & ~1);
}

/// WebCodecs plus an H.264 encoder. Checked once per page: the answer
/// cannot change within a session, and `canEncodeVideo` probes the
/// hardware, so it is not free.
let mp4SupportProbe: Promise<boolean> | null = null;
export function isMp4Available(): Promise<boolean> {
  if (mp4SupportProbe) return mp4SupportProbe;
  mp4SupportProbe = (async () => {
    if (typeof globalThis.VideoEncoder === "undefined") return false;
    try {
      const { canEncodeVideo } = await import("mediabunny");
      return await canEncodeVideo("avc");
    } catch {
      return false;
    }
  })();
  return mp4SupportProbe;
}

export interface RecorderStatus {
  recording: boolean;
  frames: number;
  maxFrames: number;
  encoding: boolean;
  format: RecorderFormat;
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
/// Names the artifact after what produced it, so a downloaded capture is
/// still traceable to a reproducible `?seed=...` URL.
let label = "galaxy";

/// MP4 state. `mp4Source` is created on the first captured frame rather
/// than at `start`, because Mediabunny fixes track dimensions at track
/// creation and the canvas size is not known until then.
let mp4Output: Output<Mp4OutputFormat, BufferTarget> | null = null;
let mp4Source: CanvasSource | null = null;
/// Serializes `add` calls. The render funnel is synchronous but encoding
/// is not, so without this a fast run interleaves frames out of order.
let mp4Chain: Promise<void> = Promise.resolve();

function status(): RecorderStatus {
  return {
    recording,
    frames,
    maxFrames: options.maxFrames,
    encoding: finishing,
    format: options.format,
  };
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
  // GIF encoder is cheap to hold and is the fallback if the MP4 track
  // cannot be created, so it is always constructed.
  encoder = GIFEncoder();
  scratch = document.createElement("canvas");
  recording = true;
  frames = 0;
  lastCapturedTick = null;
  queue = [];
  mp4Output = null;
  mp4Source = null;
  mp4Chain = Promise.resolve();
  label = runLabel;
  emit();
}

/// Downsample the finished canvas, then hand it to whichever encoder
/// the capture is using. Called from the render funnel, so it stays
/// cheap - quantization and video encoding both happen off this path.
export function capture(canvas: HTMLCanvasElement, simTick: number) {
  if (!recording || !scratch) return;
  if (lastCapturedTick != null && simTick - lastCapturedTick < options.ticksPerFrame) return;
  if (canvas.width === 0 || canvas.height === 0) return;
  lastCapturedTick = simTick;

  // Even dimensions keep the scaled output free of half-pixel seams,
  // and H.264 refuses odd ones outright.
  const width = evenDown(Math.min(options.width, canvas.width));
  const height = evenDown((width * canvas.height) / canvas.width);
  if (scratch.width !== width || scratch.height !== height) {
    // Mediabunny fixes the track dimensions when the source is created,
    // so a mid-capture resize cannot be honoured. Keep the first size.
    if (mp4Source && frames > 0) return;
    scratch.width = width;
    scratch.height = height;
  }
  const ctx = scratch.getContext("2d", { willReadFrequently: options.format === "gif" });
  if (!ctx) return;
  ctx.drawImage(canvas, 0, 0, width, height);

  if (options.format === "mp4") {
    captureMp4();
    return;
  }

  const pending: Pending = { data: ctx.getImageData(0, 0, width, height).data, width, height };

  // Backpressure: encode inline rather than let the queue grow.
  if (queue.length >= QUEUE_LIMIT) {
    encodeOne(pending);
    return;
  }
  queue.push(pending);
  scheduleDrain();
}

/// Push the scratch canvas into the MP4 track. No pixel readback: the
/// encoder reads the canvas directly. `CanvasSource.add` resolves on
/// encoder backpressure, so the chain is what keeps memory bounded.
function captureMp4() {
  if (!scratch) return;
  const frameIndex = frames;
  frames += 1;

  if (!mp4Source) {
    mp4Output = new Output({ format: new Mp4OutputFormat(), target: new BufferTarget() });
    mp4Source = new CanvasSource(scratch, { codec: "avc", bitrate: QUALITY_HIGH });
    mp4Output.addVideoTrack(mp4Source, { frameRate: options.frameRate });
    mp4Chain = mp4Output.start();
  }

  const source = mp4Source;
  const step = 1 / options.frameRate;
  mp4Chain = mp4Chain
    .then(() => source.add(frameIndex * step, step))
    .catch((err) => {
      console.error("mp4 frame encode failed", err);
    });

  if (frames >= options.maxFrames && recording) {
    void stop();
    return;
  }
  emit();
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
  // Prefer idle time, but never wait longer than a frame or two.
  const ric = (window as unknown as { requestIdleCallback?: (cb: () => void, o?: object) => void })
    .requestIdleCallback;
  if (typeof ric === "function") ric(drain, { timeout: 100 });
  else window.setTimeout(drain, 0);
}

function encodeOne(frame: Pending) {
  if (!encoder) return;
  // Per-frame palette: a run's color range shifts hard as gas drains
  // and stars ignite, and a fixed palette bands the late frames.
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

/// Finish the capture and hand it back as a downloadable blob. Drains
/// whatever is still pending first so the tail of the run is not lost.
export async function stop(): Promise<Blob | null> {
  if (!recording) return null;
  recording = false;
  finishing = true;
  emit();

  const blob = options.format === "mp4" ? await finishMp4() : finishGif();

  encoder = null;
  scratch = null;
  mp4Output = null;
  mp4Source = null;
  finishing = false;
  emit();
  return blob;
}

function finishGif(): Blob | null {
  if (!encoder) return null;
  while (queue.length > 0) {
    const next = queue.shift();
    if (next) encodeOne(next);
  }
  if (frames === 0) return null;
  encoder.finish();
  return new Blob([encoder.bytesView() as unknown as BlobPart], { type: "image/gif" });
}

async function finishMp4(): Promise<Blob | null> {
  const output = mp4Output;
  if (!output || frames === 0) return null;
  // Every queued `add` must land before finalize, or the tail frames
  // are silently dropped from the file.
  await mp4Chain;
  await output.finalize();
  const buffer = output.target.buffer;
  return buffer ? new Blob([buffer], { type: "video/mp4" }) : null;
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
  // Cancel releases the encoder without finalizing; the partial buffer
  // is dropped with the output.
  void mp4Output?.cancel().catch(() => {});
  mp4Output = null;
  mp4Source = null;
  mp4Chain = Promise.resolve();
  emit();
}

export function fileName(): string {
  return `${label}.${options.format}`;
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

# Recording internals

How `src/js/lib/recorder.ts` gets frames out of the renderer and into a file.
The user-facing surface is in [recording.md](recording.md).

## Sampling the render funnel

The recorder subscribes to `dataviz.setFrameListener`, which fires at the end of
`updateData` - the one funnel every render path already goes through, for the
live worker loop, single steps, and the initial draw alike. Sampling the canvas
on an independent timer would race the renderer and yield duplicated or
half-drawn frames. Subscribing to the funnel means a captured frame is always a
completed draw paired with the sim tick that produced it.

Both formats share that funnel and share the downsample. Each capture does one
`drawImage` into a scratch canvas at the target width. From there they diverge,
because the two encoders want the frame in different shapes.

## Frame budget

At size 500 a mature run already spends its whole budget drawing, around 54ms
median per frame. The GIF path does the least possible work inline: one
`getImageData` at the reduced size, and the result goes on a queue.
Quantization and GIF encoding happen off that path, drained through
`requestIdleCallback` with a 100ms timeout fallback. The MP4 path does even
less, because Mediabunny's `CanvasSource` reads the scratch canvas directly.
There is no pixel readback on the main thread at all.

## Memory and backpressure

Raw frames are far too big to hold. A 960x540 RGBA frame is about 2MB, so a
200-frame run would buffer 400MB. Downsampling before queueing, and encoding
queued frames into the output byte stream rather than holding them, keeps the
working set to the compressed output plus at most a few pending frames.

Budget and memory pull against each other, and each format meets them
differently. GIF uses an explicit queue: past 8 pending frames the recorder
encodes inline instead of queueing. That is deliberate backpressure, trading a
visible hitch for a bound on memory, and it never silently drops a frame. A file
missing arbitrary frames is worse than one that stuttered while recording.

MP4 gets the same property for free. `CanvasSource.add` resolves only when the
encoder is ready for more, so awaiting it in a chain is the backpressure. The
chain also keeps frames in order, which matters because the render funnel is
synchronous and the encoder is not.

## MP4 constraints that leak into shared code

H.264 rejects odd dimensions, so both axes round down to even via `evenDown`.
GIF does not care, and one sizing rule beats two.

Mediabunny fixes track dimensions when the source is created, so a mid-capture
canvas resize cannot be honored. The recorder keeps the first size rather than
producing a file that claims one resolution and carries another.

## Tuning

Defaults live in `DEFAULT_OPTIONS`:

- `ticksPerFrame: 10` - the sim advances far faster than a watchable clip, so
  frames are decimated by sim tick rather than taken one per draw. Decimating by
  tick rather than wall clock is what makes a capture of a `?seed=` run
  reproducible frame for frame.
- `maxFrames: 240` - recording auto-stops here so a forgotten session cannot
  grow without bound.
- `width: 640` - output width. Height follows the canvas aspect ratio, rounded
  down to even.
- `frameRate: 12` - playback rate, written into each GIF frame's delay and into
  the MP4 track metadata.
- `format: "gif"` - the default container.

For GIF, each frame gets its own palette rather than sharing a global one. That
costs bytes, but a run's color range shifts hard as gas drains and stars ignite,
and a single fixed palette visibly bands the late frames. MP4 has no equivalent
problem, which is most of why it looks better.

## See also

- [recording.md](recording.md) - the user-facing surface.
- [perf-rewrite.md](journal/perf-rewrite.md) - where the frame budget went.

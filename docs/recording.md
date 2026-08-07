# Recording

Capture a run as an animated GIF or an MP4 from the browser. No checkout, no
ffmpeg, no dev server - the whole path runs client-side on the public site.

## Using it

1. Generate a galaxy.
2. Pick a format with the **gif** / **mp4** pills.
3. Press **record**. The button switches to `stop (n/240)` and counts frames as
   they are banked. The format pills lock for the duration.
4. Play the run, step it, or leave it running. The recorder captures whatever
   happens next.
5. Press **stop**. The file encodes and downloads.

The file is named for the permalink that reproduces the run, so
`galaxy-3885981479949904436-irregular-spiral-500.mp4` came from
`?seed=3885981479949904436&scenario=irregular-spiral&size=500`. Someone handed
the file can still reach the exact galaxy that produced it.

Recording captures the canvas only, so no control panel appears in the output.
Pair it with the chrome toggle (or load `?ui=0`) if you also want the live page
clean while recording. For a maintainer asset, `ward exec capture-readme`
remains the headless path and is unaffected by any of this.

## Choosing a format

MP4 is the better output on every axis that matters - true color instead of 256
palette entries, hardware encoding, and roughly an order of magnitude smaller
for the same run. GIF stays the default because it pastes into more places
without a player, and because it is the format with no capability floor.

MP4 needs WebCodecs and an H.264 encoder. `isMp4Available()` probes both once
per page via Mediabunny's `canEncodeVideo("avc")`, and the **mp4** pill renders
disabled where the answer is no, with the reason in its tooltip. Disabled
rather than absent: a viewer should be able to see that the option exists and
why it is unavailable, rather than wonder whether the site is broken.

## How it works

The recorder subscribes to `dataviz.setFrameListener`, which fires at the end
of `updateData` - the one funnel every render path already goes through, for
the live worker loop, single steps, and the initial draw alike. That matters:
sampling the canvas on an independent timer would race the renderer and yield
duplicated or half-drawn frames. Subscribing to the funnel means a captured
frame is always a completed draw paired with the sim tick that produced it.

Both formats share that funnel and share the downsample. Each capture does one
`drawImage` into a scratch canvas at the target width. From there they diverge,
because the two encoders want the frame in different shapes.

**Frame budget.** At size 500 a mature run already spends its whole budget
drawing, around 54ms median per frame. The GIF path does the least possible
work inline: one `getImageData` at the reduced size, and the result goes on a
queue. Quantization and GIF encoding happen off that path, drained through
`requestIdleCallback` with a 100ms timeout fallback. The MP4 path does even
less, because Mediabunny's `CanvasSource` reads the scratch canvas directly.
There is no pixel readback on the main thread at all.

**Memory.** Raw frames are far too big to hold. A 960x540 RGBA frame is about
2MB, so a 200-frame run would buffer 400MB. Downsampling before queueing, and
encoding queued frames into the output byte stream rather than holding them,
keeps the working set to the compressed output plus at most a few pending
frames.

Those two pull against each other, and each format meets them differently. GIF
uses an explicit queue: past 8 pending frames the recorder encodes inline
instead of queueing. That is deliberate backpressure, trading a visible hitch
for a bound on memory, and it never silently drops a frame. A file missing
arbitrary frames is worse than one that stuttered while recording. MP4 gets the
same property for free - `CanvasSource.add` resolves only when the encoder is
ready for more, so awaiting it in a chain is the backpressure. The chain also
keeps frames in order, which matters because the render funnel is synchronous
and the encoder is not.

Two MP4 constraints leak into the shared code. H.264 rejects odd dimensions, so
both axes round down to even via `evenDown` (GIF does not care, and one sizing
rule beats two). And Mediabunny fixes track dimensions when the source is
created, so a mid-capture canvas resize cannot be honored - the recorder keeps
the first size rather than producing a file that claims one resolution and
carries another.

## Tuning

Defaults live in `DEFAULT_OPTIONS` in `src/js/lib/recorder.ts`:

- `ticksPerFrame: 10` - the sim advances far faster than a watchable clip, so
  frames are decimated by sim tick rather than taken one per draw. Decimating
  by tick rather than wall clock is what makes a capture of a `?seed=` run
  reproducible frame for frame.
- `maxFrames: 240` - recording auto-stops here so a forgotten session cannot
  grow without bound.
- `width: 640` - output width; height follows the canvas aspect ratio, rounded
  down to even.
- `frameRate: 12` - playback rate, written into each GIF frame's delay and into
  the MP4 track metadata.
- `format: "gif"` - the default container.

For GIF, each frame gets its own palette rather than sharing a global one. That
costs bytes, but a run's color range shifts hard as gas drains and stars
ignite, and a single fixed palette visibly bands the late frames. MP4 has no
equivalent problem, which is most of why it looks better.

## Tests

`e2e/record.spec.ts` parses the downloaded bytes rather than trusting them. For
GIF it checks the `GIF89a` signature, reads the logical screen dimensions,
walks the block structure to count image descriptors, and asserts the `0x3b`
trailer is present so a truncated stream cannot pass. For MP4 it walks the
top-level ISO-BMFF box tree and requires both `ftyp` and `moov`, since `moov`
is what proves `finalize()` ran - a capture killed mid-stream still has samples
but no index, and a plain size check would let it through. It covers the
disabled state before generate, a stepped capture in each format, the pill
interlock during recording, and a live run.

## See also

- [FEATURES.md](FEATURES.md) - inventory of what ships.
- [perf-rewrite.md](perf-rewrite.md) - where the frame budget went.

# GIF recording

Capture a run as an animated GIF from the browser. No checkout, no ffmpeg,
no dev server - the whole path runs client-side on the public site.

## Using it

1. Generate a galaxy.
2. Press **record gif**. The button switches to `stop recording (n/240)` and
   counts frames as they are banked.
3. Play the run, step it, or leave it running. The recorder captures whatever
   happens next.
4. Press **stop recording**. The GIF encodes and downloads.

The file is named for the permalink that reproduces the run, so
`galaxy-3885981479949904436-irregular-spiral-500.gif` came from
`?seed=3885981479949904436&scenario=irregular-spiral&size=500`. Someone
handed the GIF can still reach the exact galaxy that produced it.

Recording captures the canvas only - the galaxy, not the stats sidebar. For a
maintainer asset that includes the sidebar, `ward exec capture-readme` remains
the headless path and is unaffected by any of this.

## How it works

The recorder subscribes to `dataviz.setFrameListener`, which fires at the end
of `updateData` - the one funnel every render path already goes through, for
the live worker loop, single steps, and the initial draw alike. That matters:
sampling the canvas on an independent timer would race the renderer and yield
duplicated or half-drawn frames. Subscribing to the funnel means a captured
frame is always a completed draw paired with the sim tick that produced it.

Two costs shape the rest of the design.

**Frame budget.** At size 500 a mature run already spends its whole budget
drawing, around 54ms median per frame. So the capture path does the least
possible work inline: one `drawImage` to downsample the canvas to the target
width, one `getImageData` at that reduced size, and the result goes on a
queue. Quantization and GIF encoding happen off that path, drained through
`requestIdleCallback` with a 100ms timeout fallback.

**Memory.** Raw frames are far too big to hold. A 960x540 RGBA frame is about
2MB, so a 200-frame run would buffer 400MB. Downsampling before queueing, and
encoding queued frames into the GIF byte stream rather than holding them,
keeps the working set to the compressed output plus at most a few pending
frames.

Those two pull against each other, and the queue is where they meet. Past 8
pending frames the recorder encodes inline instead of queueing. That is
deliberate backpressure: it trades a visible hitch for a bound on memory, and
it never silently drops a frame. A GIF missing arbitrary frames is worse than
one that stuttered while recording.

## Tuning

Defaults live in `DEFAULT_OPTIONS` in `src/js/lib/gif-recorder.ts`:

- `ticksPerFrame: 10` - the sim advances far faster than a watchable GIF, so
  frames are decimated by sim tick rather than taken one per draw.
- `maxFrames: 240` - recording auto-stops here so a forgotten session cannot
  grow without bound.
- `width: 640` - output width; height follows the canvas aspect ratio.
- `frameRate: 12` - playback rate written into each frame's delay.

Each frame gets its own palette rather than sharing a global one. That costs
bytes, but a run's color range shifts hard as gas drains and stars ignite, and
a single fixed palette visibly bands the late frames.

## Tests

`e2e/gif-record.spec.ts` parses the downloaded bytes rather than trusting
them: it checks the `GIF89a` signature, reads the logical screen dimensions,
walks the block structure to count image descriptors, and asserts the `0x3b`
trailer is present so a truncated stream cannot pass. It covers the disabled
state before generate, a stepped capture, and a live run.

## See also

- [FEATURES.md](FEATURES.md) - inventory of what ships.
- [perf-rewrite.md](perf-rewrite.md) - where the frame budget went.

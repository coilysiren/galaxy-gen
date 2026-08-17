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
clean while recording. For a maintainer asset, `just capture-readme`
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

- [recording-internals.md](recording-internals.md) - funnel, backpressure, tuning.
- [FEATURES.md](FEATURES.md) - inventory of what ships.
- [perf-rewrite.md](perf-rewrite.md) - where the frame budget went.

# Visual capture harness

`e2e/visual-capture.spec.ts` captures the same galaxy at the same tick so two
builds can be compared frame to frame, which is the only way to judge a change
whose whole point is how it looks.

It is not an assertion suite. It writes an artifact for a human to look at, and
skips unless `GALAXY_CAPTURE` is set, so it never slows an ordinary run.

```bash
GALAXY_CAPTURE=1 npx playwright test e2e/visual-capture.spec.ts --project=chromium
```

Then compare two runs with ImageMagick, cropping the control panel out:

```bash
magick shot.png -crop 980x720+300+0 +repage crop.png
magick compare -metric RMSE before.png after.png null:
magick crop.png -format 'mean=%[fx:mean*100]' info:
```

## Why it exists

Built for galaxy-gen#72, where it settled the question: dropping the dim field
stars outright moved frame brightness by 1.5%, so the diffuse-light machinery
two competing designs both assumed was necessary turned out not to be needed at
all. Kept because galaxy-gen#70 will change the look considerably more than
that and deserves the same treatment.

## Two traps it encodes

The probe advances on the main thread, so the capture lands on an exact tick
rather than wherever the worker happens to have reached.

It then paints the advanced state explicitly. The app's render loop follows the
worker, which is still at tick 0 while the probe advances the main-thread
frontend, so waiting would photograph a fresh galaxy. That mistake produced
three plausible and useless shots the first time this ran.

`WARP` is set far enough out that the field-star population dominates the
frame, which is when treatments of it diverge.

## See also

- [recording.md](recording.md) - the shipped in-browser GIF and MP4 capture.
- [ablation.md](ablation.md) - the numeric counterpart to this visual one.

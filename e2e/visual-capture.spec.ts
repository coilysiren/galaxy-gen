import { test, expect, Page } from "@playwright/test";

// Before-and-after visual harness. Captures the same galaxy at the same
// tick so two builds can be compared frame to frame, which is the only way
// to judge a change whose whole point is how it looks.
//
// Built for galaxy-gen#72, where it settled the question: dropping the dim
// field stars outright moved frame brightness by 1.5%, so the diffuse-light
// machinery two competing designs both assumed was necessary turned out not
// to be needed at all. Kept because galaxy-gen#70 will change the look
// considerably more than that and deserves the same treatment.
//
// Not an assertion suite - it writes an artifact for a human to look at.
// Skipped unless GALAXY_CAPTURE is set, so it never slows an ordinary run.
//
//   GALAXY_CAPTURE=1 npx playwright test e2e/visual-capture.spec.ts --project=chromium
//
// Then compare two runs with ImageMagick, cropping the control panel out:
//
//   magick shot.png -crop 980x720+300+0 +repage crop.png
//   magick compare -metric RMSE before.png after.png null:
//   magick crop.png -format 'mean=%[fx:mean*100]' info:

const FIXED_SEED = 424242;
const SIZE = 500;
// Far enough that the field-star population dominates the frame, which is
// when treatments of it diverge.
const WARP = 1500;

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

test.describe("visual capture", () => {
  test.skip(!process.env.GALAXY_CAPTURE, "set GALAXY_CAPTURE=1 to capture");

  test(`capture — size=${SIZE} t=${WARP}`, async ({ page }, testInfo) => {
    test.setTimeout(600_000);
    await page.goto(`/?seed=${FIXED_SEED}&size=${SIZE}`);
    await waitForWasm(page);
    await page.getByTestId("input-galaxy-size").fill(String(SIZE));
    await page.getByTestId("btn-init").click();

    // Advance on the main thread so the capture lands on an exact tick
    // rather than wherever the worker happens to have reached.
    const state = await page.evaluate(
      async ([n]) => {
        const fe: any = (window as any).__galaxyGen.frontend;
        for (let i = 0; i < n; i++) {
          fe.tick(0.5);
          if (i % 25 === 0) await new Promise((r) => setTimeout(r, 0));
        }
        return { tick: fe.tickCount(), stars: fe.starCount() };
      },
      [WARP]
    );

    // Paint the advanced state explicitly. The app's render loop follows
    // the worker, which is still at tick 0 while this probe advances the
    // main-thread frontend, so waiting would photograph a fresh galaxy -
    // a mistake that produced three plausible and useless shots the first
    // time this ran.
    await page.evaluate(() => {
      const g: any = (window as any).__galaxyGen;
      g.dataviz.updateData(g.frontend, g.frontend.tickCount());
    });
    await page.waitForTimeout(300);

    const shot = testInfo.outputPath("galaxy.png");
    await page.locator("#dataviz canvas").first().screenshot({ path: shot });
    await testInfo.attach("galaxy", { path: shot, contentType: "image/png" });
    console.log(`CAPTURE tick=${state.tick} stars=${state.stars} -> ${shot}`);
  });
});

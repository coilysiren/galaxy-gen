import { test, expect, Page } from "@playwright/test";

// Before-and-after visual harness, skipped unless GALAXY_CAPTURE is set.
// Run it and read the results per docs/visual-capture.md.

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

    // Paint the advanced state explicitly; waiting would photograph a fresh
    // galaxy. See docs/visual-capture.md.
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

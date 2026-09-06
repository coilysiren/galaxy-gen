import { test, expect, Page } from "@playwright/test";

// Before-and-after visual harness, skipped unless GALAXY_CAPTURE is set.
// Run it and read the results per docs/visual-capture.md.

const FIXED_SEED = 424242;
const SIZE = 500;
// Far enough that the field-star population dominates the frame, which is
// when treatments of it diverge.
const WARP = Number(process.env.GALAXY_CAPTURE_WARP ?? 1500);
// The scenario rides the URL rather than the select, because init reads it
// once and a later select change would not rebuild the seeded galaxy.
const SCENARIO = process.env.GALAXY_CAPTURE_SCENARIO ?? "";
const LABEL = process.env.GALAXY_CAPTURE_LABEL ?? "galaxy";

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

test.describe("visual capture", () => {
  test.skip(!process.env.GALAXY_CAPTURE, "set GALAXY_CAPTURE=1 to capture");

  test(`capture — size=${SIZE} t=${WARP}`, async ({ page }, testInfo) => {
    test.setTimeout(600_000);
    const scenario = SCENARIO ? `&scenario=${SCENARIO}` : "";
    await page.goto(`/?seed=${FIXED_SEED}&size=${SIZE}${scenario}`);
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

    const shot = testInfo.outputPath(`${LABEL}.png`);
    await page.locator("#dataviz canvas").first().screenshot({ path: shot });
    await testInfo.attach(LABEL, { path: shot, contentType: "image/png" });
    console.log(`CAPTURE tick=${state.tick} stars=${state.stars} -> ${shot}`);
  });
});

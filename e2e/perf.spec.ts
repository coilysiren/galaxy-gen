import { test, expect, Page } from "@playwright/test";

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute(
    "data-wasm-ready",
    "true",
    { timeout: 30_000 }
  );
}

/**
 * Browser-side perf probe. Not a pass/fail gate — just logs numbers so we
 * can see how the Rust→WASM pipeline performs in a real chromium at
 * several galaxy sizes. Run with `npx playwright test e2e/perf.spec.ts`.
 *
 * Covers both the tick cost (Rust/WASM) and the tick+render cost
 * (including the canvas path). Diverging numbers point at render being
 * the bottleneck, not physics.
 */
test.describe("perf bench", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await waitForWasm(page);
  });

  for (const size of [20, 50, 75, 100, 150, 250]) {
    test(`tick timings — size=${size}`, async ({ page }) => {
      await page.getByTestId("input-galaxy-size").fill(String(size));
      await page.getByTestId("btn-init").click();
      await page.getByTestId("btn-seed").click();

      const result = await page.evaluate((iters) => {
        const fe: any = (window as any).__galaxyGen.frontend;
        // warmup
        fe.tick(0.01);
        const samples: number[] = [];
        for (let i = 0; i < iters; i++) {
          const t0 = performance.now();
          fe.tick(0.01);
          samples.push(performance.now() - t0);
        }
        samples.sort((a, b) => a - b);
        const mean = samples.reduce((a, b) => a + b, 0) / samples.length;
        const median = samples[Math.floor(samples.length / 2)];
        return { mean, median, min: samples[0], max: samples[samples.length - 1] };
      }, size <= 75 ? 20 : size <= 150 ? 5 : 3);

      console.log(
        `TICK  size=${size.toString().padStart(3)}  ` +
          `mean=${result.mean.toFixed(2).padStart(8)}ms  ` +
          `median=${result.median.toFixed(2).padStart(8)}ms`
      );

      // Now measure tick + canvas render combined — this is what the
      // `run` loop actually does per frame.
      const render = await page.evaluate((iters) => {
        const fe: any = (window as any).__galaxyGen.frontend;
        // Pull the exported updater off the module. We read it via the
        // live `dataviz` module that `application.tsx` imports — expose
        // it via a well-known symbol for measurement.
        const dataviz: any = (window as any).__galaxyGen.dataviz;
        const samples: number[] = [];
        for (let i = 0; i < iters; i++) {
          const t0 = performance.now();
          fe.tick(0.5);
          if (dataviz && dataviz.updateData) dataviz.updateData(fe);
          samples.push(performance.now() - t0);
        }
        samples.sort((a, b) => a - b);
        const mean = samples.reduce((a, b) => a + b, 0) / samples.length;
        const median = samples[Math.floor(samples.length / 2)];
        return { mean, median };
      }, size <= 75 ? 20 : size <= 150 ? 5 : 3);

      console.log(
        `FRAME size=${size.toString().padStart(3)}  ` +
          `mean=${render.mean.toFixed(2).padStart(8)}ms  ` +
          `median=${render.median.toFixed(2).padStart(8)}ms`
      );
      expect(result.mean).toBeGreaterThan(0);
    });
  }
});

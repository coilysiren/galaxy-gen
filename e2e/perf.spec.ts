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
 */
test.describe("perf bench", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await waitForWasm(page);
  });

  for (const size of [20, 50, 75, 100]) {
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
      }, size <= 50 ? 20 : 5);

      console.log(
        `size=${size.toString().padStart(3)}  ` +
          `mean=${result.mean.toFixed(2).padStart(8)}ms  ` +
          `median=${result.median.toFixed(2).padStart(8)}ms  ` +
          `min=${result.min.toFixed(2).padStart(8)}ms  ` +
          `max=${result.max.toFixed(2).padStart(8)}ms`
      );
      // Don't fail on timing; just record.
      expect(result.mean).toBeGreaterThan(0);
    });
  }
});

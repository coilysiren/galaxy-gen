import { test, expect, Page } from "@playwright/test";

// Render-only probe. `perf.spec.ts` times tick+render together, which was
// the right shape when physics ran on the main thread. Physics now lives
// in the worker, so the number that decides whether the UI stutters is
// the canvas frame on its own.
//
// Warp first: an unwarped galaxy has no stars, no transients and no
// metallicity, so it renders a fraction of the real workload.

const WARP_TICKS = 300;
const FIXED_SEED = 424242;

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

test.describe("render perf", () => {
  for (const size of [250, 500]) {
    test(`canvas frame — size=${size}`, async ({ page }) => {
      test.setTimeout(600_000);
      // Pin the seed: the first generate honours a URL seed, so successive
      // runs profile the same galaxy and the numbers are comparable.
      await page.goto(`/?seed=${FIXED_SEED}&size=${size}`);
      await waitForWasm(page);
      await page.getByTestId("input-galaxy-size").fill(String(size));
      await page.getByTestId("btn-init").click();

      // Advance on the main thread so the probe controls exactly how far
      // the sim has run before the render is timed.
      const tick = await page.evaluate(
        async ([n]) => {
          const fe: any = (window as any).__galaxyGen.frontend;
          for (let i = 0; i < n; i++) {
            fe.tick(0.5);
            if (i % 25 === 0) await new Promise((r) => setTimeout(r, 0));
          }
          return { tick: fe.tickCount(), stars: fe.starCount() };
        },
        [WARP_TICKS]
      );

      const render = await page.evaluate(() => {
        const fe: any = (window as any).__galaxyGen.frontend;
        const dataviz: any = (window as any).__galaxyGen.dataviz;
        const samples: number[] = [];
        // Pass timings are averaged over the same frames as the totals -
        // a single frame's breakdown swings too much to compare runs.
        const totals: Record<string, number> = {};
        for (let i = 0; i < 12; i++) {
          const t0 = performance.now();
          dataviz.updateData(fe, fe.tickCount() + i);
          samples.push(performance.now() - t0);
          for (const [k, v] of Object.entries(
            dataviz.lastFrameTimings() as Record<string, number>
          )) {
            totals[k] = (totals[k] ?? 0) + v / 12;
          }
        }
        samples.sort((a, b) => a - b);
        return {
          median: samples[Math.floor(samples.length / 2)],
          min: samples[0],
          max: samples[samples.length - 1],
          passes: totals,
        };
      });

      console.log(
        `RENDER size=${String(size).padStart(3)} tick=${tick.tick} stars=${tick.stars}  ` +
          `median=${render.median.toFixed(1)}ms  min=${render.min.toFixed(1)}ms  ` +
          `max=${render.max.toFixed(1)}ms`
      );
      const passes = Object.entries(render.passes as Record<string, number>).sort(
        (a, b) => b[1] - a[1]
      );
      for (const [name, ms] of passes) {
        console.log(`   ${name.padEnd(16)} ${ms.toFixed(2).padStart(7)}ms`);
      }
      expect(render.median).toBeGreaterThan(0);
    });
  }
});

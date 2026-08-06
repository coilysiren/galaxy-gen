import { test, expect, Page } from "@playwright/test";

// Live run probe: the numbers a viewer actually experiences. The other
// perf specs time a single frame in isolation, which answers "how much
// work is a frame" but not "does the run stutter" - stutter is the tail
// of the frame-interval distribution while physics, rendering and the
// React panel all compete for the main thread.

const FIXED_SEED = 424242;
const WARP_TICKS = 400;
const SAMPLE_MS = 6000;

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

test.describe("runtime perf", () => {
  for (const size of [250, 500]) {
    test(`live run — size=${size}`, async ({ page }) => {
      test.setTimeout(600_000);
      await page.goto(`/?seed=${FIXED_SEED}&size=${size}`);
      await waitForWasm(page);
      await page.getByTestId("input-galaxy-size").fill(String(size));
      await page.getByTestId("btn-init").click();

      await page.evaluate(async (n) => {
        const fe: any = (window as any).__galaxyGen.frontend;
        for (let i = 0; i < n; i++) {
          fe.tick(0.5);
          if (i % 25 === 0) await new Promise((r) => setTimeout(r, 0));
        }
      }, WARP_TICKS);

      await page.getByTestId("btn-run").click();

      const stats = await page.evaluate(async (sampleMs) => {
        // Let the worker fill its pipeline before sampling.
        await new Promise((r) => setTimeout(r, 750));
        const deltas: number[] = [];
        const startTicks = Number(
          document.querySelector('[data-testid="stat-ticks"]')?.textContent ?? "0"
        );
        await new Promise<void>((resolve) => {
          let last = performance.now();
          const t0 = last;
          const step = () => {
            const now = performance.now();
            deltas.push(now - last);
            last = now;
            if (now - t0 >= sampleMs) resolve();
            else requestAnimationFrame(step);
          };
          requestAnimationFrame(step);
        });
        const endTicks = Number(
          document.querySelector('[data-testid="stat-ticks"]')?.textContent ?? "0"
        );
        const sorted = [...deltas].sort((a, b) => a - b);
        const at = (q: number) =>
          sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))];
        return {
          frames: deltas.length,
          fps: (deltas.length * 1000) / sampleMs,
          p50: at(0.5),
          p95: at(0.95),
          p99: at(0.99),
          worst: sorted[sorted.length - 1],
          // Anything past two 60Hz frames reads as a hitch.
          jank: deltas.filter((d) => d > 34).length,
          simTicksPerSec: ((endTicks - startTicks) * 1000) / sampleMs,
        };
      }, SAMPLE_MS);

      console.log(
        `LIVE size=${String(size).padStart(3)}  fps=${stats.fps.toFixed(1)}  ` +
          `sim=${stats.simTicksPerSec.toFixed(1)}tick/s  ` +
          `p50=${stats.p50.toFixed(1)}ms p95=${stats.p95.toFixed(1)}ms ` +
          `p99=${stats.p99.toFixed(1)}ms worst=${stats.worst.toFixed(1)}ms  ` +
          `jank=${stats.jank}/${stats.frames}`
      );
      expect(stats.frames).toBeGreaterThan(0);
    });
  }
});

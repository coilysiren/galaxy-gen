import { test, expect, Page } from "@playwright/test";

// Small grid: these assert on the backdrop, not the simulation, and a
// small galaxy leaves more of the frame as sky to sample.
const SIZE = 60;

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

async function generate(page: Page, seed: string) {
  // lock=1 pins the seed so `generate` cannot roll a fresh one, which
  // would make the determinism comparisons meaningless.
  await page.goto(`/?seed=${seed}&size=${SIZE}&scenario=irregular-spiral&lock=1`);
  await waitForWasm(page);
  await page.getByTestId("btn-init").click();
  await expect(page.locator("#dataviz canvas")).toBeVisible();
}

/// Sample the four corners - the only regions that are sky and nothing
/// else, since the disk fits the short edge and the halo reaches past.
async function skyPixels(page: Page): Promise<number[]> {
  return page.evaluate(() => {
    const canvas = document.querySelector("#dataviz canvas") as HTMLCanvasElement;
    const ctx = canvas.getContext("2d")!;
    const bw = 140;
    const bh = 110;
    const out: number[] = [];
    for (const [x, y] of [
      [0, 0],
      [canvas.width - bw, 0],
      [0, canvas.height - bh],
      [canvas.width - bw, canvas.height - bh],
    ]) {
      out.push(...Array.from(ctx.getImageData(x, y, bw, bh).data));
    }
    return out;
  });
}

/// Count pixels brighter than the space-black base. Stars are faint
/// enough that their contribution to a mean is swamped by the base.
function litPixels(pixels: number[]): number {
  let lit = 0;
  for (let i = 0; i < pixels.length; i += 4) {
    if (pixels[i] > 0x05 + 2 || pixels[i + 1] > 0x06 + 2 || pixels[i + 2] > 0x0a + 2) lit += 1;
  }
  return lit;
}

test.describe("seeded backdrop", () => {
  test("draws a sky brighter than bare space-black", async ({ page }) => {
    await generate(page, "12345");
    const pixels = await skyPixels(page);
    const total = pixels.length / 4;
    const lit = litPixels(pixels);

    // Something is drawn out there.
    expect(lit).toBeGreaterThan(total * 0.01);
    // But it stays scenery: a backdrop lighting up half the corners
    // would be competing with the galaxy.
    expect(lit).toBeLessThan(total * 0.5);
  });

  test("the same seed reproduces the same sky", async ({ page }) => {
    await generate(page, "777777");
    const first = await skyPixels(page);
    await generate(page, "777777");
    const second = await skyPixels(page);
    expect(second).toEqual(first);
  });

  test("a different seed gives a different sky", async ({ page }) => {
    await generate(page, "111111");
    const first = await skyPixels(page);
    await generate(page, "222222");
    const second = await skyPixels(page);
    expect(second).not.toEqual(first);
  });

  test("the sky is static across ticks and does not rotate with the disk", async ({ page }) => {
    await generate(page, "424242");
    const before = await skyPixels(page);

    // Past the point where the co-rotating frame has turned the world:
    // a backdrop inside that transform would smear along the rotation.
    for (let i = 0; i < 30; i++) await page.getByTestId("btn-tick").click();

    const after = await skyPixels(page);
    expect(after).toEqual(before);
  });

  test("backdrop generation is cached, not repeated per frame", async ({ page }) => {
    await generate(page, "31337");
    // First draw builds it; every later frame should only blit.
    for (let i = 0; i < 5; i++) await page.getByTestId("btn-tick").click();
    const ms = await page.evaluate(() => {
      const timings = (window as any).__galaxyGen?.dataviz?.lastFrameTimings?.();
      return timings?.background ?? -1;
    });
    expect(ms).toBeGreaterThanOrEqual(0);
    // A rebuild stamps hundreds of sprites and costs milliseconds; a
    // cached blit is a fraction of one.
    expect(ms).toBeLessThan(1.5);
  });
});

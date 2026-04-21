import { test, expect, Page } from "@playwright/test";

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

test.describe("Galaxy Generator", () => {
  test.beforeEach(async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("pageerror", (err) => consoleErrors.push(err.message));
    page.on("console", (msg) => {
      if (msg.type() === "error") consoleErrors.push(msg.text());
    });
    (page as any).__consoleErrors = consoleErrors;

    await page.goto("/");
    await waitForWasm(page);
  });

  test.afterEach(async ({ page }) => {
    const errors: string[] = (page as any).__consoleErrors ?? [];
    expect(errors, `unexpected page errors: ${errors.join("\n")}`).toEqual([]);
  });

  test("renders the UI shell with controls", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "Galaxy Generator" })).toBeVisible();
    await expect(page.getByTestId("input-galaxy-size")).toHaveValue("50");
    await expect(page.getByTestId("input-seed-mass")).toHaveValue("25");
    await expect(page.getByTestId("input-time-modifier")).toHaveValue("0.01");
    await expect(page.getByTestId("btn-init")).toBeVisible();
    await expect(page.getByTestId("btn-seed")).toBeVisible();
    await expect(page.getByTestId("btn-tick")).toBeVisible();
  });

  test("init creates an svg inside the dataviz container", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await expect(page.locator("#dataviz svg")).toBeVisible();
  });

  test("seed populates cells and draws circles", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();

    const circles = page.locator("#dataviz svg #data circle");
    await expect(circles.first()).toBeAttached();
    const count = await circles.count();
    expect(count).toBeGreaterThan(0);

    const cellCount = await page.evaluate(() => {
      const frontend = (window as any).__galaxyGen?.frontend;
      return frontend ? frontend.cells().length : 0;
    });
    expect(cellCount).toBe(50 * 50);
  });

  test("tick advances the simulation without errors", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();

    const before = await page.locator("#dataviz svg #data circle").count();
    await page.getByTestId("btn-tick").click();
    await page.getByTestId("btn-tick").click();
    const after = await page.locator("#dataviz svg #data circle").count();

    expect(after).toBe(before);
    expect(after).toBeGreaterThan(0);
  });

  test("ticks actually redistribute mass (sim is not frozen)", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();

    const snapshotBefore = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    // Run enough ticks for the mass field to noticeably redistribute.
    await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      for (let i = 0; i < 120; i++) fe.tick(0.5);
    });

    const snapshotAfter = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    let changed = 0;
    for (let i = 0; i < snapshotBefore.length; i++) {
      if (snapshotBefore[i] !== snapshotAfter[i]) changed++;
    }
    // Anything less than ~5% changed means the sim is frozen.
    const pctChanged = changed / snapshotBefore.length;
    expect(pctChanged).toBeGreaterThan(0.05);
  });

  test("changing galaxy size changes cell count after re-init", async ({ page }) => {
    const sizeInput = page.getByTestId("input-galaxy-size");
    await sizeInput.fill("20");
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();

    const cellCount = await page.evaluate(() => {
      const frontend = (window as any).__galaxyGen?.frontend;
      return frontend ? frontend.cells().length : 0;
    });
    expect(cellCount).toBe(20 * 20);
  });
});

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
    await expect(page.getByTestId("input-time-modifier")).toHaveValue("0.5");
    await expect(page.getByTestId("btn-init")).toBeVisible();
    await expect(page.getByTestId("btn-seed")).toBeVisible();
    await expect(page.getByTestId("btn-tick")).toBeVisible();
  });

  test("init creates a canvas inside the dataviz container", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await expect(page.locator("#dataviz canvas")).toBeVisible();
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

  test("keyboard shortcut: Space toggles run/pause", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    const runBtn = page.getByTestId("btn-run");
    await expect(runBtn).toHaveText("run");

    await page.locator("body").press("Space");
    await expect(runBtn).toHaveText("pause");

    await page.locator("body").press("Space");
    await expect(runBtn).toHaveText("run");
  });

  test("keyboard shortcut: Space does nothing before galaxy is initialised", async ({ page }) => {
    // Run button is disabled; Space should be a no-op and must not throw.
    await page.locator("body").press("Space");
    await expect(page.getByTestId("btn-run")).toBeDisabled();
  });

  test("keyboard shortcut: ArrowUp raises dt", async ({ page }) => {
    const dtStat = page.getByTestId("stat-dt");
    await expect(dtStat).toHaveText("dt: 0.500");

    await page.locator("body").press("ArrowUp");
    // 0.5 * 1.25 = 0.625
    await expect(dtStat).toHaveText("dt: 0.625");

    await page.locator("body").press("ArrowUp");
    await page.locator("body").press("ArrowUp");
    const text = (await dtStat.textContent()) ?? "";
    const dt = parseFloat(text.replace(/[^\d.]/g, ""));
    // 0.5 * 1.25^3 ≈ 0.977
    expect(dt).toBeCloseTo(0.977, 2);
  });

  test("keyboard shortcut: ArrowDown lowers dt", async ({ page }) => {
    const dtStat = page.getByTestId("stat-dt");
    await expect(dtStat).toHaveText("dt: 0.500");

    await page.locator("body").press("ArrowDown");
    // 0.5 / 1.25 = 0.4
    await expect(dtStat).toHaveText("dt: 0.400");

    await page.locator("body").press("ArrowDown");
    const text = (await dtStat.textContent()) ?? "";
    const dt = parseFloat(text.replace(/[^\d.]/g, ""));
    // 0.5 / 1.25^2 = 0.32
    expect(dt).toBeCloseTo(0.32, 2);
  });

  test("keyboard shortcut: dt is clamped to [0.01, 10]", async ({ page }) => {
    const dtStat = page.getByTestId("stat-dt");

    // Hammer ArrowUp a lot; dt should cap at 10.
    for (let i = 0; i < 40; i++) {
      await page.locator("body").press("ArrowUp");
    }
    const hiText = (await dtStat.textContent()) ?? "";
    const hi = parseFloat(hiText.replace(/[^\d.]/g, ""));
    expect(hi).toBeLessThanOrEqual(10);
    expect(hi).toBeGreaterThan(9);

    // Reset to default, then hammer ArrowDown; dt should floor at 0.01.
    await page.locator("body").press("r");
    for (let i = 0; i < 60; i++) {
      await page.locator("body").press("ArrowDown");
    }
    const loText = (await dtStat.textContent()) ?? "";
    const lo = parseFloat(loText.replace(/[^\d.]/g, ""));
    expect(lo).toBeGreaterThanOrEqual(0.01);
    expect(lo).toBeLessThan(0.02);
  });

  test("keyboard shortcut: R resets dt to default", async ({ page }) => {
    const dtStat = page.getByTestId("stat-dt");

    await page.locator("body").press("ArrowUp");
    await page.locator("body").press("ArrowUp");
    await expect(dtStat).not.toHaveText("dt: 0.500");

    await page.locator("body").press("r");
    await expect(dtStat).toHaveText("dt: 0.500");

    // Uppercase variant also works (caps lock, etc.).
    await page.locator("body").press("ArrowUp");
    await page.locator("body").press("R");
    await expect(dtStat).toHaveText("dt: 0.500");
  });

  test("keyboard shortcut: R does not affect the tick counter", async ({ page }) => {
    // R only scopes to dt; it must not secretly reset sim state.
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();
    await page.getByTestId("btn-tick").click();
    await page.getByTestId("btn-tick").click();

    const ticksStat = page.getByTestId("stat-ticks");
    await expect(ticksStat).toHaveText("ticks: 2");

    await page.locator("body").press("r");
    await expect(ticksStat).toHaveText("ticks: 2");
  });

  test("keyboard shortcuts are ignored while typing in an input", async ({ page }) => {
    const sizeInput = page.getByTestId("input-galaxy-size");
    const dtStat = page.getByTestId("stat-dt");

    await sizeInput.click();

    // Neither arrow keys nor 'r' nor Space should trigger shortcuts while the
    // cursor is inside an input — otherwise typing "0.5" into dt would
    // inadvertently pause the sim on the space bar, etc.
    await sizeInput.press("ArrowUp");
    await sizeInput.press("ArrowDown");
    await sizeInput.press("r");
    await sizeInput.press("Space");
    await expect(dtStat).toHaveText("dt: 0.500");
  });

  test("keyboard shortcuts are ignored when a modifier key is held", async ({ page }) => {
    // Cmd/Ctrl/Alt chords should be left to the browser / OS, never to the sim.
    const dtStat = page.getByTestId("stat-dt");
    await page.locator("body").press("Meta+ArrowUp");
    await page.locator("body").press("Control+ArrowUp");
    await page.locator("body").press("Alt+ArrowUp");
    await expect(dtStat).toHaveText("dt: 0.500");
  });

  test("keyboard hints row is visible to users", async ({ page }) => {
    const hints = page.getByTestId("keyboard-hints");
    await expect(hints).toBeVisible();
    await expect(hints).toContainText("space");
    await expect(hints).toContainText("reset");
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

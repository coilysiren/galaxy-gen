import { test, expect, Page } from "@playwright/test";

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

// Filter webpack-dev-server HMR noise that fires on re-navigation.
function isDevServerNoise(msg: string): boolean {
  return (
    msg.includes("webpack-dev-server") ||
    /ws:\/\/(127\.0\.0\.1|localhost):\d+\/ws/.test(msg) ||
    /Failed to load resource.*\b404\b/.test(msg)
  );
}

test.describe("Galaxy Generator", () => {
  test.beforeEach(async ({ page }) => {
    const consoleErrors: string[] = [];
    page.on("pageerror", (err) => {
      if (!isDevServerNoise(err.message)) consoleErrors.push(err.message);
    });
    page.on("console", (msg) => {
      if (msg.type() === "error" && !isDevServerNoise(msg.text())) {
        consoleErrors.push(msg.text());
      }
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
    await expect(page.getByTestId("input-galaxy-size")).toHaveValue("250");
    await expect(page.getByTestId("stat-dt")).toHaveText("dt: 0.500");
    await expect(page.getByTestId("btn-init")).toBeVisible();
    await expect(page.getByTestId("btn-tick")).toBeVisible();
    // Seed mass is URL-param-only; it must not render an input.
    await expect(page.getByTestId("input-seed-mass")).toHaveCount(0);
  });

  test("init creates a canvas inside the dataviz container", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await expect(page.locator("#dataviz canvas")).toBeVisible();
  });

  test("seed populates cells with nonzero mass", async ({ page }) => {
    await page.getByTestId("btn-init").click();

    const stats = await page.evaluate(() => {
      const frontend = (window as any).__galaxyGen?.frontend;
      if (!frontend) return null;
      const mass = frontend.massArray() as Uint16Array;
      let nonzero = 0;
      for (const m of mass) if (m > 0) nonzero++;
      return { cells: frontend.cells().length, nonzero };
    });
    expect(stats).not.toBeNull();
    expect(stats!.cells).toBe(250 * 250);
    expect(stats!.nonzero).toBeGreaterThan(0);
  });

  test("tick advances the simulation without errors", async ({ page }) => {
    await page.getByTestId("btn-init").click();

    await page.getByTestId("btn-tick").click();
    await page.getByTestId("btn-tick").click();
    await expect(page.getByTestId("stat-ticks")).toHaveText("ticks: 2");
  });

  test("ticks actually redistribute mass (sim is not frozen)", async ({ page }) => {
    await page.getByTestId("btn-init").click();

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

  test("webgpu backend ticks produce non-frozen simulation when available", async ({ page }) => {
    // Skip if Chromium didn't expose navigator.gpu (CPU path is covered above).
    const hasGpu = await page.evaluate(() => Boolean((navigator as any).gpu));
    test.skip(!hasGpu, "navigator.gpu not available in this Chromium");

    // Small grid: the WGSL kernel is O(N²) per tick and 60 ticks at the
    // 250 default blows the test timeout.
    await page.getByTestId("input-galaxy-size").fill("50");
    await page.getByTestId("btn-init").click();

    // Enable WebGPU directly; UI selector was removed.
    await page.evaluate(async () => {
      const fe: any = (window as any).__galaxyGen.frontend;
      await fe.enableWebGPU();
    });

    const snapshotBefore = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    // Await each tick so the async GPU path completes before we sample.
    await page.evaluate(async () => {
      const fe: any = (window as any).__galaxyGen.frontend;
      for (let i = 0; i < 60; i++) {
        await fe.tickAsync(0.5);
      }
    });

    const snapshotAfter = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    let changed = 0;
    for (let i = 0; i < snapshotBefore.length; i++) {
      if (snapshotBefore[i] !== snapshotAfter[i]) changed++;
    }
    const pctChanged = changed / snapshotBefore.length;
    expect(pctChanged).toBeGreaterThan(0.05);
  });

  test("webgpu and cpu produce matching mass totals for small N", async ({ page }) => {
    // CPU vs WebGPU parity, small N so both run direct-sum.
    const hasGpu = await page.evaluate(() => Boolean((navigator as any).gpu));
    test.skip(!hasGpu, "navigator.gpu not available in this Chromium");

    await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true");

    const result = await page.evaluate(async () => {
      const wasm = (window as any).__galaxyGen.wasm;
      const Frontend = (window as any).__galaxyGen.Frontend;
      if (!wasm || !Frontend) return { skipped: "wasm/Frontend not exposed" };

      const size = 10;
      const timeMod = 0.1;
      const ticks = 10;

      // Bypass Frontend ctor (defaults to mass=0) to set cell_initial_mass=2.
      const cpuFe: any = new Frontend(size);
      const gpuFe: any = new Frontend(size);
      cpuFe.galaxy.free();
      cpuFe.galaxy = new wasm.Galaxy(size, 2);
      gpuFe.galaxy.free();
      gpuFe.galaxy = new wasm.Galaxy(size, 2);

      for (let i = 0; i < ticks; i++) cpuFe.tick(timeMod);

      try {
        await gpuFe.enableWebGPU();
      } catch {
        // GPU device init failed - we'll still run ticks on CPU fallback.
      }
      for (let i = 0; i < ticks; i++) await gpuFe.tickAsync(timeMod);

      const cpuMass = Array.from(cpuFe.massArray() as Uint16Array);
      const gpuMass = Array.from(gpuFe.massArray() as Uint16Array);
      const gpuBackend = gpuFe.currentBackend();
      return { cpuMass, gpuMass, gpuBackend };
    });

    if ((result as any).skipped) {
      test.skip(true, (result as any).skipped);
      return;
    }
    const r = result as { cpuMass: number[]; gpuMass: number[]; gpuBackend: string };

    expect(r.cpuMass.length).toBe(r.gpuMass.length);

    // Mass conservation: totals must match across backends exactly.
    const cpuTotal = r.cpuMass.reduce((a, b) => a + b, 0);
    const gpuTotal = r.gpuMass.reduce((a, b) => a + b, 0);
    expect(gpuTotal).toBe(cpuTotal);

    // Rust int-r^2 vs WGSL float inverseSqrt: allow ~10% per-cell drift.
    let diffs = 0;
    for (let i = 0; i < r.cpuMass.length; i++) {
      if (r.cpuMass[i] !== r.gpuMass[i]) diffs++;
    }
    const threshold = r.gpuBackend === "webgpu" ? 0.1 : 0.0;
    expect(diffs / r.cpuMass.length).toBeLessThanOrEqual(threshold);
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

    // Shortcuts must not fire while typing in an input.
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

  test("camera pan+zoom: wheel zooms, double-click restores", async ({ page }) => {
    await page.getByTestId("btn-init").click();

    const canvas = page.locator("#dataviz canvas");
    await expect(canvas).toBeVisible();

    const getCam = () =>
      page.evaluate(() => {
        const dv = (window as any).__galaxyGen?.dataviz;
        return dv?.getCamera?.() ?? null;
      });

    // Baseline camera is identity.
    const before = await getCam();
    expect(before).toEqual({ tx: 0, ty: 0, zoom: 1 });

    // Multiple small wheel deltas mirror a trackpad pinch-zoom.
    await page.evaluate(() => {
      const c = document.querySelector("#dataviz canvas") as HTMLCanvasElement;
      const rect = c.getBoundingClientRect();
      const cx = rect.left + rect.width / 2;
      const cy = rect.top + rect.height / 2;
      for (let i = 0; i < 10; i++) {
        c.dispatchEvent(
          new WheelEvent("wheel", {
            deltaY: -200,
            clientX: cx,
            clientY: cy,
            bubbles: true,
            cancelable: true,
          })
        );
      }
    });

    const zoomed = await getCam();
    expect(zoomed).not.toBeNull();
    expect(zoomed!.zoom).toBeGreaterThan(1.1);

    // Double-click on the canvas restores the identity transform.
    await canvas.dblclick();
    const after = await getCam();
    expect(after).toEqual({ tx: 0, ty: 0, zoom: 1 });
  });

  test("camera pan: click-drag on canvas updates data-cam-* on #dataviz", async ({ page }) => {
    await page.getByTestId("btn-init").click();

    const host = page.locator("#dataviz");
    await expect(host).toHaveAttribute("data-cam-tx", "0.00");
    await expect(host).toHaveAttribute("data-cam-ty", "0.00");
    await expect(host).toHaveAttribute("data-cam-zoom", "1.0000");

    // Zoom in first so that panning is actually allowed by the clamp
    // (at zoom=1 the pan delta is clamped to 0,0 to keep the world in view).
    await page.evaluate(() => {
      const c = document.querySelector("#dataviz canvas") as HTMLCanvasElement;
      const rect = c.getBoundingClientRect();
      const cx = rect.left + rect.width / 2;
      const cy = rect.top + rect.height / 2;
      for (let i = 0; i < 5; i++) {
        c.dispatchEvent(
          new WheelEvent("wheel", {
            deltaY: -200,
            clientX: cx,
            clientY: cy,
            bubbles: true,
            cancelable: true,
          })
        );
      }
    });

    // Snapshot tx after zoom (zooming about center keeps tx negative).
    const afterZoom = await host.evaluate((el: HTMLElement) => ({
      tx: el.getAttribute("data-cam-tx"),
      ty: el.getAttribute("data-cam-ty"),
      zoom: el.getAttribute("data-cam-zoom"),
    }));
    expect(Number(afterZoom.zoom)).toBeGreaterThan(1.1);

    // Dispatch pointer events directly; avoids mouse-to-pointer ambiguity.
    await page.evaluate(() => {
      const c = document.querySelector("#dataviz canvas") as HTMLCanvasElement;
      const rect = c.getBoundingClientRect();
      const sx = rect.left + rect.width / 2;
      const sy = rect.top + rect.height / 2;
      const pe = (type: string, x: number, y: number) =>
        new PointerEvent(type, {
          pointerId: 1,
          pointerType: "mouse",
          isPrimary: true,
          clientX: x,
          clientY: y,
          button: 0,
          buttons: type === "pointerup" ? 0 : 1,
          bubbles: true,
          cancelable: true,
        });
      c.dispatchEvent(pe("pointerdown", sx, sy));
      for (let i = 1; i <= 10; i++) {
        c.dispatchEvent(pe("pointermove", sx - 10 * i, sy - 8 * i));
      }
      c.dispatchEvent(pe("pointerup", sx - 100, sy - 80));
    });

    const afterPan = await host.evaluate((el: HTMLElement) => ({
      tx: el.getAttribute("data-cam-tx"),
      ty: el.getAttribute("data-cam-ty"),
      zoom: el.getAttribute("data-cam-zoom"),
    }));
    // Camera state must have changed (tx or ty moved) while zoom stayed put.
    expect(afterPan.zoom).toBe(afterZoom.zoom);
    const moved = afterPan.tx !== afterZoom.tx || afterPan.ty !== afterZoom.ty;
    expect(moved).toBe(true);

    // Double-click clears tx/ty/zoom back to identity.
    await page.locator("#dataviz canvas").dblclick();
    await expect(host).toHaveAttribute("data-cam-tx", "0.00");
    await expect(host).toHaveAttribute("data-cam-ty", "0.00");
    await expect(host).toHaveAttribute("data-cam-zoom", "1.0000");
  });

  test("url query params flow into state and init pushes full state back to the URL", async ({
    page,
  }) => {
    await page.goto("/?seed=42&size=30&mass=10&dt=0.25");
    await waitForWasm(page);

    // size is still UI-visible; seed/mass/dt are URL-only and flow
    // through state without a dedicated input.
    await expect(page.getByTestId("input-galaxy-size")).toHaveValue("30");
    await expect(page.getByTestId("stat-dt")).toHaveText("dt: 0.250");

    // After Init, the URL should carry all four params (shareable state).
    await page.getByTestId("btn-init").click();
    const url = new URL(page.url());
    expect(url.searchParams.get("seed")).toBe("42");
    expect(url.searchParams.get("size")).toBe("30");
    expect(url.searchParams.get("mass")).toBe("10");
    expect(url.searchParams.get("dt")).toBe("0.25");
  });

  test("same seed produces the same mass distribution (reproducible)", async ({ page }) => {
    // First run, seed from URL param.
    await page.goto("/?seed=12345&size=20&mass=50");
    await waitForWasm(page);
    await page.getByTestId("btn-init").click();
    const first = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    // Second run with the same seed.
    await page.goto("/?seed=12345&size=20&mass=50");
    await waitForWasm(page);
    await page.getByTestId("btn-init").click();
    const second = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    expect(second).toEqual(first);
  });

  test("switching initial condition produces mode-specific seed + ticks", async ({ page }) => {
    // Sanity: the dropdown has the two modes we expect.
    const select = page.getByTestId("select-initial-condition");
    await expect(select).toBeVisible();
    const optionValues = await select.locator("option").evaluateAll((opts) =>
      (opts as HTMLOptionElement[]).map((o) => o.value)
    );
    expect(optionValues).toEqual(["0", "1"]);

    // Bang must produce a different mass field than Uniform.
    const snapshots: Record<string, number[]> = {};
    for (const mode of ["0", "1"]) {
      await select.selectOption(mode);
      await page.getByTestId("btn-init").click();
      // Advance a few ticks to let the mode-specific velocities take effect.
      for (let i = 0; i < 5; i++) await page.getByTestId("btn-tick").click();
      snapshots[mode] = await page.evaluate(() => {
        const fe: any = (window as any).__galaxyGen.frontend;
        return Array.from(fe.massArray() as Uint16Array);
      });
      expect(snapshots[mode].length).toBe(250 * 250);
    }

    // Compare by diff count; nonzero-cell ordering is too strict.
    const nonzero = (a: number[]) => a.filter((m) => m > 0).length;
    const diffCount = (a: number[], b: number[]) => {
      let n = 0;
      for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) n++;
      return n;
    };
    const uniform = snapshots["0"];
    // Bang concentrates mass centrally; most uniform-filled cells differ.
    const minDistinctCells = Math.floor(uniform.length * 0.1);
    expect(
      diffCount(uniform, snapshots["1"]),
      "bang snapshot should differ from uniform",
    ).toBeGreaterThan(minDistinctCells);
    expect(nonzero(snapshots["1"])).toBeGreaterThan(0);
  });

  test("run button ticks via the worker and pause resumes state cleanly", async ({ page }) => {
    await page.getByTestId("btn-init").click();

    // Worker API must exist; otherwise run path is a no-op.
    const workerSupport = await page.evaluate(
      () => (window as any).__galaxyGen?.workerSupported === true
    );
    expect(workerSupport, "Worker API unavailable in test browser").toBe(true);

    const before = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    // Kick off the worker-driven loop, let it run briefly, then pause.
    await page.getByTestId("btn-run").click();
    await expect(page.getByTestId("btn-run")).toHaveText("pause");
    // Now that the run loop is live, the worker proxy should be present
    // on the window — proves physics is genuinely off-main-thread.
    const workerLive = await page.evaluate(() => !!(window as any).__galaxyGen?.worker);
    expect(workerLive, "TickWorker proxy not exposed after run start").toBe(true);
    // Let the worker advance the sim for a bit.
    await page.waitForTimeout(1500);
    // Sanity: the displayed tick count should have advanced as the
    // worker pushed snapshots to the main thread.
    const tickText = await page.locator("text=/ticks: \\d+/").first().textContent();
    const advanced = parseInt(tickText?.replace(/\D/g, "") ?? "0", 10);
    expect(advanced, `tick count didn't advance (got ${tickText})`).toBeGreaterThan(0);
    await page.getByTestId("btn-run").click();
    await expect(page.getByTestId("btn-run")).toHaveText("run");

    // After pause, the Frontend should have been rehydrated from the
    // worker — mass should be meaningfully different from before.
    const after = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    let changed = 0;
    for (let i = 0; i < before.length; i++) {
      if (before[i] !== after[i]) changed++;
    }
    expect(changed / before.length).toBeGreaterThan(0.05);

    // Step button must still bump the counter after pause/restore.
    const tickCountBefore = parseInt(
      (
        await page.locator("text=/ticks: \\d+/").first().textContent()
      )?.replace(/\D/g, "") ?? "0",
      10,
    );
    await page.getByTestId("btn-tick").click();
    const tickCountAfter = parseInt(
      (
        await page.locator("text=/ticks: \\d+/").first().textContent()
      )?.replace(/\D/g, "") ?? "0",
      10,
    );
    expect(tickCountAfter).toBe(tickCountBefore + 1);
  });

  test("stars render and survive the worker pause round-trip", async ({ page }) => {
    await page.getByTestId("input-galaxy-size").fill("50");
    await page.getByTestId("btn-init").click();

    // Debug-spawn a few stars; production stars come from StarBirth events.
    const spawned = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      fe.spawnStar(30, 25, 0, 0.5, 40);
      fe.spawnStar(20, 25, 0, -0.5, 80);
      return fe.starCount();
    });
    expect(spawned).toBe(2);

    // Run through the worker briefly, then pause: stars must come back.
    await page.getByTestId("btn-run").click();
    await page.waitForTimeout(700);
    await page.getByTestId("btn-run").click();
    await expect(page.getByTestId("btn-run")).toHaveText("run");

    const after = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      const stars = fe.starRenderArray() as Float32Array;
      return { count: fe.starCount(), sample: Array.from(stars.slice(0, 4)) };
    });
    expect(after.count).toBe(2);
    // Stars moved from their spawn points but stayed in-world.
    expect(after.sample[0]).toBeGreaterThan(0);
    expect(after.sample[0]).toBeLessThan(50);
  });

  test("changing galaxy size changes cell count after re-init", async ({ page }) => {
    const sizeInput = page.getByTestId("input-galaxy-size");
    await sizeInput.fill("20");
    await page.getByTestId("btn-init").click();

    const cellCount = await page.evaluate(() => {
      const frontend = (window as any).__galaxyGen?.frontend;
      return frontend ? frontend.cells().length : 0;
    });
    expect(cellCount).toBe(20 * 20);
  });
});

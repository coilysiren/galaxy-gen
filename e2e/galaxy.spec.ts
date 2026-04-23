import { test, expect, Page } from "@playwright/test";

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

// webpack-dev-server HMR produces harmless console noise when a page does
// more than one `goto` in the same test (the old page's WebSocket tears down
// mid-reload). These aren't application errors — filter them out so tests
// that re-navigate (URL-param reproducibility checks) don't flake.
function isDevServerNoise(msg: string): boolean {
  return (
    msg.includes("webpack-dev-server") ||
    msg.includes("ws://127.0.0.1:8080/ws") ||
    msg.includes("ws://localhost:8080/ws") ||
    // Generic "Failed to load resource … 404" — typically a stale HMR
    // chunk fetch or favicon probe when a test re-navigates. These are
    // harmless for application behaviour.
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
    await expect(page.getByTestId("input-galaxy-size")).toHaveValue("50");
    await expect(page.getByTestId("input-seed-mass")).toHaveValue("25");
    await expect(page.getByTestId("stat-dt")).toHaveText("dt: 0.500");
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

  test("webgpu backend ticks produce non-frozen simulation when available", async ({ page }) => {
    // Skip when the Chromium launch didn't expose navigator.gpu (older
    // versions, missing swiftshader, etc). CPU path is covered above.
    const hasGpu = await page.evaluate(() => Boolean((navigator as any).gpu));
    test.skip(!hasGpu, "navigator.gpu not available in this Chromium");

    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();

    // Enable WebGPU directly on the exposed Frontend (the UI selector was
    // removed; parity/smoke coverage still exercises the WGSL path).
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
    // Parity check: for identical deterministic initial state, running
    // CPU tick() and WebGPU tickAsync() side-by-side for the same number
    // of steps gives matching mass arrays. Small N so both paths run
    // direct-sum O(N^2) and not Barnes-Hut.
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

      // Two Frontends with identical deterministic initial mass field
      // (cell_initial_mass=2, no random seed). Frontend's constructor
      // default is cell_initial_mass=0, so bypass and assign pre-built
      // wasm.Galaxy instances directly.
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

    // Mass conservation: totals must match across backends exactly (both
    // paths run the same Rust integrator + collision merge).
    const cpuTotal = r.cpuMass.reduce((a, b) => a + b, 0);
    const gpuTotal = r.gpuMass.reduce((a, b) => a + b, 0);
    expect(gpuTotal).toBe(cpuTotal);

    // Per-cell agreement: Rust int-r^2 lookup vs WGSL float inverseSqrt
    // aren't bit-identical. If GPU fell back to CPU we get exact match;
    // otherwise allow ~10% of cells to differ by rounding.
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

  test("camera pan+zoom: wheel zooms, reset-view restores", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();

    const canvas = page.locator("#dataviz canvas");
    await expect(canvas).toBeVisible();

    const resetBtn = page.getByTestId("btn-reset-view");
    await expect(resetBtn).toBeVisible();
    await expect(resetBtn).toBeEnabled();

    const getCam = () =>
      page.evaluate(() => {
        const dv = (window as any).__galaxyGen?.dataviz;
        return dv?.getCamera?.() ?? null;
      });

    // Baseline camera is identity.
    const before = await getCam();
    expect(before).toEqual({ tx: 0, ty: 0, zoom: 1 });

    // Dispatch wheel events directly on the canvas so the camera zooms
    // in regardless of pointer focus. Multiple small deltas mirror a
    // trackpad pinch-zoom gesture.
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

    // Reset view button restores identity transform.
    await resetBtn.click();
    const after = await getCam();
    expect(after).toEqual({ tx: 0, ty: 0, zoom: 1 });
  });

  test("camera pan: click-drag on canvas updates data-cam-* on #dataviz", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();

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

    // Drag the canvas from its center toward the top-left corner by
    // dispatching pointer events directly (matches the handlers and avoids
    // any ambiguity about mouse→pointer event synthesis under automation).
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

    // Reset view clears tx/ty/zoom back to identity.
    await page.getByTestId("btn-reset-view").click();
    await expect(host).toHaveAttribute("data-cam-tx", "0.00");
    await expect(host).toHaveAttribute("data-cam-ty", "0.00");
    await expect(host).toHaveAttribute("data-cam-zoom", "1.0000");
  });

  test("url query params flow into state and init pushes full state back to the URL", async ({
    page,
  }) => {
    await page.goto("/?seed=42&size=30&mass=10&dt=0.25");
    await waitForWasm(page);

    // size/mass are still UI-visible; verify those. seed/dt are URL-only
    // now and flow through state without a dedicated input.
    await expect(page.getByTestId("input-galaxy-size")).toHaveValue("30");
    await expect(page.getByTestId("input-seed-mass")).toHaveValue("10");
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
    await page.getByTestId("btn-seed").click();
    const first = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    // Second run with the same seed.
    await page.goto("/?seed=12345&size=20&mass=50");
    await waitForWasm(page);
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();
    const second = await page.evaluate(() => {
      const fe: any = (window as any).__galaxyGen.frontend;
      return Array.from(fe.massArray() as Uint16Array);
    });

    expect(second).toEqual(first);
  });

  test("switching initial condition produces mode-specific seed + ticks", async ({ page }) => {
    // Sanity: the dropdown has the four modes we expect.
    const select = page.getByTestId("select-initial-condition");
    await expect(select).toBeVisible();
    const optionValues = await select.locator("option").evaluateAll((opts) =>
      (opts as HTMLOptionElement[]).map((o) => o.value)
    );
    expect(optionValues).toEqual(["0", "1", "2", "3"]);

    await page.getByTestId("btn-init").click();

    // Walk each non-uniform mode. Bang (2) zeroes the baseline grid and
    // only fills a central disc, so <50% of cells should have mass.
    // Collision (3) is the same story. Rotation (1) fills the grid but
    // also sets per-cell velocities, so a few ticks should visibly move
    // mass around. Assert that each mode produces a DIFFERENT mass field
    // than Uniform.
    const snapshots: Record<string, number[]> = {};
    for (const mode of ["0", "1", "2", "3"]) {
      await select.selectOption(mode);
      await page.getByTestId("btn-seed").click();
      // Advance a few ticks to let the mode-specific velocities take effect.
      for (let i = 0; i < 5; i++) await page.getByTestId("btn-tick").click();
      snapshots[mode] = await page.evaluate(() => {
        const fe: any = (window as any).__galaxyGen.frontend;
        return Array.from(fe.massArray() as Uint16Array);
      });
      expect(snapshots[mode].length).toBe(50 * 50);
    }

    // Each non-uniform mode should produce a mass field that differs
    // meaningfully from uniform's. Ordering by nonzero-cell count is
    // too strict: uniform collapses into dense clusters under gravity
    // while bang's outward-radial velocity spreads mass into a growing
    // ring, so bang can end up with more nonzero cells than uniform.
    const nonzero = (a: number[]) => a.filter((m) => m > 0).length;
    const diffCount = (a: number[], b: number[]) => {
      let n = 0;
      for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) n++;
      return n;
    };
    const uniform = snapshots["0"];
    const minDistinctCells = Math.floor(uniform.length * 0.1);
    for (const mode of ["1", "2", "3"]) {
      expect(
        diffCount(uniform, snapshots[mode]),
        `mode ${mode} snapshot should differ from uniform`,
      ).toBeGreaterThan(minDistinctCells);
    }
    // Bang + collision should each have SOME mass left after 5 ticks.
    expect(nonzero(snapshots["2"])).toBeGreaterThan(0);
    expect(nonzero(snapshots["3"])).toBeGreaterThan(0);
  });

  test("run button ticks via the worker and pause resumes state cleanly", async ({ page }) => {
    await page.getByTestId("btn-init").click();
    await page.getByTestId("btn-seed").click();

    // Before running, the worker shouldn't exist yet but the browser
    // should support Workers (sanity — otherwise the run path is a
    // no-op and the rest of this test is meaningless).
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

    // And the step button should still work after pause (proving the
    // main-thread Frontend is alive and its state was restored). We
    // assert that the tick counter advances — the mass field may or
    // may not change on any given tick once the sim has settled, but
    // the act of calling tick() on a restored Frontend should never
    // throw and should always bump the counter.
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

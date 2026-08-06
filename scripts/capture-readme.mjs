import { chromium } from "@playwright/test";
import { existsSync } from "node:fs";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

const captureUrl = process.env.GALAXY_CAPTURE_URL ?? "http://127.0.0.1:8081";
const output = resolve(process.env.GALAXY_CAPTURE_OUTPUT ?? "docs/project-galaxy-gen.next.gif");
const frameCount = 80;
const firstTick = 560;
const ticksPerFrame = 12;
const frameRate = 10;

if (existsSync(output)) {
  throw new Error(`refusing to overwrite existing capture: ${output}`);
}

const scratch = await mkdtemp(join(tmpdir(), "galaxy-readme-"));
const frames = join(scratch, "frame-%04d.png");
const palette = join(scratch, "palette.png");
const browser = await chromium.launch({ headless: true });

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`${command} exited with status ${result.status}`);
  }
}

try {
  const page = await browser.newPage({
    // Capture at 720p so the complete sidebar fits, then downsample to
    // the README's compact 960x540 asset.
    viewport: { width: 1280, height: 720 },
    deviceScaleFactor: 1,
  });
  const url = new URL(captureUrl);
  url.searchParams.set("seed", "12345");
  url.searchParams.set("size", "80");
  url.searchParams.set("scenario", "irregular-spiral");
  url.searchParams.set("lock", "1");
  await page.goto(url.toString(), { waitUntil: "networkidle" });
  await page.waitForFunction(() => window.__galaxyGen?.wasmReady === true);
  await page.getByTestId("btn-init").click();
  await page.evaluate(() => {
    window.__galaxyGen.captureInitialBh = window.__galaxyGen.frontend.bhMass();
  });

  for (let frame = 0; frame < frameCount; frame++) {
    const targetTick = firstTick + frame * ticksPerFrame;
    await page.evaluate((target) => {
      const api = window.__galaxyGen;
      const frontend = api.frontend;
      while (frontend.tickCount() < target) frontend.tick(0.5);
      api.dataviz.updateData(frontend, target);

      const values = new Map([
        ["ticks", target.toLocaleString("en-US")],
        ["stars", frontend.starCount().toLocaleString("en-US")],
        ["supernovae", frontend.supernovaCount().toLocaleString("en-US")],
        ["clusters born", frontend.birthCount().toLocaleString("en-US")],
        ["eaten by black hole", frontend.captureCount().toLocaleString("en-US")],
        ["black hole", `×${(frontend.bhMass() / api.captureInitialBh).toFixed(2)}`],
        ["gas reservoir", `${(100 * frontend.gasColdFraction()).toFixed(0)}%`],
      ]);
      for (const row of document.querySelectorAll("tbody tr")) {
        const cells = row.querySelectorAll("td");
        const value = values.get(cells[0]?.textContent?.trim());
        if (value != null && cells[1]) cells[1].textContent = value;
      }
    }, targetTick);

    const file = join(scratch, `frame-${String(frame).padStart(4, "0")}.png`);
    await page.screenshot({ path: file, type: "png" });
    process.stdout.write(`captured ${frame + 1}/${frameCount}\r`);
  }
  process.stdout.write("\n");

  run("ffmpeg", [
    "-n",
    "-framerate",
    String(frameRate),
    "-i",
    frames,
    "-vf",
    "scale=960:540:flags=lanczos,palettegen=max_colors=96:stats_mode=diff",
    palette,
  ]);
  run("ffmpeg", [
    "-n",
    "-framerate",
    String(frameRate),
    "-i",
    frames,
    "-i",
    palette,
    "-lavfi",
    "[0:v]scale=960:540:flags=lanczos[x];[x][1:v]paletteuse=dither=sierra2_4a:diff_mode=rectangle",
    "-loop",
    "0",
    output,
  ]);
  console.log(`wrote ${output}`);
} finally {
  await browser.close();
  await rm(scratch, { recursive: true, force: true });
}

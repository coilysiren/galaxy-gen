import { test, expect, Page } from "@playwright/test";
import { readFile } from "node:fs/promises";

// A small grid keeps the recording cheap: the recorder samples the
// render funnel, so what matters here is that frames are captured and
// encoded, not that the physics is interesting.
const RECORD_SIZE = 60;

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

/// GIF89a header, then the logical screen width/height as little-endian
/// u16. Parsed rather than trusted so a truncated or empty blob cannot
/// pass as a valid capture.
function parseGifHeader(bytes: Buffer): { signature: string; width: number; height: number } {
  return {
    signature: bytes.subarray(0, 6).toString("latin1"),
    width: bytes.readUInt16LE(6),
    height: bytes.readUInt16LE(8),
  };
}

/// Count image descriptors (0x2C) at the top level of the GIF stream.
/// Walking the block structure is the only honest frame count - 0x2C
/// also occurs inside pixel data, so a naive byte scan overcounts.
function countFrames(bytes: Buffer): number {
  let i = 13; // header + logical screen descriptor
  const flags = bytes[10];
  if (flags & 0x80) i += 3 * (1 << ((flags & 0x07) + 1)); // global color table
  let frames = 0;
  const skipSubBlocks = () => {
    while (i < bytes.length && bytes[i] !== 0) i += bytes[i] + 1;
    i += 1;
  };
  while (i < bytes.length) {
    const marker = bytes[i];
    if (marker === 0x3b) break; // trailer
    if (marker === 0x21) {
      i += 2; // extension introducer + label
      skipSubBlocks();
    } else if (marker === 0x2c) {
      frames += 1;
      const localFlags = bytes[i + 9];
      i += 10;
      if (localFlags & 0x80) i += 3 * (1 << ((localFlags & 0x07) + 1)); // local color table
      i += 1; // LZW minimum code size
      skipSubBlocks();
    } else {
      break;
    }
  }
  return frames;
}

test.describe("GIF recording", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await waitForWasm(page);
    await page.getByTestId("input-galaxy-size").fill(String(RECORD_SIZE));
    await page.getByTestId("btn-init").click();
  });

  test("record button is gated until a galaxy exists", async ({ page }) => {
    await page.goto("/");
    await waitForWasm(page);
    await expect(page.getByTestId("btn-record")).toBeDisabled();
    await page.getByTestId("btn-init").click();
    await expect(page.getByTestId("btn-record")).toBeEnabled();
    await expect(page.getByTestId("btn-record")).toHaveText("record gif");
  });

  test("stepping while armed captures frames and downloads a real GIF", async ({ page }) => {
    const record = page.getByTestId("btn-record");
    await record.click();
    await expect(record).toContainText("stop recording");

    // Default cadence is one frame per 10 sim ticks, so step past
    // several boundaries to bank more than one frame.
    for (let i = 0; i < 45; i++) await page.getByTestId("btn-tick").click();
    await expect(record).not.toContainText("(0/", { timeout: 15_000 });

    const download = await Promise.all([page.waitForEvent("download"), record.click()]).then(
      ([d]) => d
    );

    const path = await download.path();
    expect(path).toBeTruthy();
    const bytes = await readFile(path!);

    const header = parseGifHeader(bytes);
    expect(header.signature).toBe("GIF89a");
    expect(header.width).toBeGreaterThan(0);
    expect(header.height).toBeGreaterThan(0);
    // Default target width is 640, clamped to the source canvas width.
    expect(header.width).toBeLessThanOrEqual(640);
    // Trailer present means the stream was finished, not truncated.
    expect(bytes[bytes.length - 1]).toBe(0x3b);
    expect(countFrames(bytes)).toBeGreaterThan(1);

    // Filename carries the permalink coordinates that reproduce the run.
    expect(download.suggestedFilename()).toMatch(
      new RegExp(`^galaxy-.+-irregular-spiral-${RECORD_SIZE}\\.gif$`)
    );

    await expect(record).toHaveText("record gif");
  });

  test("a live run records without stalling the sim", async ({ page }) => {
    await page.getByTestId("btn-record").click();
    await page.getByTestId("btn-run").click();
    await page.waitForTimeout(4000);
    await page.getByTestId("btn-run").click();

    const ticks = await page.getByTestId("stat-ticks").textContent();
    expect(Number((ticks ?? "0").replace(/,/g, ""))).toBeGreaterThan(20);
    await expect(page.getByTestId("btn-record")).not.toContainText("(0/");

    const download = await Promise.all([
      page.waitForEvent("download"),
      page.getByTestId("btn-record").click(),
    ]).then(([d]) => d);
    const bytes = await readFile((await download.path())!);
    expect(parseGifHeader(bytes).signature).toBe("GIF89a");
    expect(countFrames(bytes)).toBeGreaterThan(1);
  });
});

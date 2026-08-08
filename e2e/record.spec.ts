import { test, expect, Page } from "@playwright/test";
import { readFile } from "node:fs/promises";

// Small grid: what matters is that frames are captured and encoded,
// not that the physics is interesting.
const RECORD_SIZE = 60;

async function waitForWasm(page: Page) {
  await expect(page.getByTestId("app")).toHaveAttribute("data-wasm-ready", "true", {
    timeout: 30_000,
  });
}

/// GIF89a header plus logical screen size, parsed rather than trusted
/// so a truncated or empty blob cannot pass as a valid capture.
function parseGifHeader(bytes: Buffer): { signature: string; width: number; height: number } {
  return {
    signature: bytes.subarray(0, 6).toString("latin1"),
    width: bytes.readUInt16LE(6),
    height: bytes.readUInt16LE(8),
  };
}

/// Walk the block structure counting image descriptors. 0x2C also
/// occurs inside pixel data, so a naive byte scan overcounts.
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

/// Walk the top level of the ISO-BMFF box tree. A playable MP4 has both `ftyp`
/// and `moov`, and an unfinalized capture drops `moov` past any size check.
function topLevelBoxes(bytes: Buffer): string[] {
  const boxes: string[] = [];
  let i = 0;
  while (i + 8 <= bytes.length) {
    let size = bytes.readUInt32BE(i);
    const type = bytes.subarray(i + 4, i + 8).toString("latin1");
    if (size === 1) {
      // 64-bit extended size in the 8 bytes after the type.
      if (i + 16 > bytes.length) break;
      size = Number(bytes.readBigUInt64BE(i + 8));
    } else if (size === 0) {
      // Box runs to end of file.
      boxes.push(type);
      break;
    }
    if (size < 8) break;
    boxes.push(type);
    i += size;
  }
  return boxes;
}

test.describe("recording", () => {
  test.beforeEach(async ({ page }) => {
    // debug=1: capture is driven by the single-step button, which is a
    // debug affordance rather than a viewer control.
    await page.goto("/?debug=1");
    await waitForWasm(page);
    await page.getByTestId("input-galaxy-size").fill(String(RECORD_SIZE));
    await page.getByTestId("btn-init").click();
  });

  test("record button is gated until a galaxy exists", async ({ page }) => {
    // debug=1: capture is driven by the single-step button, which is a
    // debug affordance rather than a viewer control.
    await page.goto("/?debug=1");
    await waitForWasm(page);
    await expect(page.getByTestId("btn-record")).toBeDisabled();
    await page.getByTestId("btn-init").click();
    await expect(page.getByTestId("btn-record")).toBeEnabled();
    await expect(page.getByTestId("btn-record")).toHaveText("record gif");
  });

  test("stepping while armed captures frames and downloads a real GIF", async ({ page }) => {
    const record = page.getByTestId("btn-record");
    await record.click();
    await expect(record).toContainText("stop (");

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

  test("the format pills switch the button label and the file extension", async ({ page }) => {
    const record = page.getByTestId("btn-record");
    await expect(record).toHaveText("record gif");
    await expect(page.getByTestId("btn-format-gif")).toHaveAttribute("data-active", "true");

    const mp4Pill = page.getByTestId("btn-format-mp4");
    // Chromium ships WebCodecs, so the pill must be live in this suite.
    // If it is disabled here the capability probe has regressed.
    await expect(mp4Pill).toBeEnabled();
    await mp4Pill.click();
    await expect(mp4Pill).toHaveAttribute("data-active", "true");
    await expect(page.getByTestId("btn-format-gif")).toHaveAttribute("data-active", "false");
    await expect(record).toHaveText("record mp4");

    // Format is locked for the duration of a capture.
    await record.click();
    await expect(mp4Pill).toBeDisabled();
    await expect(page.getByTestId("btn-format-gif")).toBeDisabled();
  });

  test("recording as mp4 downloads a finalized, playable file", async ({ page }) => {
    await page.getByTestId("btn-format-mp4").click();
    const record = page.getByTestId("btn-record");
    await record.click();
    await expect(record).toContainText("stop (");

    for (let i = 0; i < 45; i++) await page.getByTestId("btn-tick").click();
    await expect(record).not.toContainText("(0/", { timeout: 15_000 });

    const download = await Promise.all([page.waitForEvent("download"), record.click()]).then(
      ([d]) => d
    );
    const bytes = await readFile((await download.path())!);

    const boxes = topLevelBoxes(bytes);
    // ftyp proves it is an MP4; moov proves finalize() ran and the file
    // is not a truncated stream of samples with no index.
    expect(boxes).toContain("ftyp");
    expect(boxes).toContain("moov");
    expect(bytes.length).toBeGreaterThan(1024);

    expect(download.suggestedFilename()).toMatch(
      new RegExp(`^galaxy-.+-irregular-spiral-${RECORD_SIZE}\\.mp4$`)
    );
    await expect(record).toHaveText("record mp4");
  });
});

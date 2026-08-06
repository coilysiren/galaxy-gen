import { defineConfig, devices } from "@playwright/test";
import base from "./playwright.config";

// Perf-only config. The default config pins Chromium to SwiftShader so
// the WebGPU compute smoke tests have a deterministic adapter, which also
// makes every Canvas2D op software-rasterized - fine for correctness,
// useless for deciding where a frame's time goes. This config runs the
// system Chrome with hardware acceleration left alone, so render timings
// reflect what a viewer's machine actually does.
//
// Run: npx playwright test --config playwright.gpu.config.ts <spec>
export default defineConfig({
  ...base,
  projects: [
    {
      name: "chrome-gpu",
      use: {
        ...devices["Desktop Chrome"],
        channel: "chrome",
        launchOptions: {
          // Headless Chrome defaults to software GL; ask for the real one.
          args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist"],
        },
      },
    },
  ],
});

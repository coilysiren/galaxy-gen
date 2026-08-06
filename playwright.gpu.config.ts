import { defineConfig, devices } from "@playwright/test";
import base from "./playwright.config";

// Perf-only config: the default one pins SwiftShader, which
// software-rasterizes Canvas2D and misattributes render cost.

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

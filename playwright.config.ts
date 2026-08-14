import { defineConfig, devices } from "@playwright/test";
import { createHash } from "crypto";

// Per-worktree port so parallel agents don't collide on the dev server.
const portFromCwd = (): number => {
  const hash = createHash("sha256").update(process.cwd()).digest();
  return 20000 + (hash.readUInt16BE(0) % 30000);
};

const PORT = Number(process.env.PLAYWRIGHT_PORT ?? portFromCwd());
const BASE_URL = `http://localhost:${PORT}`;

export default defineConfig({
  testDir: "./e2e",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  timeout: 60_000,
  expect: { timeout: 15_000 },
  use: {
    baseURL: BASE_URL,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        // PLAYWRIGHT_BROWSER_CHANNEL=chrome runs system Chrome when the
        // bundled-browser download stalls. CI leaves this unset.
        channel: process.env.PLAYWRIGHT_BROWSER_CHANNEL || undefined,
        // Enable WebGPU in headless Chromium for compute-shader smoke tests.
        launchOptions: {
          args: [
            "--enable-unsafe-webgpu",
            "--enable-features=Vulkan",
            "--use-vulkan=swiftshader",
            "--use-webgpu-adapter=swiftshader",
          ],
        },
      },
    },
  ],
  webServer: {
    command: `npx webpack serve --port ${PORT} --host 127.0.0.1 --no-open`,
    url: BASE_URL,
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});

import { defineConfig, devices } from "@playwright/test";
import { createHash } from "crypto";

// Derive a per-worktree port so parallel agents on different worktrees don't
// collide on the dev server. Same cwd → same port, so `reuseExistingServer`
// still works across reruns in one worktree.
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
      use: { ...devices["Desktop Chrome"] },
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

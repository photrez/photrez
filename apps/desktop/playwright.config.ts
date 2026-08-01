import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  testDir: "./e2e",
  // 60s: the e2e suite runs against the Vite dev server (required so
  // page.evaluate(() => import("/src/...")) resolves). The first heavy test's
  // cold load must wait for Vite to compile the on-demand module graph, which
  // can exceed 30s on a fresh CI runner.
  timeout: 60_000,
  expect: {
    timeout: 5_000,
  },
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: [["line"]],
  use: {
    baseURL: "http://127.0.0.1:1420",
    trace: "on-first-retry",
  },
  webServer: {
    // Always use the Vite dev server. The e2e suite drives the app through
    // page.evaluate(() => import("/src/...")) to unit-test engine/export
    // helpers in-browser; those unbundled module paths only resolve under the
    // dev server, not the production `preview` build. The production bundle is
    // still validated by the separate `build` CI step.
    command: "bun run dev -- --host 127.0.0.1",
    url: "http://127.0.0.1:1420",
    reuseExistingServer: !process.env.CI,
    timeout: 60_000,
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { width: 1366, height: 820 },
        // Chromium-only flags: SwiftShader/ANGLE WebGL for headless CI.
        // Kept here (not global) because WebKit rejects `args` in launchOptions.
        launchOptions: {
          args: ["--use-gl=angle", "--use-angle=swiftshader-webgl"],
        },
      },
    },
    // WebKit runs only on macOS — the engine Tauri embeds there. Playwright's
    // WebKit on Windows/Linux is a bundled build that does NOT match the
    // engine Tauri uses, so it would add noise without coverage value.
    ...(process.platform === "darwin"
      ? [
          {
            name: "webkit",
            use: {
              ...devices["Desktop Safari"],
              viewport: { width: 1366, height: 820 },
            },
          },
        ]
      : []),
  ],
});

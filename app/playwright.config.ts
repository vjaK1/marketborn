import { defineConfig } from '@playwright/test';

/**
 * E2E against the real stack (TEST_PLAN): `sim-cli serve` hosts the world
 * over websocket, vite serves the app, Chromium drives it. The sim-cli
 * release binary must exist (`npm run e2e` at the repo root builds it
 * first). Windows-primary, like the project.
 */
export default defineConfig({
  testDir: './e2e',
  timeout: 90_000,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'retain-on-failure',
    viewport: { width: 1600, height: 900 },
  },
  webServer: [
    {
      command:
        'cmd /c "(if not exist ..\\target\\e2e-saves mkdir ..\\target\\e2e-saves) & ..\\target\\release\\sim-cli.exe serve --save-dir ..\\target\\e2e-saves"',
      port: 17771,
      reuseExistingServer: false,
      timeout: 30_000,
    },
    {
      command: 'npm run dev',
      port: 5173,
      reuseExistingServer: false,
      timeout: 60_000,
    },
  ],
});

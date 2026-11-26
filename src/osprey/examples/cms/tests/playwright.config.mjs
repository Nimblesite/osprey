// Playwright for the Git CMS editor [CMS-E2E]: real Chromium drives the Osprey Native editor the
// CMS server serves, over a throwaway site whose bare remote stands in for the published site.
import { defineConfig } from "@playwright/test";
import { BROWSER_PORT } from "./fixture.mjs";

export default defineConfig({
  testDir: ".",
  testMatch: "*.spec.mjs",
  timeout: 120_000,
  workers: 1,
  reporter: [["list"]],
  use: { baseURL: `http://127.0.0.1:${BROWSER_PORT}`, viewport: { width: 1440, height: 900 }, screenshot: "only-on-failure" },
  webServer: { command: "node serve.mjs", cwd: import.meta.dirname, url: `http://127.0.0.1:${BROWSER_PORT}/api/tree`, reuseExistingServer: false, timeout: 600_000 },
});

// Playwright config for the Osprey website end-to-end tests.
// The webServer runs `npm run build` (which regenerates the reference docs from
// the compiler that CI builds first) and serves _site over a tiny static
// server, then the tests run against it.
const { defineConfig, devices } = require("@playwright/test");

const PORT = Number(process.env.PW_PORT) || 8099;

module.exports = defineConfig({
  testDir: "./tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "on-first-retry",
  },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],
  webServer: {
    command: `npm run build && node tests/serve.cjs ${PORT}`,
    url: `http://localhost:${PORT}`,
    timeout: 120_000,
    reuseExistingServer: !process.env.CI,
  },
});

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
  // Hosted Linux runners for this public repository have four cores; two workers
  // left the suite at half the job's eight-minute budget.
  workers: process.env.CI ? 4 : undefined,
  reporter: process.env.CI ? [["github"], ["list"]] : "list",
  use: {
    baseURL: `http://localhost:${PORT}`,
    trace: "on-first-retry",
  },
  // [WEBSITE-ASSET-VERSIONS] iPhone Safari is a separate engine: tests tagged
  // @safari also run there, so a Safari-only layout failure turns the suite red.
  globalSetup: require.resolve("./tests/install-webkit.cjs"),
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    { name: "iphone-safari", grep: /@safari/, use: { ...devices["iPhone 13"] } },
  ],
  webServer: {
    command: `npm run build && node tests/serve.cjs ${PORT}`,
    url: `http://localhost:${PORT}`,
    timeout: 120_000,
    reuseExistingServer: !process.env.CI,
  },
});

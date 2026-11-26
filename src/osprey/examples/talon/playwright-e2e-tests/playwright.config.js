// Playwright config for the Talon Bank showcase. The webServer block owns the
// app lifecycle: ledger.js creates a per-run hold marker so the Osprey demo
// pauses while serving, then removes it on teardown. Dynamic API/web ports and
// marker/database paths let independent Playwright runs share one host.
const { defineConfig } = require('@playwright/test');
const { WEB, API, webClient } = require('./web-client');

module.exports = defineConfig({
  testDir: './tests',
  // The journey starts its own fresh server for each run: journey.config.js.
  testIgnore: 'journey.spec.js',
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: WEB,
    screenshot: 'only-on-failure',
  },
  webServer: [{
    command: 'node ledger.js --serve',
    cwd: __dirname,
    url: `${API}/api/accounts`,
    reuseExistingServer: false,
    timeout: 120_000,
    gracefulShutdown: { signal: 'SIGTERM', timeout: 5_000 },
  }, webClient],
});

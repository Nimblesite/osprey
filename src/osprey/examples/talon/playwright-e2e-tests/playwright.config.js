// Playwright config for the Talon Bank showcase. The webServer block owns the
// app lifecycle: it creates the hold marker so the Osprey demo pauses while
// serving, then removes it on teardown. Teardown must be a signal the shell
// can trap: without `gracefulShutdown` Playwright SIGKILLs the process group,
// and dash (Linux /bin/sh) runs no EXIT trap on an untrapped SIGTERM, so
// either way the marker leaks and every later bank-*-test refuses to start.
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
    command:
      'touch /tmp/talon_bank.hold && trap "rm -f /tmp/talon_bank.hold" EXIT INT TERM && ../../../../../target/release/osprey ../backend --run',
    cwd: __dirname,
    url: `${API}/api/accounts`,
    reuseExistingServer: false,
    timeout: 120_000,
    gracefulShutdown: { signal: 'SIGTERM', timeout: 5_000 },
  }, webClient],
});

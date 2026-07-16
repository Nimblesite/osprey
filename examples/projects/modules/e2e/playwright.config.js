// Playwright config for the Talon Bank showcase. The webServer block owns the
// app lifecycle: it creates the hold marker so the Osprey demo pauses while
// serving, then removes it on teardown so the binary finishes its run.
const { defineConfig } = require('@playwright/test');

module.exports = defineConfig({
  testDir: './tests',
  timeout: 30_000,
  fullyParallel: false,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: 'http://127.0.0.1:18790',
    screenshot: 'only-on-failure',
  },
  webServer: {
    command:
      'touch /tmp/talon_bank.hold && trap "rm -f /tmp/talon_bank.hold" EXIT && ../../../../target/release/osprey .. --run',
    cwd: __dirname,
    url: 'http://127.0.0.1:18790/api/accounts',
    reuseExistingServer: false,
    timeout: 20_000,
  },
});

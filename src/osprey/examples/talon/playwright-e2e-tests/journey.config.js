// Talon Bank's journey (app/self-test.json) in real Chromium. Each run starts its own fresh server
// (ledger.js), so there is no webServer here. Every shot is a golden beside the app, one folder per
// platform since fonts render differently; 0.5% of pixels may differ, the limit every host's
// goldens share. Linux goldens come from the official Playwright image:
//   docker run --platform linux/amd64 -p 3000:3000 mcr.microsoft.com/playwright:v1.64.1-noble \
//     npx -y playwright@1.64.1 run-server --port 3000 --host 0.0.0.0
//   OSPREY_GOLDENS_PLATFORM=linux PW_TEST_CONNECT_WS_ENDPOINT=ws://127.0.0.1:3000/ \
//     PW_TEST_CONNECT_EXPOSE_NETWORK='<loopback>' npx playwright test -c journey.config.js --update-snapshots
const { defineConfig } = require('@playwright/test');

module.exports = defineConfig({
  testDir: './tests',
  testMatch: 'journey.spec.js',
  timeout: 180_000,
  fullyParallel: false,
  workers: 1,
  reporter: [['list']],
  // A browser on another machine (PW_TEST_CONNECT_WS_ENDPOINT) names its own platform here.
  snapshotPathTemplate: `../app/goldens/web/${process.env.OSPREY_GOLDENS_PLATFORM ?? '{platform}'}/{testName}/{arg}{ext}`,
  expect: { toHaveScreenshot: { maxDiffPixelRatio: 0.005 } },
  use: { baseURL: 'http://127.0.0.1:18790', screenshot: 'only-on-failure' },
});

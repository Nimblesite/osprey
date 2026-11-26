// Talon Bank's one journey, app/self-test.json, played in real Chromium at a desktop and a phone
// width, each from a fresh ledger. The SwiftUI, iOS and Android hosts play the same file; each keeps
// its own goldens.
const path = require('node:path');
const { test } = require('@playwright/test');
const { start } = require('../ledger');
const { useApi } = require('../web-client');
const { play } = require('./journey');

const SCRIPT = path.join(__dirname, '..', '..', 'app', 'self-test.json');

test.describe.configure({ mode: 'serial' });

for (const [name, viewport] of [['desktop', { width: 1280, height: 860 }], ['phone', { width: 390, height: 844 }]]) {
  test(`the Talon journey plays at ${name} width`, async ({ page }) => {
    const stop = await start();
    try {
      await page.setViewportSize(viewport);
      await useApi(page);
      await page.goto('/');
      await play(page, SCRIPT);
    } finally {
      await stop();
    }
  });
}

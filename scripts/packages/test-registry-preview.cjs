// Browser acceptance for the served folder registry [PACKAGE-HOSTED-ROUTES-CATALOG].
const assert = require('node:assert/strict');
const { chromium } = require('../../src/toolchain/website/node_modules/playwright');

(async () => {
  const browser = await chromium.launch();
  try {
    for (const width of [390, 1440]) {
      const page = await browser.newPage({ viewport: { width, height: 900 } });
      await page.goto('http://127.0.0.1:4187/packages/@osprey/fastapi/');
      const icon = await page.locator('link[rel="icon"]').getAttribute('href');
      assert.equal(await page.locator('.prose-brand img').getAttribute('src'), icon);
      const asset = await page.request.get(icon.startsWith('/') ? new URL(icon, page.url()).href : icon);
      assert.equal(asset.status(), 200);
      const hash = require('node:crypto').createHash('sha256').update(await asset.body()).digest('hex');
      assert.equal(new URL(icon, page.url()).searchParams.get('v'), hash.slice(0, 12));
      assert.match(await page.locator('main').innerText(), /MIT AND BSD-3-Clause/);
      assert.match(await page.locator('main').innerText(), /python-flavor/);
      await page.getByRole('link', { name: 'Osprey repository publisher', exact: true }).first().click();
      await page.getByRole('heading', { name: 'Osprey repository publisher' }).waitFor();
      await page.getByRole('link', { name: '@osprey/re', exact: true }).click();
      await page.getByRole('heading', { name: '@osprey/re' }).waitFor();
      assert.equal(await page.evaluate(() => document.body.scrollWidth <= innerWidth), true);
      await page.goto('http://127.0.0.1:4187/packages/@osprey/flash-attn/');
      await page.getByRole('link', { name: 'API documentation' }).click();
      await page.getByRole('link', { name: 'flash_attn_func', exact: true }).first().click();
      await page.getByRole('heading', { name: /flash_attn_func/ }).first().waitFor();
      await page.goto('http://127.0.0.1:4187/?q=regular-expression');
      await page.locator('.prose-package[hidden]').first().waitFor({ state: 'attached' });
      assert.deepEqual(await page.locator('.prose-package:visible h3').allTextContents(), ['@osprey/re']);
      await page.goto('http://127.0.0.1:4187/?q=nonexistent-fixture');
      await page.getByText('No matching packages.').waitFor();
      await page.route('**/*.wasm', route => route.abort());
      await page.reload();
      await page.getByText('Search is unavailable.').waitFor();
      assert.equal(await page.locator('.prose-package:visible').count(), 0);
      await page.close();
    }
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });

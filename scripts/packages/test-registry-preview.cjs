// Browser acceptance for the served folder registry [PACKAGE-HOSTED-ROUTES-CATALOG].
const assert = require('node:assert/strict');
const { chromium } = require('../../src/toolchain/website/node_modules/playwright');

(async () => {
  const browser = await chromium.launch();
  try {
    const catalog = await browser.newPage();
    await catalog.goto('http://127.0.0.1:4187/');
    const packages = await catalog.locator('.prose-package h3 a').evaluateAll(links => links.map(link => link.href));
    assert.ok(packages.length > 0);
    const missingReferences = [];
    for (const url of packages) {
      await catalog.goto(url);
      const releases = catalog.locator('.prose-release');
      assert.ok(await releases.count() > 0, url);
      for (const release of await releases.all()) {
        const version = await release.locator('h3').innerText();
        const docs = release.getByRole('link', { name: 'API documentation' });
        if (await docs.count() === 0) {
          missingReferences.push(url + ' ' + version);
          continue;
        }
        const link = await docs.getAttribute('href');
        assert.equal(link, version + '/docs/api/index.html');
        const reference = await catalog.request.get(new URL(link, url).href);
        assert.equal(reference.status(), 200, url);
        assert.match(await reference.text(), /<h1[^>]*>API Reference<\/h1>/);
      }
    }
    await catalog.close();
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
      const downloadAction = page.getByRole('link', { name: 'Download e1.r1' });
      const [download] = await Promise.all([page.waitForEvent('download'), downloadAction.click()]);
      const bundle = JSON.parse(await require('node:fs/promises').readFile(await download.path(), 'utf8'));
      assert.equal(bundle.key, '@osprey/re');
      assert.equal(bundle.epoch, 1);
      assert.equal(bundle.ordinal, 1);
      assert.equal(bundle.digest, await page.locator('.prose-release code').first().textContent());
      assert.ok(bundle.files.some(file => file.path === 'osprey.toml'));
      await page.getByRole('link', { name: 'Upload a package' }).click();
      await page.getByRole('heading', { name: 'Upload a package' }).waitFor();
      assert.equal(await page.getByRole('status').innerText(), 'Public uploads are unavailable.');
      await page.getByRole('link', { name: 'Browse packages' }).click();
      await page.getByRole('heading', { name: 'Browse packages' }).waitFor();
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
    assert.deepEqual(missingReferences, [], 'Releases missing an API reference');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });

// Serve the real Pages archive with its shipped asset names and security headers.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync: exec } = require('node:child_process');
const { createHash } = require('node:crypto');

async function withHostedPage(browser, run) {
  const work = fs.mkdtempSync('target/registry-site-build/archive-check.');
  try {
    const pack = directory => exec('python3', ['scripts/packages/archive-registry-site.py', directory, work], {encoding:'utf8'}).trim();
    const archive = pack('target/registry-site-build/hosted-fixture');
    const digest = createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
    assert.equal(path.basename(archive), 'registry-site-' + digest + '.zip');
    const extracted = path.join(work, 'site');
    exec('unzip', ['-q', archive, '-d', extracted]);
    assert.equal(pack(extracted), archive, 'Repacking yields the same bytes');
    await run(await packagedPage(browser, extracted));
  } finally {
    fs.rmSync(work, {recursive:true, force:true});
  }
}

async function packagedPage(browser, site) {
  const website = 'https://packages.example.test';
  const api = 'https://api.example.test';
  const page = await browser.newPage();
  const policy = fs.readFileSync(path.join(site, '_headers'), 'utf8').trim().split('\n');
  assert.equal(policy.shift(), '/*');
  const siteHeaders = Object.fromEntries(policy.map(line => {
    const delimiter = line.indexOf(':');
    return [line.slice(0, delimiter).trim().toLowerCase(), line.slice(delimiter + 1).trim()];
  }));
  page.setDefaultTimeout(10000);
  const failures = [];
  page.on('pageerror', error => failures.push(error.message));
  page.on('requestfailed', request => failures.push({url:request.url(), error:request.failure()?.errorText}));
  await page.route(website + '/**', async route => {
    const pathname = new URL(route.request().url()).pathname;
    const file = (pathname.startsWith('/packages/') || pathname.startsWith('/publishers/') || pathname.startsWith('/tags/')) ? path.join(site, 'package.html') : path.join(site, pathname, pathname.endsWith('/') ? 'index.html' : '');
    const body = await fs.promises.readFile(file);
    const mime = {'.html':'text/html','.css':'text/css','.svg':'image/svg+xml','.mjs':'text/javascript','.wasm':'application/wasm'}[path.extname(file)];
    assert.ok(mime, pathname);
    const hash = /\.([0-9a-f]{12})\.(?:css|mjs|wasm)$/.exec(pathname);
    if (hash) assert.equal(createHash('sha256').update(body).digest('hex').slice(0,12), hash[1]);
    await route.fulfill({body, headers:siteHeaders, contentType:mime});
  });
  return {page, website, api, failures};
}

module.exports = { withHostedPage };

// Browser acceptance for the served folder registry [PACKAGE-HOSTED-ROUTES-CATALOG].
const assert = require('node:assert/strict');
const { chromium } = require('../../src/toolchain/website/node_modules/playwright');

async function checkHostedAccount(browser, livePort) {
  const fs = require('node:fs');
  const path = require('node:path');
  const exec = require('node:child_process').execFileSync;
  const work = fs.mkdtempSync('target/registry-site-build/archive-check.');
  try {
    const pack = directory => exec('python3', ['scripts/packages/archive-registry-site.py', directory, work], {encoding:'utf8'}).trim();
    const archive = pack('target/registry-site-build/hosted-fixture');
    const digest = require('node:crypto').createHash('sha256').update(fs.readFileSync(archive)).digest('hex');
    assert.equal(path.basename(archive), 'registry-site-' + digest + '.zip');
    const extracted = path.join(work, 'site');
    exec('unzip', ['-q', archive, '-d', extracted]);
    assert.equal(pack(extracted), archive, 'Repacking yields the same bytes');
    await checkPackagedAccount(browser, extracted, livePort);
  } finally {
    fs.rmSync(work, {recursive:true, force:true});
  }
}

async function checkPackagedAccount(browser, site, livePort) {
  const website = 'https://packages.example.test';
  const api = 'https://api.example.test';
  const page = await browser.newPage();
  const fs = require('node:fs');
  const path = require('node:path');
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
    if (hash) assert.equal(require('node:crypto').createHash('sha256').update(body).digest('hex').slice(0,12), hash[1]);
    await route.fulfill({body, headers:siteHeaders, contentType:mime});
  });
  const headers = {'access-control-allow-origin':website,'access-control-allow-credentials':'true'};
  if (livePort) {
    assert.match(livePort, /^[0-9]{4,5}$/);
    await page.route(url => url.origin === api && (url.pathname.startsWith('/v1/packages') || url.pathname.startsWith('/v1/publishers/')), async route => {
      const upstream = await page.request.get('http://127.0.0.1:' + livePort + new URL(route.request().url()).pathname, {
        headers:{Host:'127.0.0.1','X-Edge-Secret':'fixture-edge-secret-at-least-32-bytes',Origin:website}, timeout:5000,
      });
      assert.equal(upstream.status(), 200);
      assert.equal(upstream.headers()['access-control-allow-origin'], website);
      await route.fulfill({response:upstream});
    });
    await page.goto(website);
    await page.getByRole('link', {name:'By tag'}).click();
    await page.getByRole('heading', {name:'Tags', exact:true}).waitFor();
    await page.getByText('No tags have five packages yet.').waitFor();
    assert.equal(await page.getByRole('link', {name:'graphics', exact:true}).count(), 0);
    await page.goto(website + '/tags/graphics/');
    await page.getByRole('heading', {name:'Tag not found'}).waitFor();
    await page.goto(website);
    await page.getByRole('link', {name:'By publisher'}).click();
    await page.getByRole('heading', {name:'Publishers'}).waitFor();
    assert.equal(await page.getByRole('link', {name:'Atlas Engineering', exact:true}).count(), 1);
    await page.goto(website + '/?q=http');
    await page.getByRole('link', {name:'@atlas/net'}).click();
    await page.getByRole('heading', {name:'e1.r1'}).waitFor();
    await page.locator('[id="e1.r1"] .prose-license').first().waitFor();
    await page.getByText("NOTICE.md", {exact:true}).first().click();
    await page.getByText("Adapted from Atlas upstream.", {exact:false}).waitFor();
    assert.equal(await page.locator('[id="e1.r2"] .prose-license').textContent(), "Apache-2.0");
    const content = 'https://content.example.test';
    async function atContent(path, credential) {
      return page.request.get('http://127.0.0.1:' + livePort + path, {
        headers:{Host:'content.example.test','X-Edge-Secret':'fixture-edge-secret-at-least-32-bytes',...credential},
        timeout:5000,
      });
    }
    await page.route(content + '/**', async route => {
      await route.fulfill({response:await atContent(new URL(route.request().url()).pathname)});
    });
    const downloadLink = page.getByRole('link', {name:'Download e1.r1', exact:true});
    const href = new URL(await downloadLink.getAttribute('href'));
    assert.equal(href.origin, content);
    const downloaded = page.waitForEvent('download');
    await downloadLink.click();
    const file = await downloaded;
    assert.equal(file.suggestedFilename(), 'osprey-source.json');
    const source = JSON.parse(await fs.promises.readFile(await file.path(), 'utf8'));
    assert.equal(source.key, '@atlas/net');
    assert.equal(source.digest, href.pathname.split('/')[3]);
    assert.equal(source.files.find(item => item.path === 'src/main.osp').text, 'fn main() = 0\n');
    assert.equal((await atContent(href.pathname, {Cookie:'__Host-registry=opaque'})).status(), 421);
    assert.equal((await atContent('/v1/packages')).status(), 421);
    const corrupted = new URL(await page.getByRole('link', {name:'Download e1.r2', exact:true}).getAttribute('href'));
    assert.equal((await atContent(corrupted.pathname)).status(), 503);
    await page.getByText('Network access', {exact:true}).waitFor();
    await page.getByRole('link', {name:'Atlas Labs'}).waitFor();
    assert.equal(await page.getByRole('link', {name:'networking', exact:true}).count(), 0);

    await page.getByRole('link', {name:'Atlas Engineering'}).click();
    await page.getByRole('heading', {name:'Atlas Engineering'}).waitFor();
    assert.equal(await page.getByRole('link', {name:'@atlas/net e1.r2'}).count(), 0);
    await page.getByText('Network access', {exact:true}).waitFor();
    await page.getByRole('link', {name:'@atlas/net e1.r1'}).click();
    assert.equal(new URL(page.url()).hash, '#e1.r1');
    await page.getByRole('heading', {name:'e1.r1'}).waitFor();
    await page.locator('[id="e1.r1"] .prose-license').first().waitFor();
    await page.getByText("NOTICE.md", {exact:true}).first().waitFor();
    assert.deepEqual(failures.filter(failure => !(failure.url === href.href && failure.error === "net::ERR_ABORTED")), []);
    await page.close();
    return;
  }
  await page.route(api + '/v1/packages', route => route.fulfill({headers,json:{packages:[]}}));
  const csrfToken = 'sha256:' + 'a'.repeat(64);
  let signOutRequests = 0;
  await page.route(api + '/v1/auth/logout', route => {
    if (route.request().method() === 'OPTIONS') return route.fulfill({status:204,headers:{...headers,
      'access-control-allow-methods':'POST','access-control-allow-headers':'X-CSRF-Token'}});
    assert.equal(route.request().method(), 'POST');
    assert.equal(route.request().headers()['x-csrf-token'], csrfToken);
    assert.equal(route.request().postData(), null);
    signOutRequests++;
    if (signOutRequests === 1) return route.fulfill({status:503,headers,json:{code:'registry.not-ready',status:503}});
    return route.fulfill({status:204,headers});
  });
  for (const code of [401, 503, 200]) {
    await page.route(api + '/v1/auth/session', route => route.fulfill({status:code,headers,json:{authenticated:code === 200,csrfToken}}));
    await page.goto(website + '/upload/');
    const panel = page.locator('[data-session-url]');
    const login = panel.getByRole('link', {includeHidden:true});
    if (code === 401) {
      await login.waitFor();
      assert.equal(await login.getAttribute('href'), api + '/v1/auth/github/start');
    } else {
      await panel.getByText(code === 503 ? /could not verify/ : /Signed in/).waitFor();
      assert.equal(await login.isVisible(), false);
    }
    if (code === 200) {
      const logout = panel.getByRole('button', {name:'Sign out'});
      await logout.click();
      await panel.getByText('Could not sign out. Try again.').waitFor();
      assert.equal(await login.isVisible(), false);
      await logout.click();
      await panel.getByText('Signed out.', {exact:true}).waitFor();
      await login.waitFor();
      assert.equal(await logout.count(), 0);
      assert.equal(signOutRequests, 2);
    }
    await page.unroute(api + '/v1/auth/session');
  }
  await page.unroute(api + '/v1/auth/logout');
  await page.unroute(api + '/v1/packages');
  const entry = {key:'@atlas/net',keywords:['networking','web'],releases:[{digest:'sha256:' + 'a'.repeat(64),epoch:1,ordinal:1,publisherId:"11111111-1111-4111-8111-111111111111",publisherName:"Atlas Engineering",licenseExpression:"MIT",notices:[{path:"NOTICE.md",text:"Upstream <img src=x>"}]}],summary:'HTTP tools <img src=x>',warnings:['Network access']};
  entry.releases.unshift({...entry.releases[0], digest:'sha256:' + 'b'.repeat(64), ordinal:2, licenseExpression:'Apache-2.0', notices:[]});
  entry.releases[1].epoch = 2;
  entry.releases[1].notices[0].path = 'NOTICES.md';
  let payload = {packages:[entry]};
  await page.route(api + '/v1/packages', route => route.fulfill({headers,json:payload}));
  await page.goto(website + '/?q=http');
  assert.equal(await page.locator('#results .prose-license').textContent(), 'MIT');
  await page.getByRole('link', {name:entry.key}).click();
  assert.equal(new URL(page.url()).pathname, '/packages/@atlas/net/');
  await page.getByText('Network access', {exact:true}).waitFor();
  const metadata = page.locator('.prose-facts');
  assert.equal(await metadata.locator('.prose-license').textContent({timeout:2000}), 'MIT');
  const notices = metadata.getByRole('link', {name:'Notices and attribution'});
  await notices.click();
  assert.equal(new URL(page.url()).hash, '#e2.r1-notices');
  await page.getByText('NOTICES.md', {exact:true}).click();
  await page.getByText('Upstream <img src=x>', {exact:true}).waitFor();
  assert.equal(await page.locator('.prose-notice img').count(), 0);
  await page.goBack();
  await page.goBack();
  assert.equal(await page.locator('#q').inputValue(), 'http');
  await page.goto(website + '/?q=unknown');
  await page.getByText('No matching packages.').waitFor();
  const tagged = Array.from({length:4}, (_, i) => ({
    ...entry, key: '@atlas/fixture-' + i, keywords: i === 3 ? ['networking'] : entry.keywords,
  }));
  payload = {packages:[entry, ...tagged, tagged[0]]};
  await page.goto(website + '/tags/');
  await page.getByRole('link', {name:'networking', exact:true}).click();
  await page.getByRole('heading', {name:'networking', exact:true}).waitFor();
  assert.equal(await page.locator('.prose-package').count(), 5);
  await page.getByRole('link', {name:entry.key, exact:true}).click();
  await page.getByRole('link', {name:'networking', exact:true}).waitFor();
  assert.equal(await page.getByRole('link', {name:'web', exact:true}).count(), 0);
  await page.goto(website + '/tags/web/');
  await page.getByRole('heading', {name:'Tag not found'}).waitFor();
  for (const keywords of [null, ['../escape'], ['web', 'networking'], ['web', 'web']]) {
    payload = {packages:[{...entry,keywords}]};
    await page.goto(website);
    await page.getByText('The public catalog is unavailable.').waitFor();
    assert.equal(await page.locator('#results article').count(), 0);
  }
  payload = {packages:[entry, {key:'@atlas/incomplete',summary:'Missing warnings'}]};
  await page.goto(website);
  await page.getByText('The public catalog is unavailable.').waitFor();
  assert.equal(await page.locator('#results article').count(), 0);
  assert.deepEqual(failures, []);
  await page.close();
}

async function checkReferenceTable(browser) {
  for (const width of [390, 1024, 1440]) {
    const page = await browser.newPage({viewport:{width, height:900}});
    await page.goto('http://127.0.0.1:4187/packages/@osprey/einops/e1.r1/docs/api/index.html');
    const table = page.locator('.article table').first();
    const lines = await table.locator('thead th, tbody tr:first-child td:nth-child(-n+2)').evaluateAll(cells => cells.map(cell => {
      const range = document.createRange();
      range.selectNodeContents(cell.querySelector('a') ?? cell);
      return [cell.textContent, new Set([...range.getClientRects()].map(rect => rect.top)).size];
    }));
    assert.deepEqual(lines, [['Name',1], ['Kind',1], ['Description',1], ['Parsing',1], ['Module',1]], 'Reference headings, short names and kinds must each fit on one line at ' + width);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
    await page.close();
  }
}

async function checkReferenceHighlighting(browser) {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  for (const [path, language, signature, example] of [
    ['express/e1.r1/docs/api/express-encode-encodeurl.html', 'typescript', 'fn', 'encodeUrl'],
    ['einops/e1.r1/docs/api/einops-parsing-format_axes.html', 'python', 'def', 'from'],
  ]) {
    await page.goto('http://127.0.0.1:4187/packages/@osprey/' + path);
    const observed = await page.locator('.prose-doc-article pre code').evaluateAll(blocks => blocks.map(code => ({
      language: code.className,
      text: code.textContent,
      tokens: [...code.querySelectorAll('.token')].map(span => ({
        kind: span.classList[1], text: span.textContent, color: getComputedStyle(span).color,
      })),
      color: getComputedStyle(code).color,
    })));
    const [declaration, source, output] = observed;
    assert.equal(declaration.language, 'language-' + language);
    assert.ok(declaration.tokens.some(token => token.kind === 'keyword' && token.text === signature && token.color !== declaration.color), path);
    assert.ok(source.tokens.some(token => token.text === example && token.color !== source.color), path);
    assert.ok(source.tokens.some(token => token.kind === 'string' && token.color !== source.color), path);
    assert.deepEqual(output.tokens, [], path);
    if (language === 'typescript') assert.match(source.text, /encodeUrl\("\/find\/a b\?x=1&y=2"\)/);
  }
  assert.deepEqual(errors, []);
  await page.close();
}

async function checkPackageReferenceScope(browser) {
  const page = await browser.newPage();
  const base = 'http://127.0.0.1:4187/packages/@osprey/express/e1.r1/docs/';
  await page.goto(base + 'api/index.html');
  assert.equal(await page.getByText('Built-in functions', {exact: false}).count(), 0);
  await page.getByRole('searchbox').fill('encodeUrl');
  await page.locator('#results').getByRole('link', {name: 'express::Encode::encodeUrl'}).waitFor();
  await page.getByRole('searchbox').fill('byteAt');
  assert.equal(await page.locator('#results').getByRole('link', {name: 'byteAt'}).count(), 0);
  assert.equal((await page.request.get(base + 'functions/byteat.html')).status(), 404);
  await page.close();
}

async function checkLocalTagThreshold(browser) {
  const page = await browser.newPage();
  const base = 'http://127.0.0.1:4187';
  await page.goto(base + '/tags/');
  assert.equal(await page.getByRole('link', {name: 'fastapi', exact: true}).count(), 0);
  await page.getByRole('link', {name: 'python-flavor', exact: true}).click();
  assert.ok(await page.locator('.prose-package').count() >= 5);
  await page.getByRole('link', {name: '@osprey/fastapi', exact: true}).click();
  assert.equal(await page.getByRole('link', {name: 'fastapi', exact: true}).count(), 0);
  assert.equal((await page.request.get(base + '/tags/fastapi/')).status(), 404);
  await page.close();
}

async function checkCoverGeometry(browser) {
  const base = 'http://127.0.0.1:4187';
  const paths = ['/', '/publishers/', '/publishers/osprey-repository/', '/tags/',
    '/tags/python-flavor/', '/packages/@osprey/flash-attn/', '/upload/', '/guide/', '/publishing/'];
  for (const width of [320, 390, 768, 1440]) {
    const page = await browser.newPage({viewport:{width, height:900}});
    let origin;
    for (const path of paths) {
      await page.goto(base + path);
      const panel = page.locator('main :is(.prose-intro, .prose-cover, .prose-publisher)').first();
      const bounds = await panel.boundingBox();
      assert.ok(bounds, path);
      origin ??= bounds;
      assert.deepEqual(bounds, origin, path + ' at ' + width);
      assert.equal(bounds.height, 296, path);
      assert.equal(bounds.y, 108, path);
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), path);
    }
    await page.close();
  }
}

(async () => {
  const browser = await chromium.launch();
  try {
    if (process.argv[2] === '--database-catalog') return await checkHostedAccount(browser, process.argv[3]);
    if (process.argv.includes('--built-hosted-account')) return await checkHostedAccount(browser);
    if (process.argv.includes('--highlighting-only')) return await checkReferenceHighlighting(browser);
    if (process.argv.includes('--reference-scope-only')) return await checkPackageReferenceScope(browser);
    if (process.argv.includes('--tag-threshold-only')) return await checkLocalTagThreshold(browser);
    if (process.argv.includes('--covers-only')) return await checkCoverGeometry(browser);
    if (!process.argv.includes('--tables-only')) require('node:child_process').execFileSync('make', ['-C','src/osprey/packages','registry-website-fixture']);
    if (process.argv.includes('--hosted-account-only')) return await checkHostedAccount(browser);
    await checkReferenceTable(browser);
    if (process.argv.includes('--tables-only')) return;
    await checkReferenceHighlighting(browser);
    await checkPackageReferenceScope(browser);
    await checkLocalTagThreshold(browser);
    await checkCoverGeometry(browser);
    await checkHostedAccount(browser);
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
      await page.goto('http://127.0.0.1:4187/');
      const box = selector => page.locator(selector).boundingBox();
      const header = await box('.prose-bar');
      const cover = await box('.prose-intro');
      await page.evaluate(() => scrollTo(0, 600));
      assert.deepEqual(await box('.prose-bar'), header, 'Header stays anchored while scrolling');
      for (const path of ['/upload/', '/guide/', '/publishing/', '/tags/', '/publishers/', '/packages/@osprey/express/e1.r1/docs/api/index.html']) {
        await page.goto(new URL(path, page.url()).href);
        assert.deepEqual(await box('.prose-bar'), header, path);
        assert.equal((await box('main')).x, cover.x, path);
      }
      await page.goto('http://127.0.0.1:4187/');
      await page.getByRole('link', { name: 'By publisher' }).click();
      await page.getByRole('link', { name: 'Osprey repository publisher' }).click();
      await page.getByRole('link', { name: '@osprey/fastapi', exact: true }).click();
      await page.getByRole('link', { name: 'python-flavor', exact: true }).click();
      await page.getByRole('heading', { name: 'python-flavor', exact: true }).waitFor();
      assert.ok(await page.getByRole('link', { name: '@osprey/fastapi', exact: true }).count());
      assert.equal(await page.getByRole('link', { name: '@osprey/express', exact: true }).count(), 0);
      await page.goto('http://127.0.0.1:4187/');
      await page.getByRole('link', { name: 'By tag' }).click();
      await page.getByRole('link', { name: 'python-flavor', exact: true }).click();
      await page.getByRole('link', { name: '@osprey/fastapi', exact: true }).click();
      assert.deepEqual(await box('.prose-bar'), header);
      const detail = await box('.prose-cover');
      for (const axis of ['x', 'y', 'width']) assert.equal(detail[axis], cover[axis]);

      const icon = await page.locator('link[rel="icon"]').getAttribute('href');
      assert.equal(await page.locator('.prose-brand img').getAttribute('src'), icon);
      const asset = await page.request.get(icon.startsWith('/') ? new URL(icon, page.url()).href : icon);
      assert.equal(asset.status(), 200);
      const hash = require('node:crypto').createHash('sha256').update(await asset.body()).digest('hex');
      assert.equal(hash, 'b3839fb6decc4df859ec12db9c62e530dc24b91349e90cf11be1038271a94a0b', 'approved catalog drawer icon');
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
      await page.getByLabel('Package', {exact:true}).selectOption('@osprey/fastapi');
      const commit = 'a'.repeat(40);
      await page.getByLabel('Git commit').fill(commit);
      await page.getByRole('button', {name:'Review release'}).click();
      assert.equal(await page.locator('#review code').innerText(), commit);
      assert.equal(await page.locator('#review strong').innerText(), '@osprey/fastapi');
      assert.equal(await page.getByText('Public uploads are unavailable.').count(), 1);
      await page.getByRole('link', { name: 'Browse packages' }).click();
      await page.getByRole('heading', { name: 'Browse packages' }).waitFor();
      assert.equal(await page.evaluate(() => document.body.scrollWidth <= innerWidth), true);
      await page.goto('http://127.0.0.1:4187/packages/@osprey/flash-attn/');
      const catalogStyles = await page.locator('link[rel=stylesheet]').getAttribute('href');
      await page.getByRole('link', { name: 'API documentation' }).click();
      assert.equal(await page.locator('link[rel=stylesheet]').first().getAttribute('href'), catalogStyles);
      assert.equal(await page.locator('.prose-brand img').getAttribute('src'), icon);
      assert.equal(await page.getByRole('link', { name: '@osprey/flash-attn', exact: true }).first().getAttribute('href'), '/packages/@osprey/flash-attn/');
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

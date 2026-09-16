// Browser acceptance for `osprey --docs --docs-format html` ([DOC-EXPORT-HTML],
// [DOC-EXPORT-PAGES], [DOC-EXPORT-CSS]).
//
// The unit tests assert what the exporter WRITES. Only a browser can assert what
// a reader GETS: that the stylesheet cascade lands the way the argument order
// promised, that search works with no server behind it, that a phone reader
// reaches the article, that a keyboard reaches the navigation. Every defect this
// file has caught — a dead brand link, pages with no <h1>, 131 nav links pushing
// the article off a phone screen, raw `**markdown**` in search results, symbol
// links resolving to nothing — passed the unit tests first.
//
// Pages are opened over file://, not through a server: that is the strictest
// case (opaque origin, no fetch, no base URL) and the one users hit first.
//
// Usage: node scripts/verify-docs-html.mjs [path-to-osprey-binary]
import { createRequire } from 'node:module';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { FIXTURES } from './verify-docs-html-fixtures.mjs';

const REPO = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');

// Playwright is a dependency of the node subprojects, not of the repo root, and
// an ES import resolves from THIS file's directory rather than the caller's. Ask
// each subproject that already installs it, and say which ones were tried when
// none has it: a gate that cannot run must fail, never quietly report success.
const PLAYWRIGHT_HOSTS = ['examples/projects/modules/e2e', 'website'];
const loadChromium = () => {
  for (const host of PLAYWRIGHT_HOSTS) {
    const anchor = path.join(REPO, host, 'package.json');
    if (!fs.existsSync(anchor)) continue;
    try {
      return createRequire(anchor)('playwright').chromium;
    } catch {
      continue;
    }
  }
  console.error(`playwright is not installed. Run \`npm ci\` in one of: ${PLAYWRIGHT_HOSTS.join(', ')}`);
  return process.exit(1);
};
const chromium = loadChromium();
const BIN = path.resolve(process.argv[2] ?? path.join(REPO, 'target/release/osprey'));
const WORK = fs.mkdtempSync(path.join(os.tmpdir(), 'osprey-docs-html-'));
const THEMES = ['osprey', 'midnight', 'paper'];

let failures = 0;
const check = (name, ok, detail = '') => {
  if (!ok) failures++;
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${name}${detail ? '  — ' + detail : ''}`);
};

if (!fs.existsSync(BIN)) {
  console.error(`osprey binary not found at ${BIN}`);
  process.exit(1);
}

// ── fixtures ────────────────────────────────────────────────────────────────
for (const [relative, body] of Object.entries(FIXTURES)) {
  const file = path.join(WORK, relative);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, body);
}

const generate = (out, args) =>
  execFileSync(BIN, ['--docs', '--docs-dir', path.join(WORK, out), '--docs-format', 'html', ...args], {
    cwd: REPO, stdio: ['ignore', 'pipe', 'pipe'],
  });

for (const theme of THEMES) {
  // With user pages and stylesheets…
  generate(`site-${theme}`, [
    '--docs-theme', theme,
    '--docs-page', path.join(WORK, 'guides'),
    '--docs-css', path.join(WORK, 'brand.css'),
    '--docs-css', path.join(WORK, 'override.css'),
    path.join(REPO, 'examples/projects/modules'),
  ]);
  // …and without, so the themes can be compared as themselves rather than as
  // the override that was deliberately forced on all three above.
  generate(`plain-${theme}`, ['--docs-theme', theme, path.join(REPO, 'examples/projects/modules')]);
}
generate('site-symbols', [path.join(WORK, 'symbols')]);

const SITE = path.join(WORK, 'site-osprey');
const url = (site, page) => 'file://' + path.join(WORK, site, page);

// ── static audit: every reference resolves, and none is root-absolute ────────
const walk = (dir) => fs.readdirSync(dir, { withFileTypes: true })
  .flatMap((e) => (e.isDirectory() ? walk(path.join(dir, e.name)) : [path.join(dir, e.name)]));
const pages = walk(SITE).filter((f) => f.endsWith('.html'));
const broken = [];
const absolute = [];
for (const file of pages) {
  // The inline search script contains JS strings that look like hrefs.
  const html = fs.readFileSync(file, 'utf8').replace(/<script[\s\S]*?<\/script>/g, '');
  for (const m of html.matchAll(/(?:href|src)="([^"]*)"/g)) {
    const raw = m[1];
    if (!raw || raw.startsWith('#') || raw.startsWith('http') || raw.startsWith('mailto:')) continue;
    if (raw.startsWith('/')) { absolute.push(`${path.relative(SITE, file)} -> ${raw}`); continue; }
    const target = path.resolve(path.dirname(file), decodeURIComponent(raw.split('#')[0]));
    if (!fs.existsSync(target)) broken.push(`${path.relative(SITE, file)} -> ${raw}`);
  }
}
check(`link audit: ${pages.length} pages, ${broken.length} broken`, broken.length === 0, broken.slice(0, 5).join('; '));
// A root-absolute reference pins the site to a server root: it breaks under a
// URL prefix and breaks again on file://.
check(`every reference is relative (${absolute.length} absolute)`, absolute.length === 0, absolute.slice(0, 3).join('; '));

// ── browser ─────────────────────────────────────────────────────────────────
const browser = await chromium.launch();
const errors = [];
const page = await browser.newPage();
page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
page.on('pageerror', (e) => errors.push(String(e)));
const article = () => page.evaluate(() => document.querySelector('main').innerHTML);

await page.goto(url('site-osprey', 'index.html'));
check('landing loads from file://', (await page.title()) === 'Osprey documentation');
check('landing has an h1', (await page.locator('h1').first().innerText()) === 'Osprey documentation');
check(`sidebar lists pages (${await page.locator('#tree a').count()})`, (await page.locator('#tree a').count()) > 100);

// search, with no server and no fetch
await page.fill('#q', 'websocket');
await page.waitForTimeout(120);
const hits = await page.locator('.hits li').count();
check(`file:// search returns hits (${hits})`, hits > 0);
const firstHit = await page.locator('.hits a').first().getAttribute('href');
check('a search hit resolves to a real file', fs.existsSync(path.join(SITE, firstHit)), firstHit);
check('search hides the navigation while filtering', await page.locator('#menu').isHidden());
check('no raw markdown reaches the results', !(await page.locator('.hits').innerText()).includes('**'));
await page.fill('#q', 'zzznotathing');
await page.waitForTimeout(120);
check('search states an empty result', (await page.locator('#results .note').innerText()).includes('No page matches'));
await page.fill('#q', 'Release');
await page.waitForTimeout(120);
check('user guides are searchable', (await page.locator('.hits a').allInnerTexts()).some((t) => t.includes('Release Notes')));

// stylesheet cascade
await page.goto(url('site-osprey', 'guides/getting-started.html'));
const styles = await page.evaluate(() => ({
  accent: getComputedStyle(document.documentElement).getPropertyValue('--accent').trim(),
  background: getComputedStyle(document.body).backgroundColor,
  sheets: [...document.querySelectorAll('link[rel=stylesheet]')].map((l) => l.getAttribute('href')),
}));
check('the theme is linked first', styles.sheets[0].endsWith('assets/theme.css'), styles.sheets.join(' '));
check('user stylesheets follow in argument order',
  styles.sheets[1].includes('brand') && styles.sheets[2].includes('override'), styles.sheets.join(' '));
check('user CSS wins on a theme custom property', styles.accent === 'rgb(255, 0, 128)', styles.accent);
check('user CSS wins on an ordinary theme selector', styles.background === 'rgb(1, 2, 3)', styles.background);
check('highlighting preserves every code character',
  await page.locator('main pre code').first().textContent() === 'fn double(x) = x * 2\n');
check('Osprey examples highlight keywords and numbers',
  await page.locator('main pre code .token.keyword').first().textContent().catch(() => '') === 'fn' &&
  await page.locator('main pre code .token.number').count() === 1);

// user markdown, in full
const guide = await page.evaluate(() => ({
  table: !!document.querySelector('main table td'),
  task: !!document.querySelector('main input[type=checkbox]'),
  strike: !!document.querySelector('main del'),
  quote: !!document.querySelector('main blockquote'),
  code: !!document.querySelector('main pre code'),
  pwned: window.__PWNED__ === 1,
  injected: !!document.querySelector('main script'),
  shown: document.querySelector('main').innerText.includes('<script>'),
}));
for (const [what, ok] of [['a table', guide.table], ['task list items', guide.task], ['strikethrough', guide.strike],
  ['a blockquote', guide.quote], ['a code fence', guide.code]]) {
  check(`a user guide renders ${what}`, ok);
}
check('raw HTML did not execute', !guide.pwned && !guide.injected);
check('raw HTML is shown as text', guide.shown);

// links between user pages
await page.goto(url('site-osprey', 'guides/deep-dive/advanced-topics.html'));
const links = await page.evaluate(() => [...document.querySelectorAll('main a')].map((a) => a.getAttribute('href')));
check('a percent-encoded link keeps its anchor', links.includes('../getting-started.html#install'), links.join(' '));
check('an angle-bracketed sibling link resolves', links.includes('release-notes.html'), links.join(' '));
check('a javascript: link is inert', !links.some((l) => (l ?? '').startsWith('javascript:')), links.join(' '));
await page.locator('main a', { hasText: 'the start' }).click();
await page.waitForLoadState();
check('following a guide link arrives', page.url().endsWith('guides/getting-started.html#install'), page.url());

// symbol links, resolved in their own scope first
await page.goto(url('site-symbols', 'api/shop-money-parse.html'));
const symbols = await page.evaluate(() => ({
  // The breadcrumb is a paragraph of links too, and it is asserted on its own
  // page below; here only the links a doc comment wrote are the subject.
  links: Object.fromEntries([...document.querySelectorAll('main p:not(.crumb) a')].map((a) => [a.textContent, a.getAttribute('href')])),
  text: document.querySelector('main').innerText,
}));
check('[helper] resolves inside its own module',
  symbols.links.helper === '../api/shop-money-helper.html', JSON.stringify(symbols.links));
check('[Owner.member] resolves across modules',
  symbols.links['Ledger.post'] === '../api/shop-ledger-post.html', JSON.stringify(symbols.links));
check('a built-in [Symbol] resolves',
  symbols.links.toString === '../functions/tostring.html', JSON.stringify(symbols.links));
// The nearest owner wins: `total` exists in this module and in Ledger, and
// inside this module it means this module's.
check('a name the enclosing scope owns resolves there',
  symbols.links.total === '../api/shop-money-total.html', JSON.stringify(symbols.links));
// `summarize` belongs to two modules, neither of them this one. Guessing would
// send the reader to a declaration the author never mentioned.
check('an ambiguous unrelated leaf stays plain text',
  symbols.text.includes('[summarize]') && !symbols.links.summarize, JSON.stringify(symbols.links));
for (const [label, href] of Object.entries(symbols.links)) {
  check(`symbol link ${label} points at a real file`,
    fs.existsSync(path.resolve(WORK, 'site-symbols/api', href)), href);
}
await page.goto(url('site-symbols', 'api/shop-ledger-post.html'));
check('the same spelling resolves to the sibling module there',
  (await article()).includes('href="../api/shop-ledger-helper.html"'), 'shop::Ledger::post');

// ── what an undocumented declaration page says for itself ───────────────────
// The bank project's API carries no `(** *)` comments at all. Every page below
// was reported as empty by a reader: a name and a bare type, and nothing else.
await page.goto(url('plain-osprey', 'api/bank-api-route.html'));
const declaration = await page.evaluate(() => {
  const section = (heading) => [...document.querySelectorAll('.article h2')]
    .find((h) => h.textContent.trim() === heading)?.nextElementSibling;
  const items = (heading) => {
    let node = section(heading);
    while (node && node.tagName !== 'UL') node = node.nextElementSibling;
    return [...(node?.children ?? [])].map((li) => li.textContent.trim());
  };
  const panel = document.querySelector('.signature-panel');
  return {
    open: !!panel?.open,
    signature: panel?.querySelector('pre')?.textContent.trim(),
    crumb: [...document.querySelectorAll('.crumb a')].map((a) => a.getAttribute('href')),
    parameters: items('Parameters').map((text) => text.split('—')[0].trim()),
    effects: [...document.querySelectorAll('.article a')]
      .map((a) => a.getAttribute('href')).filter((href) => href?.includes('store') || href?.includes('audit')),
    provenance: document.querySelector('.article > p:last-child')?.textContent.trim(),
    headings: document.querySelectorAll('.article h2').length,
  };
});
// The effect row is the fact a caller cannot compile without and the one the
// type model leaves off: a signature without it reads as pure.
check('the signature carries the declared effect row',
  (declaration.signature ?? '').includes('! [Ledger::Store, Audit]'), declaration.signature);
check('the signature is a panel that opens expanded', declaration.open);
// ML lowers a clause head into a chain of one-parameter lambdas, so a page
// built from the declaration node alone names `method` and stops.
check('every parameter of a curried function is named',
  JSON.stringify(declaration.parameters) === JSON.stringify(['method', 'path', 'body']),
  JSON.stringify(declaration.parameters));
check('each effect links to the page documenting it',
  declaration.effects.length === 2 && declaration.effects.every((href) => fs.existsSync(
    path.resolve(WORK, 'plain-osprey/api', href))), JSON.stringify(declaration.effects));
check('the page says which file and line declares it',
  /Defined in src\/api\/routes\.ospml\s*, line \d+\./.test(declaration.provenance ?? ''),
  declaration.provenance);
check('the breadcrumb links the module that owns the declaration',
  declaration.crumb.includes('../api/bank-api.html'), JSON.stringify(declaration.crumb));
check('an undocumented declaration still fills a page',
  declaration.headings >= 3, `${declaration.headings} sections`);

// The disclosure is a real one: the code is shown on arrival and goes away when
// the control is used. A closed <details> keeps a box for its children, so the
// question is answered by checkVisibility rather than by measuring one.
const disclosure = await page.evaluate(() => {
  const summary = document.querySelector('.signature-panel > summary');
  const code = document.querySelector('.signature-panel pre');
  if (!summary || !code) return null;
  const shown = code.checkVisibility({ contentVisibilityAuto: true });
  summary.click();
  return { shown, hidden: !code.checkVisibility({ contentVisibilityAuto: true }) };
});
check('the signature is shown on arrival and folds away from its control',
  !!disclosure && disclosure.shown && disclosure.hidden, JSON.stringify(disclosure));

// A Description column that is blank on every row tells a reader nothing.
await page.goto(url('plain-osprey', 'api/bank-api.html'));
const described = await page.evaluate(() => [...document.querySelectorAll('.article tbody tr')]
  .map((row) => row.cells[2]?.textContent.trim() ?? ''));
check('every member of a listing is described',
  described.length > 0 && described.every((cell) => cell.length > 0), JSON.stringify(described));

// themes
const themed = {};
for (const theme of THEMES) {
  await page.goto(url(`plain-${theme}`, 'index.html'));
  themed[theme] = await page.evaluate(() => ({
    scheme: getComputedStyle(document.documentElement).colorScheme,
    ground: getComputedStyle(document.body).backgroundColor,
    accent: getComputedStyle(document.querySelector('#tree a')).color,
  }));
}
check('midnight is a dark scheme', themed.midnight.scheme === 'dark', JSON.stringify(themed.midnight));
check('the light themes stay light', themed.osprey.scheme === 'light' && themed.paper.scheme === 'light');
for (const property of ['ground', 'accent']) {
  check(`themes render distinct ${property}s`,
    new Set(THEMES.map((t) => themed[t][property])).size === THEMES.length,
    THEMES.map((t) => `${t}:${themed[t][property]}`).join(' '));
}

// contrast: documentation nobody can read is documentation that was not written
const CONTRAST = `(() => {
  const channel = (c) => { const s = c / 255; return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4); };
  const luminance = (rgb) => {
    const [r, g, b] = rgb.match(/\\d+(\\.\\d+)?/g).slice(0, 3).map(Number);
    return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
  };
  const opaque = (rgb) => rgb && rgb !== 'transparent' && !rgb.startsWith('rgba(0, 0, 0, 0)');
  const behind = (el) => {
    for (let node = el; node; node = node.parentElement) {
      const bg = getComputedStyle(node).backgroundColor;
      if (opaque(bg)) return bg;
    }
    return getComputedStyle(document.body).backgroundColor;
  };
  const ratio = (el) => {
    const a = luminance(getComputedStyle(el).color);
    const b = luminance(behind(el));
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  };
  const of = (selector) => { const el = document.querySelector(selector); return el ? ratio(el) : null; };
  return {
    // A bare 'main p' selector matched the muted crumb first, so the body-text
    // number was silently a second copy of the muted one. Ask for prose.
    body: of('main p:not(.crumb):not(.summary)'),
    link: of('main a'),
    code: of('main code'),
    muted: of('.crumb'),
    nav: of('#tree a'),
    current: of('#tree a[aria-current=page]'),
    summary: of('#menu > summary'),
    brand: of('.brand'),
  };
})()`;
for (const theme of THEMES) {
  await page.goto(url(`plain-${theme}`, 'functions/index.html'));
  const contrast = await page.evaluate(CONTRAST);
  for (const [what, measured] of Object.entries(contrast)) {
    // WCAG AA for body-sized text. Documentation nobody can read is
    // documentation that was not written.
    check(`${theme}: ${what} contrast >= 4.5:1`,
      measured !== null && measured >= 4.5,
      measured === null ? 'element absent' : `${measured.toFixed(2)}:1`);
  }
}

// ── legibility ──────────────────────────────────────────────────────────────
// Type set too small, and an outline that read as a second column of loose
// prose, were both reported by a reader against a suite that was all green.
// Measure them.
await page.setViewportSize({ width: 1800, height: 1000 });
for (const theme of THEMES) {
  await page.goto(url(`plain-${theme}`, 'functions/abs.html'));
  const layout = await page.evaluate(() => {
    const px = (el, property) => parseFloat(getComputedStyle(el)[property]);
    const toc = document.querySelector('.toc');
    const label = document.querySelector('.on-page');
    const ruled = (el) => ['Top', 'Right', 'Bottom', 'Left']
      .some((side) => parseFloat(getComputedStyle(el)[`border${side}Width`]) > 0);
    const painted = (el) => {
      const bg = getComputedStyle(el).backgroundColor;
      return bg !== 'rgba(0, 0, 0, 0)' && bg !== getComputedStyle(document.body).backgroundColor;
    };
    return {
      body: px(document.querySelector('main p:not(.crumb):not(.summary)'), 'fontSize'),
      nav: px(document.querySelector('#tree a'), 'fontSize'),
      code: px(document.querySelector('main pre code'), 'fontSize'),
      outline: toc ? px(toc, 'fontSize') : null,
      bounded: !!toc && (ruled(toc) || painted(toc) || (!!label && ruled(label))),
      entries: document.querySelectorAll('.toc a').length,
      headings: document.querySelectorAll('.article h2[id], .article h3[id]').length,
      article: document.querySelector('.article').getBoundingClientRect().width,
      main: document.querySelector('main').getBoundingClientRect().width,
    };
  });
  // 16px is the browser default. Documentation set smaller than every other
  // page on the web is documentation people squint at.
  check(`${theme}: body text is at least 16px`, layout.body >= 16, `${layout.body}px`);
  check(`${theme}: navigation text is at least 14px`, layout.nav >= 14, `${layout.nav}px`);
  check(`${theme}: code text is at least 14px`, layout.code >= 14, `${layout.code}px`);
  check(`${theme}: the outline is at least 14px`, layout.outline >= 14, `${layout.outline}px`);
  // A boundary is what separates a list of links from the article beside it.
  check(`${theme}: the outline is a bounded panel`, layout.bounded);
  check(`${theme}: the outline lists every heading`,
    layout.entries === layout.headings && layout.entries > 0, `${layout.entries}/${layout.headings}`);
  // Prose run to the full width of a wide monitor is unreadable at any size.
  // The outline is taken out of the row first: while it sits there it absorbs
  // the slack, so an article compared against its own container measures the
  // same whether or not anything caps it.
  const measure = await page.evaluate(() => {
    document.querySelector('.toc').remove();
    const article = document.querySelector('.article');
    const style = getComputedStyle(article);
    const ruler = document.createElement('span');
    ruler.style.cssText = 'position:absolute;visibility:hidden;white-space:pre';
    ruler.textContent = '0'.repeat(100);
    article.append(ruler);
    const em = ruler.getBoundingClientRect().width / 100;
    ruler.remove();
    return (article.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight)) / em;
  });
  check(`${theme}: the article holds a readable measure`, measure <= 90, `${Math.round(measure)} characters`);
}

// keyboard and document semantics
await page.goto(url('site-osprey', 'api/index.html'));
await page.keyboard.press('Tab');
check('the first tab stop is the skip link', (await page.evaluate(() => document.activeElement.className)) === 'skip');
check('the current page is marked for assistive tech', (await page.locator('[aria-current=page]').count()) === 1);
check('the page has exactly one h1', (await page.locator('h1').count()) === 1);
check('navigation is open on a wide screen', await page.locator('#tree').isVisible());

// phone
const phone = await browser.newContext({ viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true });
const small = await phone.newPage();
small.on('pageerror', (e) => errors.push(String(e)));
await small.goto(url('site-osprey', 'guides/getting-started.html'));
const mobile = await small.evaluate(() => ({
  overflow: document.documentElement.scrollWidth > window.innerWidth,
  open: document.getElementById('menu').open,
  summary: getComputedStyle(document.querySelector('#menu>summary')).display,
  mainTop: document.querySelector('main').getBoundingClientRect().top,
}));
check('no horizontal overflow on a phone', !mobile.overflow);
check('navigation is collapsed on a phone', mobile.open === false);
check('the disclosure control is shown on a phone', mobile.summary === 'block', mobile.summary);
check('the article starts on the first screen', mobile.mainTop < 300, `${Math.round(mobile.mainTop)}px`);
// The disclosure must open from the keyboard alone, with no pointer.
await small.locator('#menu > summary').focus();
await small.keyboard.press('Enter');
check('the keyboard opens the navigation', await small.locator('#tree').isVisible());
check('opened navigation is bounded, not endless',
  (await small.evaluate(() => getComputedStyle(document.getElementById('tree')).maxHeight)) !== 'none');
await small.keyboard.press('Enter');
check('the keyboard closes it again', !(await small.locator('#tree').isVisible()));
await small.fill('#q', 'ledger');
await small.waitForTimeout(120);
check('search works on a phone', (await small.locator('.hits li').count()) > 0);

check(`no console or page errors (${errors.length})`, errors.length === 0, errors.slice(0, 3).join(' | '));

await browser.close();
fs.rmSync(WORK, { recursive: true, force: true });
console.log(`\n${failures === 0 ? 'docs html: ALL CHECKS PASSED' : `docs html: ${failures} CHECK(S) FAILED`}`);
process.exit(failures === 0 ? 0 : 1);

// Render all built-in documentation designs and a local comparison gallery.
// Usage: node scripts/preview-docs.mjs [project-or-source] [output-directory]
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const SOURCE = path.resolve(process.argv[2] ?? path.join(REPO, 'examples/projects/modules'));
const OUTPUT = path.resolve(process.argv[3] ?? path.join(REPO, 'target/api-docs-previews'));
const THEMES = [
  ['osprey', 'Osprey', 'Clear structure. Room to read.', 'A blue reference layout with a quiet sidebar, module cards, and an outline beside the article.'],
  ['midnight', 'Midnight', 'The developer’s desk.', 'Dark surfaces, compact spacing, and monospaced details for an API reference that feels at home beside your editor.'],
  ['paper', 'Paper', 'A reference worth reading.', 'An editorial layout with serif headings, generous margins, and simple ruled lists.'],
];
const { chromium } = createRequire(path.join(REPO, 'website/package.json'))('playwright');
fs.mkdirSync(OUTPUT, { recursive: true });
const browser = await chromium.launch();
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 }, deviceScaleFactor: 1 });
  for (const [theme] of THEMES) {
    execFileSync(path.join(REPO, 'target/release/osprey'), [
      '--docs', SOURCE, '--docs-format', 'html', '--docs-theme', theme,
      '--docs-dir', path.join(OUTPUT, theme), '--docs-page', path.join(REPO, 'website/src/docs/documentation.md'),
    ], { cwd: REPO, stdio: 'pipe' });
    await capture(page, theme, 'index.html', 'desktop', 1440, 1000);
    await capture(page, theme, 'functions/byteat.html', 'reference', 1440, 1000);
    await capture(page, theme, 'index.html', 'mobile', 390, 844);
  }
} finally { await browser.close(); }

fs.writeFileSync(path.join(OUTPUT, 'index.html'), gallery());
console.log(`Documentation previews: ${path.join(OUTPUT, 'index.html')}`);

async function capture(page, theme, file, name, width, height) {
  await page.setViewportSize({ width, height });
  await page.goto(pathToFileURL(path.join(OUTPUT, theme, file)).href);
  await page.screenshot({ path: path.join(OUTPUT, `${theme}-${name}.png`) });
}

function gallery() {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>Osprey · Documentation designs</title>
<style>
*{box-sizing:border-box}body{margin:0;background:#f3f5f8;color:#182132;font:16px/1.6 system-ui,sans-serif}
main{max-width:1500px;margin:auto;padding:64px 36px}header{max-width:760px;margin-bottom:48px}
.eyebrow{font-size:12px;font-weight:700;letter-spacing:.15em;text-transform:uppercase;color:#395c97}
h1{font-size:clamp(36px,5vw,60px);line-height:1.06;letter-spacing:-.045em;margin:16px 0 24px}
header p{color:#536174;font-size:18px}section{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:24px}
article{background:white;border:1px solid #dce2ea;border-radius:14px;overflow:hidden;box-shadow:0 10px 35px #17263d08}
img{display:block;width:100%;border-bottom:1px solid #dce2ea}.details{padding:26px}h2{font-size:27px;letter-spacing:-.025em;margin:0}
h3{font-size:15px;margin:8px 0}.details p{font-size:14px;color:#536174;min-height:90px}
nav{display:flex;flex-wrap:wrap;gap:10px}a{color:#234fa0;text-underline-offset:3px}
nav a{font-weight:600;font-size:13px;border:1px solid #dce2ea;border-radius:6px;padding:7px 10px;text-decoration:none}
nav a:first-child{background:#234fa0;border-color:#234fa0;color:white}a:focus-visible{outline:3px solid #e69724;outline-offset:4px}
footer{margin-top:32px;color:#536174;font-size:14px}code{font-family:ui-monospace,monospace}
@media(max-width:1000px){section{grid-template-columns:1fr}main{padding:32px 18px}.details p{min-height:0}}
</style></head><body><main><header><p class="eyebrow">Osprey / Documentation designs</p>
<h1>Three ways into the same API.</h1><p>Explore three complete templates using the same project. Each includes module documentation, search, examples, and guides. Everything works offline.</p></header>
<section>${THEMES.map(([theme, name, tagline, description]) => `<article>
<a href="${theme}/index.html" aria-label="Open ${name} documentation"><img src="${theme}-desktop.png" alt="${name} documentation at desktop width"></a>
<div class="details"><h2>${name}</h2><h3>${tagline}</h3><p>${description}</p><nav aria-label="${name} previews">
<a href="${theme}/index.html">Explore template ↗</a><a href="${theme}/functions/byteat.html">API page</a><a href="${theme}-mobile.png">Phone preview</a></nav></div></article>`).join('')}
</section><footer>Generate any template with <code>--docs-format html --docs-theme osprey|midnight|paper</code>. Add Markdown guides with <code>--docs-page</code> and your own styles with <code>--docs-css</code>.</footer>
</main></body></html>`;
}

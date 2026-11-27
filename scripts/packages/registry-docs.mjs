// Integrate the compiler's static reference with the catalog's rendered shell.
import { readFileSync, writeFileSync, readdirSync, renameSync } from 'node:fs';
import { join, relative, dirname } from 'node:path';
import { createHash } from 'node:crypto';

const [site, docs, key, version] = process.argv.slice(2);
if (!/^@[a-z0-9-]+\/[a-z0-9-]+$/.test(key) || !/^e\d+\.r\d+$/.test(version)) throw Error('Invalid release identity');
const header = readFileSync(join(site, 'upload/index.html'), 'utf8').split('</header>')[0] + '</header>';
const packageUrl = '/packages/' + key + '/';
const localAssets = new Map();
function asset(name, script) {
  const hash = createHash('sha256').update(script).digest('hex').slice(0, 12);
  const file = name + '.' + hash + '.js';
  if (!localAssets.has(file)) writeFileSync(join(docs, 'assets', file), script);
  localAssets.set(file, true);
  return file;
}
function part(html, pattern, label) {
  const match = pattern.exec(html);
  if (!match) throw Error('Compiler template is missing ' + label);
  return match[1];
}
const search = asset('search', readFileSync(join(docs, 'assets/search-index.js')));
function integrate(file) {
  const html = readFileSync(file, 'utf8');
  const title = part(html, /<title>([^<]*)<\/title>/, 'title').replace(/ — Osprey documentation$/, '');
  const nav = part(html, /(<label class="skip" for="q">[\s\S]*?)\n<div class="side-footer">/, 'reference navigation');
  const article = part(html, /<div class="article">\n([\s\S]*?)\n<\/div>\n<nav class="toc"/, 'article').replace('<h1>Osprey documentation</h1>', '<h1>' + key + ' documentation</h1>');
  const script = part(html, /<\/script>\n<script>([\s\S]*)<\/script>\n<\/body>/, 'reference controls');
  const root = relative(dirname(file), docs).split('\\').join('/') || '.';
  const head = header.replace(/<title>[^<]*<\/title>/, () => '<title>' + title + ' · ' + key + ' · Osprey Packages</title>');
  const trail = '<nav class="prose-doc-trail" aria-label="Breadcrumb"><a href="/">Packages</a> / <a href="' + packageUrl + '">' + key + '</a> / <a href="' + root + '/api/index.html">Reference</a><code>' + version + '</code></nav>';
  const body = '<main class="prose-docs">' + trail + '<div class="prose-doc-grid"><aside class="prose-doc-menu">' + nav + '<nav class="toc" aria-label="On this page" hidden></nav></aside><article id="content" class="article prose-doc-article">' + article + '</article></div></main>';
  const scripts = [search, asset('reference', script)].map(name => '<script src="' + root + '/assets/' + name + '"></script>').join('');
  writeFileSync(file + '.tmp', head + body + scripts + '</body></html>');
  renameSync(file + '.tmp', file);
}
for (const entry of readdirSync(docs, {recursive:true, withFileTypes:true})) {
  if (entry.isFile() && entry.name.endsWith('.html')) integrate(join(entry.parentPath, entry.name));
}

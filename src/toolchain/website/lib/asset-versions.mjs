// Implements [WEBSITE-ASSET-VERSIONS]: every site-local stylesheet and script
// URL carries a hash of the file it names. Browsers and the CDN cache CSS and
// JS for minutes, so without the hash a deploy can serve new HTML with old CSS.
// On 2026-11-06 that hid the mobile "Browse documentation" menu: cached
// prose.css hid `.toc-aside` unless the layout had a class the new HTML no
// longer carries. Stylesheets are versioned after their imports, so a changed
// imported file also changes the URL of every stylesheet that imports it.
import { createHash } from "node:crypto";
import { readdir, readFile, writeFile } from "node:fs/promises";
import { join, posix, resolve } from "node:path";

const LOCAL_ASSET = /^\/(?!\/)[^?#"')\s]+\.(?:css|js|mjs)$/;
const HTML_REFERENCE = /(\s(?:href|src)=")([^"]+)(")/g;
const CSS_REFERENCE = /(@import\s+url\(\s*["']?|url\(\s*["']?)([^"')\s]+)(["']?\s*\))/g;

async function filesUnder(dir) {
  const entries = await readdir(dir, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map((entry) => (entry.isDirectory() ? filesUnder(join(dir, entry.name)) : [join(dir, entry.name)])),
  );
  return nested.flat();
}

const digest = (text) => createHash("sha256").update(text).digest("hex").slice(0, 12);

function rewrite(text, pattern, version, base) {
  return text.replace(pattern, (whole, before, url, after) => {
    const absolute = url.startsWith("/") ? url : posix.join(base, url);
    if (!LOCAL_ASSET.test(absolute)) return whole;
    const hash = version(absolute);
    return hash ? `${before}${url}?v=${hash}${after}` : whole;
  });
}

// Versions one stylesheet after every stylesheet it imports; `seen` guards cycles.
async function versionStylesheet(siteDir, url, hashes, seen) {
  if (hashes.has(url) || seen.has(url)) return hashes.get(url);
  seen.add(url);
  const path = join(siteDir, url);
  const text = await readFile(path, "utf8").catch(() => null);
  if (text === null) return undefined;
  const imports = [...text.matchAll(CSS_REFERENCE)].map(([, , ref]) => (ref.startsWith("/") ? ref : posix.join(posix.dirname(url), ref)));
  for (const ref of imports.filter((ref) => ref.endsWith(".css"))) await versionStylesheet(siteDir, ref, hashes, seen);
  const versioned = rewrite(text, CSS_REFERENCE, (ref) => hashes.get(ref), posix.dirname(url));
  if (versioned !== text) await writeFile(path, versioned);
  hashes.set(url, digest(versioned));
  return hashes.get(url);
}

export async function versionAssets(outputDir) {
  const siteDir = resolve(outputDir);
  const files = await filesUnder(siteDir);
  const url = (file) => "/" + file.slice(siteDir.length).split(/[\\/]/).filter(Boolean).join("/");
  const hashes = new Map();
  for (const file of files.filter((f) => /\.(?:js|mjs)$/.test(f))) hashes.set(url(file), digest(await readFile(file)));
  for (const file of files.filter((f) => f.endsWith(".css"))) await versionStylesheet(siteDir, url(file), hashes, new Set());
  for (const file of files.filter((f) => f.endsWith(".html"))) {
    const text = await readFile(file, "utf8");
    const versioned = rewrite(text, HTML_REFERENCE, (ref) => hashes.get(ref), posix.dirname(url(file)));
    if (versioned !== text) await writeFile(file, versioned);
  }
  return hashes;
}

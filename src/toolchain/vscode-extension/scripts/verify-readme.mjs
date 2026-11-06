import assert from "node:assert/strict";
import path from "node:path";
import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import yauzl from "yauzl";

const { version } = JSON.parse(fs.readFileSync(new URL("../package.json", import.meta.url), "utf8"));
const filename = process.argv[2] ?? fileURLToPath(new URL(`../osprey-${version}.vsix`, import.meta.url));
const archive = await promisify(yauzl.open)(path.resolve(filename), { lazyEntries: true });
const files = new Map();
for await (const entry of entries(archive)) {
  assert.ok(!/(^|\/)(\.deslop|\.too_many_cooks)(\/|$)/.test(entry.fileName),
    `Local development data must not ship in VSIX: ${entry.fileName}`);
  if (entry.fileName === "extension/readme.md" || /\.(png|gif)$/i.test(entry.fileName)) {
    const stream = await promisify(archive.openReadStream.bind(archive))(entry);
    const chunks = [];
    for await (const chunk of stream) chunks.push(chunk);
    files.set(entry.fileName, Buffer.concat(chunks));
  }
}
const readme = files.get("extension/readme.md")?.toString();
assert.ok(readme, "VSIX must include its README");
const images = [...readme.matchAll(/<img\b[^>]*src="([^"]+)"/g)].map(match => match[1]);
assert.ok(images.some(source => source.endsWith(".gif")), "README must show the editor GIF");
for (const source of images) {
  assert.ok(source.startsWith("https://"), `Marketplace images require HTTPS: ${source}`);
  const bundled = source === "https://ospreylang.dev/assets/images/logo.png" ? "icon.png" :
    source.replace("https://media.githubusercontent.com/media/Nimblesite/osprey/main/vscode-extension/", "");
  assert.ok(!bundled.includes(":"), `Unknown README image host: ${source}`);
  const bytes = files.get(path.posix.join("extension", bundled));
  assert.ok(bytes, `README image missing from VSIX: ${source}`);
  assert.ok(bytes.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10])) ||
    /^GIF8[79]a/.test(bytes.subarray(0, 6).toString()), `Invalid image: ${source}`);
}
console.log("VSIX README: bundled logo and editor GIF PASS");

async function* entries(zip) {
  while (true) {
    const next = new Promise((resolve, reject) => {
      zip.once("entry", resolve);
      zip.once("end", () => resolve(null));
      zip.once("error", reject);
    });
    zip.readEntry();
    const entry = await next;
    zip.removeAllListeners("entry");
    zip.removeAllListeners("end");
    zip.removeAllListeners("error");
    if (!entry) return;
    yield entry;
  }
}

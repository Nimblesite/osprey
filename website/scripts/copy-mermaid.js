// Vendor the Mermaid runtime into src/assets so diagrams render from this
// origin — no CDN, no third-party request, and the same bytes every build.
//
// Only the minified ESM entry and its lazily-imported chunks are copied, and
// `.map` files are skipped: that is 3.6 MB of runtime instead of the 83 MB
// `mermaid/dist` tree. A page pulls the entry plus the few chunks its diagram
// types need, and pages without a diagram load none of it (see
// src/js/diagrams.js).
const fs = require('fs');
const path = require('path');

const websiteDir = path.resolve(__dirname, '..');
const mermaidDist = path.join(websiteDir, 'node_modules', 'mermaid', 'dist');
const entryName = 'mermaid.esm.min.mjs';
const chunkDirName = path.join('chunks', 'mermaid.esm.min');
const destDir = path.join(websiteDir, 'src', 'assets', 'vendor', 'mermaid');

function copyRuntimeFiles(fromDir, toDir) {
  fs.mkdirSync(toDir, { recursive: true });
  const runtime = fs
    .readdirSync(fromDir)
    .filter((file) => file.endsWith('.mjs'));
  for (const file of runtime) {
    fs.copyFileSync(path.join(fromDir, file), path.join(toDir, file));
  }
  return runtime.length;
}

try {
  if (!fs.existsSync(path.join(mermaidDist, entryName))) {
    throw new Error(`mermaid not installed at ${mermaidDist} — run npm install`);
  }

  // Rebuild from scratch so a mermaid upgrade cannot leave stale chunks behind:
  // chunk filenames carry content hashes, and orphans would ship forever.
  fs.rmSync(destDir, { recursive: true, force: true });
  fs.mkdirSync(destDir, { recursive: true });

  fs.copyFileSync(path.join(mermaidDist, entryName), path.join(destDir, entryName));
  const chunkCount = copyRuntimeFiles(
    path.join(mermaidDist, chunkDirName),
    path.join(destDir, chunkDirName)
  );

  console.log(`✅ Vendored mermaid runtime → src/assets/vendor/mermaid (${chunkCount} chunks)`);
} catch (error) {
  console.error('❌ Failed to vendor mermaid runtime:', error.message);
  process.exit(1);
}

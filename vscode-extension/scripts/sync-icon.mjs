// Single source of truth for the Osprey mark: website/src/assets/images/logo.png.
//
// The VS Code extension needs a physical icon file inside the package — vsce
// cannot reference a URL for the Marketplace icon or the `.osp` file icon — so we
// GENERATE it from the canonical logo at build time rather than committing a copy.
// Wired into `precompile` and `vscode:prepublish`; the output (icon.png) is
// gitignored. This keeps exactly one logo in version control.
import { copyFileSync, existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const SOURCE = resolve(here, "../../website/src/assets/images/logo.png");
const DEST = resolve(here, "../icon.png");

if (!existsSync(SOURCE)) {
  console.error(`sync-icon: canonical logo not found at ${SOURCE}`);
  process.exit(1);
}

// The logo is stored in Git LFS. A checkout without LFS leaves a small text
// pointer in its place, which the Marketplace rejects as an icon after every
// other channel has already published it — so refuse it here, at build time.
const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
const png = readFileSync(SOURCE);
if (!png.subarray(0, PNG_SIGNATURE.length).equals(PNG_SIGNATURE)) {
  console.error(
    `sync-icon: ${SOURCE} is not a PNG (a Git LFS pointer?) — run \`git lfs pull --include=website/src/assets/images/logo.png\``,
  );
  process.exit(1);
}

// The Marketplace rejects icons over 1024×1024, again only after every other
// channel has published. The IHDR chunk holds width and height at bytes 16–23.
const MAX_ICON_PX = 1024;
const [width, height] = [png.readUInt32BE(16), png.readUInt32BE(20)];
if (width > MAX_ICON_PX || height > MAX_ICON_PX) {
  console.error(`sync-icon: ${SOURCE} is ${width}×${height}; the Marketplace allows at most ${MAX_ICON_PX}×${MAX_ICON_PX}`);
  process.exit(1);
}

try {
  copyFileSync(SOURCE, DEST);
  console.log(`sync-icon: ${SOURCE} -> ${DEST}`);
} catch (err) {
  console.error(`sync-icon: failed to copy icon: ${err.message}`);
  process.exit(1);
}

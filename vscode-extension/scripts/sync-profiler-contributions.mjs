// Writes the shared profiler UI's contributions into package.json
// [PROF-VSCODE-SHARED]. lspkit's one generator produces them for Osprey's
// prefix, so the manifest and the library's command registrations never drift.
// Everything under `osprey.profiler.` belongs to the generator except what
// Osprey's own offline profiler contributes. `--check` exits non-zero instead
// of writing when package.json is out of date.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { profilerContributions } from "@nimblesite/lsp-profiler-ui/scripts/contributions.mjs";

/** Osprey as a profiler host; its runtime has no memory backend [PROF-LSP-RUNTIME]. */
export const PROFILER_HOST = { prefix: "osprey", brand: "Osprey", container: "ospreyDebug", debugType: "osprey", memory: false };

const PREFIX = `${PROFILER_HOST.prefix}.profiler.`;
/** Contributions of Osprey's offline profiler [PROF-VSCODE-FLAME], which the generator never owns. */
const OSPREY_OWNED = new Set([`${PREFIX}openLast`, `${PREFIX}inlineHeat`]);

const generated = (id) => typeof id === "string" && id.startsWith(PREFIX) && !OSPREY_OWNED.has(id);
const replace = (existing = [], fresh, key) => [...existing.filter((entry) => !generated(entry[key])), ...fresh];

/** `contributes` with every generated profiler contribution replaced by the generator's current output. */
export function withProfilerContributions(contributes) {
  const fresh = profilerContributions(PROFILER_HOST);
  const container = PROFILER_HOST.container;
  const menuIds = new Set([...Object.keys(contributes.menus ?? {}), ...Object.keys(fresh.menus)]);
  const menus = Object.fromEntries([...menuIds]
    .map((id) => [id, replace(contributes.menus?.[id], fresh.menus[id] ?? [], "command")])
    .filter(([, entries]) => entries.length > 0));
  const properties = Object.fromEntries(Object.entries(contributes.configuration.properties)
    .filter(([key]) => !generated(key)));
  return {
    ...contributes,
    commands: replace(contributes.commands, fresh.commands, "command"),
    views: { ...contributes.views, [container]: replace(contributes.views?.[container], fresh.views[container], "id") },
    viewsWelcome: replace(contributes.viewsWelcome, fresh.viewsWelcome, "view"),
    menus,
    keybindings: replace(contributes.keybindings, fresh.keybindings, "command"),
    configuration: { ...contributes.configuration, properties: { ...properties, ...fresh.configuration } },
  };
}

const manifestPath = path.join(path.dirname(fileURLToPath(import.meta.url)), "..", "package.json");

/** The manifest `text` with the generator's current contributions, LF-terminated. */
function synced(text) {
  const manifest = JSON.parse(text);
  return `${JSON.stringify({ ...manifest, contributes: withProfilerContributions(manifest.contributes) }, null, 2)}\n`;
}

/** Whether the manifest `text` lacks the current contributions. A Windows
 * checkout has CRLF line endings, which are formatting, not contributions. */
export function needsSync(text) {
  return synced(text) !== text.replace(/\r\n/g, "\n");
}

function main() {
  const current = fs.readFileSync(manifestPath, "utf8");
  if (!needsSync(current)) return;
  if (process.argv.includes("--check")) {
    console.error("package.json is missing the shared profiler contributions: run `npm run sync-profiler`.");
    process.exit(1);
  }
  fs.writeFileSync(manifestPath, synced(current));
  console.log(`wrote the shared profiler contributions into ${manifestPath}`);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) main();

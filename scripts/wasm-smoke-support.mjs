// Shared entry/exit checks for the two wasm smoke tests. [WASM-TARGET]
//
// wasm-smoke.mjs (Node's WASI host) and wasm-browser-smoke.mjs (the inline
// browser shim) differ only in HOW they run a module — the argument handling,
// module validation and golden comparison around that run are one contract, so
// they live here rather than in both scripts.

import { spawnSync } from "node:child_process";
import { readdirSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// `node:wasi` caches the module's memory backing store when the instance starts
// and never refreshes it after `memory.grow`, so every WASI call a growing
// module makes afterwards reads or writes freed memory. On x86_64 that is a
// SIGSEGV inside node — exit 139, no stderr, no wasm trap — and where the stale
// page happens to still be mapped it silently drops the module's output
// instead, which is worse. A 12-line hand-written module that writes, grows and
// writes again reproduces it with no Osprey involved, and Node 24 is the first
// release that runs both it and the assertion corpus clean. A defect in the
// runner must never report itself as a compiler one. [WASM-TARGET]
export const MIN_WASI_NODE_MAJOR = 24;

// Set on the relaunched child so a second too-old hop fails loudly instead of
// recursing.
const RELAUNCHED = "OSPREY_WASI_NODE_RELAUNCHED";

const majorOf = (version) => Number(version.replace(/^v/, "").split(".")[0]);

// The major version `candidate` reports, or 0 when it cannot be run at all.
function reportedMajor(candidate) {
  const probe = spawnSync(candidate, ["--version"], { encoding: "utf8" });
  return probe.status === 0 ? majorOf(probe.stdout.trim()) : 0;
}

// The highest-major nvm-installed Node clearing the floor, or undefined.
function newestInstalledNode() {
  const root = join(homedir(), ".nvm", "versions", "node");
  let best;
  let bestMajor = 0;
  let entries;
  try {
    entries = readdirSync(root);
  } catch {
    return undefined;
  }
  for (const entry of entries) {
    const candidate = join(root, entry, "bin", "node");
    const major = reportedMajor(candidate);
    if (major >= MIN_WASI_NODE_MAJOR && major > bestMajor) {
      bestMajor = major;
      best = candidate;
    }
  }
  return best;
}

/**
 * Re-exec `scriptUrl` under a Node new enough to host WASI, when this one is
 * not. `OSPREY_NODE` names an interpreter explicitly; otherwise the newest
 * qualifying nvm install is used. Exits with the child's status, so a caller
 * that merely types `node scripts/wasm-smoke.mjs` — the Makefile, CI, the
 * corpus harness, the Rust e2e test — needs no version logic of its own. This
 * is the ONE place that policy lives.
 *
 * Returns normally when the running Node already qualifies, and fails with the
 * defect's own explanation when no qualifying interpreter exists.
 *
 * Implements [WASM-TARGET-NODE].
 */
export function ensureWasiCapableNode(scriptUrl) {
  if (majorOf(process.versions.node) >= MIN_WASI_NODE_MAJOR) {
    return;
  }
  const better =
    process.env[RELAUNCHED] === undefined
      ? (process.env.OSPREY_NODE ?? newestInstalledNode())
      : undefined;
  if (better === undefined) {
    console.error(
      `FAIL: node:wasi in Node ${process.versions.node} corrupts a module's memory after ` +
        `memory.grow (use-after-free on its cached backing store). Install Node ` +
        `${MIN_WASI_NODE_MAJOR}+, point OSPREY_NODE at one, or use ` +
        `scripts/wasm-browser-smoke.mjs, which drives the same module through the ` +
        `browser WASI shim and is unaffected.`,
    );
    process.exit(1);
  }
  const relaunch = spawnSync(
    better,
    [fileURLToPath(scriptUrl), ...process.argv.slice(2)],
    { stdio: "inherit", env: { ...process.env, [RELAUNCHED]: "1" } },
  );
  process.exit(relaunch.status ?? 1);
}

// Parse argv, load the module and reject anything malformed. Exits 2 without a
// path (usage error) and 1 when the bytes are not a valid module.
export async function loadModuleFromArgv(usage) {
  const [, , wasmPath, expectedPath] = process.argv;
  if (!wasmPath) {
    console.error(usage);
    process.exit(2);
  }
  const bytes = await readFile(wasmPath);
  if (!WebAssembly.validate(bytes)) {
    console.error(`FAIL: ${wasmPath} is not a valid WebAssembly module`);
    process.exit(1);
  }
  return { wasmPath, expectedPath, bytes };
}

// Compare captured stdout against the golden, trimmed at both ends. No expected
// path means the run itself was the assertion. `prefix` names the host in the
// failure line (e.g. "browser-shim "); it is empty for the plain WASI host.
export async function assertMatchesGolden(expectedPath, captured, prefix) {
  if (!expectedPath) {
    return;
  }
  const expected = (await readFile(expectedPath, "utf8")).trim();
  if (captured.trim() !== expected) {
    console.error(`FAIL: ${prefix}stdout mismatch`);
    console.error(`  expected: ${JSON.stringify(expected)}`);
    console.error(`  actual:   ${JSON.stringify(captured.trim())}`);
    process.exit(1);
  }
}

import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url);
const { compileWasi, runWasi } = require(path.join(extension, "out/client/src/osprey-ui/wasi.js"));
const { parseEnvelope } = require(path.join(extension, "out/client/src/osprey-ui/protocol.js"));
const artifact = process.argv[2] ?? path.join(extension, "bin", `${process.platform}-${process.arch}`, "package-workspace.wasm");
const workbench = path.basename(artifact) === "workbench.wasm";
const name = workbench ? "Workbench" : "Package workspace";
const context = { platform: "vscode", now: 1, offset: 0, home: "/tmp", arguments: ["file:///tmp/project", "http://127.0.0.1:8787"] };
const module = await compileWasi(fs.readFileSync(artifact));
const result = await runWasi(module, JSON.stringify({ operation: "start", context }));
assert.equal(result.exitCode, 0, result.stderr);
assert.ok(result.stdout.trim(), `${name} silently returned no startup envelope`);
const envelope = parseEnvelope(result.stdout);
assert.deepEqual(envelope.patches.map((patch) => patch.key).sort(), workbench
  ? ["catalogue", "document", "generated", "goal", "heading", "lesson", "library", "library-actions", "root"]
  : ["details", "packages", "root"]);
assert.equal(envelope.requests.length, 2, "Startup must request its two initial capabilities");
if (workbench) assert.deepEqual(envelope.requests.map(request => request.kind), ["read-texts", "store-read"]);
assert.ok(envelope.state, "Startup must retain the actual Osprey application state");
assert.deepEqual(envelope.diagnostics, [], `${name} startup reports diagnostics`);
console.log(`${name} WASM startup: PASS`);

// End-to-end acceptance for the Express port: each example is compiled by
// Osprey under GC and ARC, served natively, sent tests/corpus.json over real
// loopback HTTP, and compared with what the pinned real Express replied to the
// same file (tests/examples.expected.json). Run from the repository root:
//   node src/osprey/packages/express/tests/e2e.mjs
// [TS-PORT-EXPRESS-ORACLE], [TS-PORT-EXPRESS]
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, openSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { COMPARED_HEADERS, exchange, listening } from "../oracle/exchange.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const pkg = path.dirname(here);
const root = path.resolve(pkg, "../../../..");
const osprey = process.env.OSPREY ?? path.join(root, "target/debug/osprey");
const out = path.join(root, "target/express");
const PORT = 3000;
const MEMORIES = ["gc", "arc"];

const read = (name) => JSON.parse(readFileSync(path.join(here, name), "utf8"));
const corpus = read("corpus.json");
const expected = read("examples.expected.json");

/** Compile `example` under `memory` and return its native binary. */
function build(example, memory) {
  const binary = path.join(out, `${example}-${memory}`);
  const project = path.join(pkg, "examples", example);
  execFileSync(osprey, [project, "--compile", "--quiet", "--no-warnings", `--memory=${memory}`, "-o", binary], {
    stdio: ["ignore", "inherit", "inherit"],
  });
  return binary;
}

/** The compared reply, with each documented divergence asserted, not skipped. */
function compare(label, want, got) {
  assert.equal(got.status, want.status, `${label}: status`);
  assert.equal(got.body, want.body, `${label}: body`);
  for (const name of COMPARED_HEADERS) {
    assert.equal(got.headers[name], want.headers[name], `${label}: ${name}`);
  }
  // Divergence (README): the port sends no ETag; Express sends a weak one.
  assert.equal(got.etag, false, `${label}: the port must not send an ETag it cannot compute`);
}

async function serve(example, memory) {
  const binary = build(example, memory);
  const log = openSync(path.join(out, `${example}-${memory}.log`), "w");
  const server = spawn(binary, [], { stdio: ["ignore", log, log] });
  let checked = 0;
  try {
    await listening(PORT);
    const recorded = expected.examples[example];
    const requests = corpus.examples[example];
    assert.equal(recorded.length, requests.length, `${example}: the recording is stale; rerun oracle/record.mjs`);
    for (const [index, request] of requests.entries()) {
      assert.deepEqual(recorded[index].request, request, `${example}: the recording is stale; rerun oracle/record.mjs`);
      const label = `${example} [${memory}] ${request.method} ${request.path}`;
      compare(label, recorded[index].reply, await exchange(PORT, request));
      checked += 1;
    }
    assert.equal(server.exitCode, null, `${example} [${memory}]: the server must still be running`);
  } finally {
    server.kill();
    await new Promise((resolve) => server.once("exit", resolve));
  }
  return checked;
}

mkdirSync(out, { recursive: true });
let total = 0;
for (const example of Object.keys(corpus.examples)) {
  for (const memory of MEMORIES) {
    const checked = await serve(example, memory);
    console.log(`ok ${example} [${memory}]: ${checked} replies match Express ${expected.express}`);
    total += checked;
  }
}
console.log(`${total} replies matched, 0 failed`);

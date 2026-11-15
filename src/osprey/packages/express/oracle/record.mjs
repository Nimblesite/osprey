// Runs each example, unchanged, on the pinned real Express under Node.js and
// records its replies in tests/examples.expected.json, the expectations the
// Osprey port is compared with. Run from this directory after `npm ci`:
//   node record.mjs
// [TS-PORT-EXPRESS-ORACLE]
import { spawn } from "node:child_process";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { exchange, listening } from "./exchange.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.dirname(here);
const PORT = 3000;

async function record(example, requests) {
  const run = path.join(here, ".run");
  await rm(run, { recursive: true, force: true });
  await mkdir(run);
  // Node resolves `express` from the file's own directory, so the example is
  // copied byte for byte beside this oracle's node_modules.
  await copyFile(path.join(root, "examples", example, "index.ts"), path.join(run, "index.ts"));
  const server = spawn(process.execPath, ["index.ts"], { cwd: run, stdio: "ignore" });
  try {
    await listening(PORT);
    const replies = [];
    for (const request of requests) {
      replies.push({ request, reply: await exchange(PORT, request) });
    }
    return replies;
  } finally {
    server.kill();
    await new Promise((resolve) => server.once("exit", resolve));
    await rm(run, { recursive: true, force: true });
  }
}

const corpus = JSON.parse(await readFile(path.join(root, "tests", "corpus.json"), "utf8"));
const express = createRequire(import.meta.url)("express/package.json").version;
const examples = {};
for (const [example, requests] of Object.entries(corpus.examples)) {
  examples[example] = await record(example, requests);
}
const recorded = {
  spec: "[TS-PORT-EXPRESS-ORACLE]",
  description: "Replies of the pinned real Express to tests/corpus.json. Written by oracle/record.mjs; never edited by hand.",
  express,
  node: process.version,
  examples,
};
await writeFile(path.join(root, "tests", "examples.expected.json"), `${JSON.stringify(recorded, null, 2)}\n`);
console.log(`recorded ${Object.values(examples).flat().length} replies from Express ${express} on Node.js ${process.version}`);

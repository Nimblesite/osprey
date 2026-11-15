// Runs each example, unchanged, on the pinned real Express under Node.js and
// records its replies in tests/examples.expected.json, the expectations the
// Osprey port is compared with. Run from this directory after `npm ci`:
//   node record.mjs
// [TS-PORT-EXPRESS-ORACLE]
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { entry, exchange, listening } from "./exchange.mjs";

const require = createRequire(import.meta.url);
const express = require("express/package.json").version;
const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.dirname(here);
const PORT = 3000;

async function record(example, requests) {
  const run = path.join(here, ".run");
  await rm(run, { recursive: true, force: true });
  await mkdir(run);
  const server = spawn(process.execPath, [await entry(example, run)], { cwd: run, stdio: "ignore" });
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

/** The hello-world app in the pinned package's own README, as published. */
async function readmeExample() {
  const readme = await readFile(require.resolve("express/Readme.md"), "utf8");
  const block = /```js\n([\s\S]*?)```/.exec(readme);
  if (!block) throw new Error("the pinned Express README has no js example");
  return block[1];
}

/** `examples/hello-world/index.ts` is that README example, byte for byte. */
async function unchangedExample() {
  const published = await readmeExample();
  const ours = await readFile(path.join(root, "examples", "hello-world", "index.ts"), "utf8");
  if (ours !== published) throw new Error("examples/hello-world/index.ts differs from the Express README example");
  return { example: "hello-world", from: `express@${express} Readme.md`, sha256: createHash("sha256").update(ours).digest("hex") };
}

const corpus = JSON.parse(await readFile(path.join(root, "tests", "corpus.json"), "utf8"));
const examples = {};
for (const [example, requests] of Object.entries(corpus.examples)) {
  examples[example] = await record(example, requests);
}
const recorded = {
  spec: "[TS-PORT-EXPRESS-ORACLE]",
  description: "Replies of the pinned real Express to tests/corpus.json. Written by oracle/record.mjs; never edited by hand.",
  express,
  node: process.version,
  unchanged: await unchangedExample(),
  examples,
};
await writeFile(path.join(root, "tests", "examples.expected.json"), `${JSON.stringify(recorded, null, 2)}\n`);
console.log(`recorded ${Object.values(examples).flat().length} replies from Express ${express} on Node.js ${process.version}`);

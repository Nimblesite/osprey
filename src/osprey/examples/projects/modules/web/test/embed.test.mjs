import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { embed } from "../scripts/embed.mjs";

async function embedFixture() {
  const directory = await mkdtemp(path.join(tmpdir(), "osprey-web-"));
  const javascriptPath = path.join(directory, "host.js");
  const cssPath = path.join(directory, "host.css");
  const wasmPath = path.join(directory, "client.wasm");
  const outputPath = path.join(directory, "bundle.ospml");
  await Promise.all([
    writeFile(javascriptPath, 'const wasm="__OSPREY_WASM_BASE64__";const label="route";'),
    writeFile(cssPath, ':root{color:#123}'),
    writeFile(wasmPath, Buffer.from([0, 97, 115, 109])),
  ]);
  return { javascriptPath, cssPath, wasmPath, outputPath };
}

async function embedsRawAssets() {
  const fixture = await embedFixture();
  await embed(fixture);
  const generated = await readFile(fixture.outputPath, "utf8");
  assert.match(generated, /namespace "bank\/web"/);
  assert.match(generated, /style \(\) = ":root\{color:#123\}"/);
  assert.match(generated, /AGFzbQ==/);
  assert.doesNotMatch(generated, /<style>|<script>/);
  assert.doesNotMatch(generated, /__OSPREY_WASM_BASE64__/);
}

test("embeds wasm into raw generated JS and CSS assets", embedsRawAssets);

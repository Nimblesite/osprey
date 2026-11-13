#!/usr/bin/env node
// Reads the generated layout header at build time and writes the constants the host and
// webview compile against. The fingerprint is never copied by hand: a runtime built for another
// layout refuses to open, and this file names the one this build speaks.
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const header = path.resolve(extension, "..", "compiler", "runtime", "native_abi.h");
const target = path.join(extension, "client", "src", "osprey-ui", "generated", "native-abi.ts");

function readHeader() {
  try {
    return fs.readFileSync(header, "utf8");
  } catch {
    throw new Error(`The layout header is missing: ${header}`);
  }
}

function fingerprint(source) {
  const found = /#define TD_SHARED_FINGERPRINT_32 "([0-9a-f]+)"/.exec(source);
  if (!found) throw new Error(`No TD_SHARED_FINGERPRINT_32 in ${header}`);
  return found[1];
}

function storage(source, name) {
  const found = new RegExp(`TD_SHARED_STORAGE\\(${name}, (\\d+)\\);`).exec(source);
  if (!found) throw new Error(`No ${name} layout in ${header}`);
  return Number(found[1]);
}

function offsets(source, name) {
  const pattern = new RegExp(`#define TD_${name}_([A-Za-z]+)_OFFSET (\\d+)`, "g");
  const fields = {};
  for (const match of source.matchAll(pattern)) fields[match[1]] = Number(match[2]);
  if (Object.keys(fields).length === 0) throw new Error(`No ${name} fields in ${header}`);
  return fields;
}

function record(fields) {
  return `  ${Object.entries(fields).map(([field, offset]) => `${field}: ${offset}`).join(", ")},`;
}

const source = readHeader();
const lines = [
  "// Generated from compiler/runtime/native_abi.h (its wasm32 fingerprint) by scripts/generate-native-abi.mjs.",
  "// Do not edit; run `npm run compile` (or `npm run generate-abi`) instead.",
  "",
  `export const SHARED_FINGERPRINT = "${fingerprint(source)}";`,
  `export const SCENE_SHAPE_BYTES = ${storage(source, "SceneShape")};`,
  `export const VIEW_ROW_BYTES = ${storage(source, "ViewRow")};`,
  `export const ASK_ROW_BYTES = ${storage(source, "AskRow")};`,
  "",
  "export const SCENE_SHAPE = {",
  record(offsets(source, "SceneShape")),
  "} as const;",
  "",
  "export const VIEW_ROW = {",
  record(offsets(source, "ViewRow")),
  "} as const;",
  "",
  "export const ASK_ROW = {",
  record(offsets(source, "AskRow")),
  "} as const;",
  "",
];
fs.mkdirSync(path.dirname(target), { recursive: true });
fs.writeFileSync(target, lines.join("\n"));

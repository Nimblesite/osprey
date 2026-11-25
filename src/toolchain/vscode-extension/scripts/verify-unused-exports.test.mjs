// [LINTS-DEADCODE-REACH] Exercise the same command as the extension lint.
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const analyzer = fileURLToPath(new URL("./verify-unused-exports.mjs", import.meta.url));

test("test references, recursion, unused aliases, and type queries cannot keep an export alive", () => {
  const root = mkdtempSync(join(tmpdir(), "osprey-ts-exports-"));
  const write = (file, text) => {
    const path = join(root, file);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, text);
  };
  try {
    write("package.json", '{"main":"./out/client/src/entry.js"}');
    write("tsconfig.json", '{"include":["client/src/**/*.ts","test/**/*.ts"]}');
    const entry = 'import { forwarded as alias } from "./aliases";\nexport function activate() { return alias(); }\nexport function deactivate() {}';
    write("client/src/entry.ts", entry + "\nexport function unusedEntryExport() { return 9; }");
    write("client/src/aliases.ts", 'export { live as forwarded, onlyTested as legacy } from "./values";\nimport type { typeQuery } from "./values";\nexport type Shape = typeof typeQuery;');
    write("client/src/values.ts", [
      "export function onlyTested() { return 1; }",
      "export function recursive(): number { return recursive(); }",
      "export function live() { return 2; }",
      "export function typeQuery() { return 3; }",
    ].join("\n"));
    write("test/calls.ts", 'import { legacy } from "../client/src/aliases";\nlegacy();');
    const red = spawnSync(process.execPath, [analyzer, root], { encoding: "utf8", timeout: 15_000 });
    assert.equal(red.status, 1, red.error?.message ?? red.stderr);
    assert.deepEqual(red.stderr.trim().replaceAll("\\", "/").split(/\r?\n/).sort(), [
      "client/src/entry.ts:4  unusedEntryExport",
      "client/src/values.ts:1  onlyTested",
      "client/src/values.ts:2  recursive",
      "client/src/values.ts:4  typeQuery",
    ]);
    write("client/src/entry.ts", entry);
    write("client/src/aliases.ts", 'export { live as forwarded } from "./values";');
    write("client/src/values.ts", "export function live() { return 2; }");
    write("test/calls.ts", 'import { live } from "../client/src/values";\nlive();');
    const green = spawnSync(process.execPath, [analyzer, root], { encoding: "utf8", timeout: 15_000 });
    assert.equal(green.status, 0, green.error?.message ?? green.stderr);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

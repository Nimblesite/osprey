// Unit tests for the shared test-environment resolvers in ./osprey-test-env.
// These are pure PATH/filesystem walks, so they are exercised directly here
// (no VS Code host needed) — the resolveOspreyOnPath body was otherwise never
// called by any suite, leaving its branch uncovered.

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import {
  extensionRoot,
  resolveBuiltOsprey,
  resolveOspreyOnPath,
  resolveRequiredLldbDap,
} from "./osprey-test-env";

// The staged `osprey` binary is the exe name the resolvers look for.
const OSPREY_EXE = process.platform === "win32" ? "osprey.exe" : "osprey";

suite("Osprey Test-Env Resolver Unit Tests", () => {
  // A throwaway PATH entry holding a fake `osprey`, restored after each case so
  // no later suite inherits a doctored PATH.
  let priorPath: string | undefined;
  let scratch: string;

  setup(() => {
    priorPath = process.env.PATH;
    scratch = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-env-"));
  });

  teardown(() => {
    process.env.PATH = priorPath;
    fs.rmSync(scratch, { recursive: true, force: true });
  });

  test("resolveOspreyOnPath finds the binary in a PATH directory and skips empties", () => {
    const staged = path.join(scratch, OSPREY_EXE);
    fs.writeFileSync(staged, "#!/bin/sh\n");
    fs.chmodSync(staged, 0o755);

    // Lead with an empty segment (the `if (!dir) continue` branch) and a
    // directory that does NOT hold osprey, so the walk has to skip both before
    // it lands on `scratch`.
    const emptyDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-empty-"));
    process.env.PATH = ["", emptyDir, scratch].join(path.delimiter);

    const found = resolveOspreyOnPath();
    assert.strictEqual(
      found,
      staged,
      "osprey resolved from the PATH entry that has it",
    );
    fs.rmSync(emptyDir, { recursive: true, force: true });
  });

  test("resolveOspreyOnPath returns undefined when no PATH entry has osprey", () => {
    process.env.PATH = scratch; // scratch is empty — no osprey binary staged
    assert.strictEqual(
      resolveOspreyOnPath(),
      undefined,
      "an osprey-less PATH resolves to undefined",
    );
  });

  test("resolveBuiltOsprey prefers the repo release binary, else falls back to PATH", () => {
    const built = path.resolve(
      extensionRoot,
      "..",
      "target",
      "release",
      OSPREY_EXE,
    );
    const resolved = resolveBuiltOsprey();

    if (fs.existsSync(built)) {
      // The made-from-source compiler is preferred over anything on PATH.
      assert.strictEqual(
        resolved,
        built,
        "uses the freshly-built repo compiler",
      );
    } else {
      // No built binary: it must degrade to the PATH lookup (whatever that is,
      // possibly undefined) — never the non-existent built path.
      assert.notStrictEqual(
        resolved,
        built,
        "does not return a non-existent built path",
      );
      assert.strictEqual(
        resolved,
        resolveOspreyOnPath(),
        "falls back to the PATH resolver verbatim",
      );
    }
  });

  test("resolveRequiredLldbDap returns an executable path or fails loudly", () => {
    // Drives resolveRequiredLldbDap's body regardless of host: on CI lldb-dap is
    // installed (the debugger E2E needs it) so it returns an existing path; on a
    // machine without it, the resolver's assert.fail branch throws with the
    // documented message. Both outcomes are asserted as correct here.
    try {
      const command = resolveRequiredLldbDap();
      assert.ok(
        typeof command === "string" &&
          command.length > 0 &&
          fs.existsSync(command),
        `resolved lldb-dap must be a real executable path, got "${command}"`,
      );
    } catch (error) {
      assert.match(
        String(error),
        /lldb-dap is required/,
        "the fail branch surfaces the documented 'required' message",
      );
    }
  });
});

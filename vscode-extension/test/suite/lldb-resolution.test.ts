import * as assert from "assert";
import * as path from "path";
import * as fs from "fs";
import { extensionRoot } from "./osprey-test-env";
import { resolveDebugConsole } from "../../client/src/debug-console";
import {
  shipwrightPlatform,
  resolveLldbDapCommand,
  resolveLldbDapExecutable,
  missingLldbDapMessage,
} from "../../client/src/extension";

// Every arm of the debug-adapter search, driven through the injectable host
// rather than the machine this suite happens to run on. A VSIX ships for
// platforms CI never boots, and the search order — launch config, VS Code
// setting, PATH, xcrun, common install paths — is the part users hit when a
// debug session refuses to start, so each arm needs an assertion of its own
// rather than whichever one the host resolves first.
//
// These live outside extension.test.ts deliberately: that file is already
// several times the size a source file in this repo is allowed to be, and
// bolting another dozen cases onto it makes the situation worse.

type Host = Parameters<typeof resolveLldbDapExecutable>[1];

/// A resolution host that finds exactly the paths in `present` and nothing else.
function hostFinding(
  platform: NodeJS.Platform,
  pathVar: string,
  present: string[],
  extras: Partial<NonNullable<Host>> = {},
): NonNullable<Host> {
  return {
    env: { PATH: pathVar },
    existsSync: (filePath: string) => present.includes(filePath),
    readDir: () => [],
    getSetting: () => undefined,
    platform,
    ...extras,
  };
}

suite("Osprey lldb-dap Resolution Unit Tests", () => {
  test("internal console is the default and requires no adapter probe", async () => {
    const forbiddenProbe = async () => { throw new Error("internal launch must not probe"); };
    for (const config of [{}, { console: "internalConsole" }]) {
      assert.strictEqual(await resolveDebugConsole(config, "lldb", forbiddenProbe), "internalConsole");
    }
  });

  test("terminal selection requires a verified compatible adapter", async () => {
    for (const console of ["integratedTerminal", "externalTerminal"] as const) {
      for (const version of ["lldb-dap: Ubuntu LLVM version 21.1.8", "lldb version 22.0.1"]) {
        assert.strictEqual(await resolveDebugConsole({ console }, "selected-adapter", async command => {
          assert.strictEqual(command, "selected-adapter");
          return version;
        }), console);
      }
      for (const version of ["lldb version 20.1.0", "lldb-1600.0.1", "unknown", ""]) {
        await assert.rejects(resolveDebugConsole({ console }, "old-adapter", async () => version), /requires LLDB-DAP 21 or newer.*old-adapter/);
      }
    }
  });

  test("invalid or conflicting console options are rejected before probing", async () => {
    const forbiddenProbe = async () => { throw new Error("invalid config must not probe"); };
    for (const console of [null, "terminal", 21, false, { toString: () => "internalConsole" }]) {
      await assert.rejects(resolveDebugConsole({ console }, "lldb", forbiddenProbe), /console must be/);
    }
    await assert.rejects(resolveDebugConsole({ runInTerminal: true }, "lldb", forbiddenProbe), /Use console: integratedTerminal/);
    await assert.rejects(resolveDebugConsole({ console: "externalTerminal", launchCommands: ["run"] }, "lldb", forbiddenProbe), /cannot be combined/);
    assert.strictEqual(await resolveDebugConsole({ console: "internalConsole", launchCommands: ["run"] }, "lldb", forbiddenProbe), "internalConsole");
    assert.strictEqual(await resolveDebugConsole({ console: "integratedTerminal", launchCommands: [] }, "lldb", async () => "lldb version 21.1.8"), "integratedTerminal");
  });

  test("adapter probe failures reject the requested terminal", async () => {
    await assert.rejects(resolveDebugConsole({ console: "integratedTerminal" }, "missing-adapter", async () => { throw new Error("probe failed"); }), /probe failed/);
    await assert.rejects(resolveDebugConsole({ console: "externalTerminal" }, path.join(extensionRoot, "nonexistent-adapter")), /Cannot determine LLDB-DAP terminal support/);
  });

  test("debug launch schema exposes the three console policies", () => {
    const manifest = JSON.parse(fs.readFileSync(path.join(extensionRoot, "package.json"), "utf8"));
    const launch = manifest.contributes.debuggers.find((item: { type: string }) => item.type === "osprey").configurationAttributes.launch;
    assert.deepStrictEqual(launch.properties.console, {
      type: "string",
      enum: ["internalConsole", "integratedTerminal", "externalTerminal"],
      default: "internalConsole",
      description: "Program input/output destination. Terminal modes require LLDB-DAP 21 or newer; the Debug Console does not provide stdin.",
    });
  });
  test("shipwrightPlatform maps every supported platform and architecture", () => {
    const cases: Array<[NodeJS.Platform, string, string]> = [
      ["win32", "x64", "win32-x64"],
      ["win32", "arm64", "win32-arm64"],
      ["darwin", "arm64", "darwin-arm64"],
      ["darwin", "x64", "darwin-x64"],
      ["linux", "x64", "linux-x64"],
      ["linux", "arm64", "linux-arm64"],
      // A triple the VSIX is not staged for collapses onto the linux/x64
      // defaults instead of naming a directory that cannot exist.
      ["freebsd", "ppc64", "linux-x64"],
    ];
    for (const [platform, arch, expected] of cases) {
      assert.strictEqual(
        shipwrightPlatform(platform, arch),
        expected,
        `${platform}/${arch} maps to ${expected}`,
      );
    }
  });

  test("a bare configured command is resolved through PATH", () => {
    const found = path.join("/second", "mydap");
    // The empty middle entry must be skipped, not resolved against the cwd.
    const host = hostFinding(
      "linux",
      ["/first", "", "/second"].join(path.delimiter),
      [found],
    );
    assert.strictEqual(
      resolveLldbDapExecutable({ lldbDapPath: "mydap" }, host),
      found,
    );
  });

  test("a bare configured command missing from PATH resolves to nothing", () => {
    const host = hostFinding("linux", "/first", []);
    assert.strictEqual(
      resolveLldbDapExecutable({ lldbDapPath: "mydap" }, host),
      undefined,
      "a name PATH cannot answer is not silently replaced by a default",
    );
  });

  test("PATH is consulted before xcrun and the install paths", () => {
    const found = path.join("/tools", "lldb-dap");
    const host = hostFinding("darwin", "/tools", [found], {
      execFileSync: () => {
        throw new Error("xcrun must not run when PATH already answers");
      },
    });
    assert.strictEqual(resolveLldbDapExecutable({}, host), found);
  });

  test("the legacy lldb-vscode name is accepted from PATH", () => {
    const found = path.join("/tools", "lldb-vscode");
    const host = hostFinding("linux", "/tools", [found]);
    assert.strictEqual(resolveLldbDapExecutable({}, host), found);
  });

  test("Linux finds the newest installed versioned lldb-dap", () => {
    const found = path.join("/tools", "lldb-dap-21");
    const host = hostFinding("linux", "/tools", [found, "/tools/lldb-dap-19"], {
      readDir: () => ["lldb-dap-19", "lldb-dap-21", "lldb-dap-old"],
    });
    assert.strictEqual(resolveLldbDapExecutable({}, host), found);
  });

  test("a host with no PATH variable at all resolves rather than throwing", () => {
    const host: NonNullable<Host> = {
      env: {},
      existsSync: () => false,
      getSetting: () => undefined,
      platform: "linux",
    };
    assert.strictEqual(resolveLldbDapExecutable({}, host), undefined);
  });

  test("xcrun answers on darwin when it names a real file", () => {
    const host = hostFinding("darwin", "", ["/xcode/lldb-dap"], {
      execFileSync: () => "  /xcode/lldb-dap\n",
    });
    assert.strictEqual(resolveLldbDapExecutable({}, host), "/xcode/lldb-dap");
  });

  test("an xcrun answer naming nothing on disk is discarded", () => {
    const host = hostFinding("darwin", "", ["/usr/bin/lldb-dap"], {
      execFileSync: () => "/gone/lldb-dap\n",
    });
    assert.strictEqual(resolveLldbDapExecutable({}, host), "/usr/bin/lldb-dap");
  });

  test("an xcrun that fails falls through to the install paths", () => {
    const found = "/opt/homebrew/opt/llvm/bin/lldb-dap";
    const host = hostFinding("darwin", "", [found], {
      execFileSync: () => {
        throw new Error("xcrun: not found");
      },
    });
    assert.strictEqual(resolveLldbDapExecutable({}, host), found);
  });

  test("an empty xcrun answer falls through to the install paths", () => {
    const found = "/usr/bin/lldb-vscode";
    const host = hostFinding("darwin", "", [found], {
      execFileSync: () => "\n",
    });
    assert.strictEqual(resolveLldbDapExecutable({}, host), found);
  });

  test("xcrun is never consulted off darwin", () => {
    const found = "/usr/bin/lldb-dap";
    const host = hostFinding("linux", "", [found], {
      execFileSync: () => {
        throw new Error("xcrun is a darwin tool");
      },
    });
    assert.strictEqual(resolveLldbDapExecutable({}, host), found);
  });

  test("windows uses the .exe names and the LLVM install paths", () => {
    const found = "C:\\Program Files\\LLVM\\bin\\lldb-dap.exe";
    assert.strictEqual(
      resolveLldbDapExecutable({}, hostFinding("win32", "", [found])),
      found,
    );
    assert.strictEqual(
      resolveLldbDapExecutable(
        {},
        hostFinding("win32", "", [
          "C:\\Program Files\\LLVM\\bin\\lldb-vscode.exe",
        ]),
      ),
      "C:\\Program Files\\LLVM\\bin\\lldb-vscode.exe",
      "the legacy adapter is still accepted from its install path",
    );
    assert.strictEqual(
      resolveLldbDapCommand({}, hostFinding("win32", "", [])),
      "lldb-dap.exe",
      "the fallback command name carries the windows extension",
    );
  });

  test("the VS Code setting supplies the path when the launch config does not", () => {
    const configured = "/from/setting/lldb-dap";
    const host = hostFinding("linux", "", [configured], {
      getSetting: () => configured,
    });
    assert.strictEqual(resolveLldbDapExecutable({}, host), configured);
  });

  test("the launch config outranks the VS Code setting", () => {
    const host = hostFinding("linux", "", ["/from/config/lldb-dap"], {
      getSetting: () => "/from/setting/lldb-dap",
    });
    assert.strictEqual(
      resolveLldbDapExecutable({ lldbDapPath: "/from/config/lldb-dap" }, host),
      "/from/config/lldb-dap",
    );
  });

  test("the missing-adapter message names a configured path only when there is one", () => {
    assert.ok(
      !/Configured lldbDapPath/.test(missingLldbDapMessage()),
      "with nothing configured the message names no path",
    );
    assert.match(
      missingLldbDapMessage({ lldbDapPath: "/missing/lldb-dap" }),
      /Configured lldbDapPath: \/missing\/lldb-dap\./,
    );
  });
});

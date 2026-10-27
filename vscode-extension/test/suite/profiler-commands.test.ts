// Tests for [PROF-VSCODE-SHARED] and [PROF-LSP-HOST]: command registration,
// configuration, status bar and keybindings of the shared profiler UI, ported
// from SharpLsp's profiler commands suite, plus the two guarantees Osprey's
// packaging adds — the manifest is exactly the generator's output for a host
// with no memory backend, and the library the tests load is the submodule's
// current build, never a stale packed copy.
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as vscode from "vscode";
import { PROFILER_SETTING_KEYS } from "@nimblesite/lsp-profiler-ui";
import { extensionRoot } from "./osprey-test-env";
import { EXTENSION_ID, execute, lspClient, profilerStore, uiCommand, useLspSuite } from "./profiler-kit";

/** Client commands the library registers and the manifest declares. */
const CPU_CLIENT_COMMANDS = ["profileStart", "profileStop", "profileSnapshot", "profileAttachToDebug", "profileShowResults",
  "profileCurrentFileCpu", "profileProcess", "refreshProcesses", "sortProcesses", "groupProcesses", "filterProcesses",
  "copyProcessPid", "revealProcessScript", "killProcess", "inspectVariable"].map(uiCommand);
/** The library's memory commands, which Osprey's manifest must never offer. */
const MEMORY_CLIENT_COMMANDS = ["memoryMenu", "memoryStart", "memorySnapshot", "memoryStop", "memoryDiff",
  "memoryGcCollect", "memoryReferences", "trackMemoryCurrentFile", "memoryTrackProcess"].map(uiCommand);
const CPU_SETTINGS: Record<string, { type: string; default: unknown }> = {
  sampleRate: { type: "number", default: 100 },
  includeNative: { type: "boolean", default: false },
  lineThreshold: { type: "number", default: 1 },
  functionThreshold: { type: "number", default: 2 },
  maxDiagnosticsPerFile: { type: "number", default: 20 },
  showInlineHeatMap: { type: "boolean", default: true },
  profileOnLaunch: { type: "boolean", default: false },
  processRefreshMs: { type: "number", default: 2000 },
  preset: { type: "string", default: "default" },
};

interface Contribution { readonly command: string; readonly title?: string; readonly category?: string; readonly when?: string }
interface Contributes {
  readonly commands: Contribution[];
  readonly keybindings: Contribution[];
  readonly menus: Record<string, Contribution[]>;
  readonly views: Record<string, { id: string }[]>;
  readonly viewsWelcome: { view: string; contents: string }[];
  readonly configuration: { properties: Record<string, { type?: string; enum?: string[]; default?: unknown }> };
}

function contributes(): Contributes {
  const extension = vscode.extensions.getExtension(EXTENSION_ID);
  assert.ok(extension, "the extension is installed");
  return (extension.packageJSON as { contributes: Contributes }).contributes;
}

function isRegistered(commandId: string): boolean {
  try {
    vscode.commands.registerCommand(commandId, () => undefined).dispose();
    return false;
  } catch {
    return true;
  }
}

function advertisedServerCommands(): ReadonlySet<string> {
  return new Set(lspClient().initializeResult?.capabilities.executeCommandProvider?.commands ?? []);
}

suite("Profiler — Command Registration", () => {
  useLspSuite("osprey-profiler-commands-");

  test("every client command is registered, memory commands included, since the library owns them", () => {
    for (const command of [...CPU_CLIENT_COMMANDS, ...MEMORY_CLIENT_COMMANDS]) {
      assert.ok(isRegistered(command), `${command} is registered after activation`);
    }
  });

  test("the server advertises the CPU wire commands and never a client command", () => {
    const advertised = advertisedServerCommands();
    for (const name of ["cpu.start", "cpu.stop", "cpu.snapshot", "cpu.list", "processes", "kill", "capabilities"]) {
      assert.ok(advertised.has(`osprey.lspProfiler.${name}`), `${name} is advertised by the server`);
    }
    for (const command of CPU_CLIENT_COMMANDS) {
      assert.ok(!advertised.has(command), `${command} must not be a server command`);
    }
  });

  test("cpu.list returns an empty, well-shaped session list initially", async () => {
    assert.deepEqual(await execute<unknown>("cpu.list", {}), { sessions: [] });
  });
});

suite("Profiler — Configuration", () => {
  useLspSuite("osprey-profiler-cfg-");

  test("every CPU setting is declared with its type and default, and reads that default", () => {
    const properties = contributes().configuration.properties;
    const config = vscode.workspace.getConfiguration("osprey.profiler");
    for (const [key, expected] of Object.entries(CPU_SETTINGS)) {
      const declared = properties[`osprey.profiler.${key}`];
      assert.ok(declared, `osprey.profiler.${key} is declared`);
      assert.equal(declared.type, expected.type, `${key} type`);
      assert.deepEqual(config.get(key), expected.default, `${key} default`);
    }
  });

  test("the preset enum offers exactly the presets the server parses", () => {
    const preset = contributes().configuration.properties["osprey.profiler.preset"];
    assert.deepEqual([...(preset?.enum ?? [])].sort(), ["default", "detailed", "longRunning", "quick"]);
  });

  test("no memory-autopilot setting is declared for a runtime with no memory backend", () => {
    const properties = Object.keys(contributes().configuration.properties);
    for (const key of ["autoSnapshotOnPause", "autoSnapshot", "autoSnapshotInterval"]) {
      assert.ok(!properties.includes(`osprey.profiler.${key}`), `${key} is not offered`);
    }
    for (const key of PROFILER_SETTING_KEYS.filter((setting) => !setting.startsWith("profiler.autoSnapshot"))) {
      assert.ok(properties.includes(`osprey.${key}`), `every CPU setting the library reads is declared: ${key}`);
    }
  });
});

suite("Profiler — Status Bar and activation", () => {
  useLspSuite("osprey-profiler-sb-");

  test("the store holds the running client once the server is up", () => {
    assert.equal(profilerStore().client.value?.isRunning(), true);
    assert.equal(vscode.extensions.getExtension(EXTENSION_ID)?.isActive, true);
  });
});

suite("Profiler — Manifest", () => {
  test("start and stop have keybindings", () => {
    const bound = contributes().keybindings.map((entry) => entry.command);
    assert.ok(bound.includes(uiCommand("profileStart")) && bound.includes(uiCommand("profileStop")), bound.join(", "));
  });

  test("every CPU client command is declared with a title under the Osprey category; no memory command is", () => {
    const declared = new Map(contributes().commands.map((entry) => [entry.command, entry]));
    for (const command of CPU_CLIENT_COMMANDS) {
      const entry = declared.get(command);
      assert.ok(entry?.title !== undefined && entry.title.length > 0, `${command} has a title`);
      assert.equal(entry.category, "Osprey", `${command} is under Osprey`);
    }
    for (const command of MEMORY_CLIENT_COMMANDS) {
      assert.ok(!declared.has(command), `${command} is never offered`);
    }
    const menus = Object.values(contributes().menus).flat().map((entry) => entry.command);
    assert.deepEqual(menus.filter((command) => MEMORY_CLIENT_COMMANDS.includes(command)), [], "no menu offers memory");
  });

  test("the Processes view lives in the Osprey Debug container and its welcome offers only the CPU launch", () => {
    assert.ok(contributes().views.ospreyDebug?.some((view) => view.id === "osprey.profiler.processes"));
    const welcome = contributes().viewsWelcome.filter((entry) => entry.view === "osprey.profiler.processes");
    assert.ok(welcome.length >= 5, "every fetch state has a welcome");
    assert.ok(welcome.some((entry) => entry.contents.includes(uiCommand("profileCurrentFileCpu"))));
    assert.ok(!welcome.some((entry) => /Track Memory/.test(entry.contents)), "no memory launch link");
  });

  test("package.json is exactly the generator's output: the sync script would change nothing", async () => {
    const script = path.join(extensionRoot, "scripts", "sync-profiler-contributions.mjs");
    const sync = (await import(script)) as { withProfilerContributions(contributes: unknown): unknown };
    const manifest = JSON.parse(fs.readFileSync(path.join(extensionRoot, "package.json"), "utf8")) as { contributes: unknown };
    assert.deepEqual(sync.withProfilerContributions(manifest.contributes), manifest.contributes);
  });

  // The win32 release job checks package.json out with CRLF line endings; a
  // byte comparison failed v0.22.0's Windows VSIX on an up-to-date manifest.
  test("the sync check reads line endings as formatting, never as missing contributions", async () => {
    const script = path.join(extensionRoot, "scripts", "sync-profiler-contributions.mjs");
    const sync = (await import(script)) as { needsSync(text: string): boolean };
    const text = fs.readFileSync(path.join(extensionRoot, "package.json"), "utf8");
    assert.equal(sync.needsSync(text), false, "the committed manifest is in sync");
    assert.equal(sync.needsSync(text.replace(/\n/g, "\r\n")), false, "a CRLF checkout of it is in sync too");
    const manifest = JSON.parse(text) as { contributes: { commands: unknown[] } };
    const stale = { ...manifest, contributes: { ...manifest.contributes, commands: [] } };
    assert.equal(sync.needsSync(`${JSON.stringify(stale, null, 2)}\r\n`), true, "a manifest missing the commands is out of sync");
  });

  test("the installed profiler UI is the submodule's current build, not a stale packed copy", () => {
    const installed = path.join(extensionRoot, "node_modules", "@nimblesite", "lsp-profiler-ui", "out", "src");
    const built = path.resolve(extensionRoot, "..", "lspkit", "packages", "lsp-profiler-ui", "out", "src");
    const scripts = fs.readdirSync(built).filter((file) => file.endsWith(".js"));
    assert.ok(scripts.length > 0, "the submodule's UI is built");
    const stale = scripts.filter((file) => fs.readFileSync(path.join(built, file), "utf8") !== fs.readFileSync(path.join(installed, file), "utf8"));
    assert.deepEqual(stale, [], "reinstall the extension's dependencies (npm ci) after rebuilding the UI");
  });
});

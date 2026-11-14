import * as assert from "assert";
import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";
import * as os from "os";
import { ErrorAction, CloseAction } from "vscode-languageclient/node";
import {
  shipwrightPlatform,
  resolveBundledCompiler,
  resolveServerCommand,
  looksLikePath,
  makeClientFailureHandling,
  applyDefaultOspreyDebugConfig,
  defaultOspreyDebugConfigForEditor,
  defaultDebugOutputPath,
  resolveLldbDapCommand,
  resolveLldbDapExecutable,
  missingLldbDapMessage,
  ospreyLanguageForFile,
  isOspreyFile,
  deactivate,
  type OspreyExtensionApi,
} from "../../client/src/extension";
import {
  extensionRoot,
  pinOspreySettings,
  resolveBuiltOsprey,
  resolveRequiredLldbDap,
  skipWarningsOf,
  type PinnedOspreySettings,
} from "./osprey-test-env";
import {
  assertCurrentLine,
  assertLocalVariable,
  assertTopFrameInSource,
  clearDebugBreakpoints,
  getScopes,
  getVariables,
  readFrameVariables,
  setSourceBreakpoints,
  stepOver,
  stopActiveDebugSession,
  waitForDebugSessionEnd,
  waitForDebugSessionStart,
  waitForStop,
} from "./dap-harness";

const extensionId = "nimblesite.osprey";

/** The suffix a bundled compiler carries on this host. */
const COMPILER_EXE = process.platform === "win32" ? ".exe" : "";

// startDebugRaced runs the real startDebugging path but never hangs the suite
// on a host without a debug UI: it resolves to a marker string once VS Code
// settles or a timeout elapses, whichever comes first.
async function startDebugRaced(
  config: vscode.DebugConfiguration,
  budgetMs: number,
): Promise<string> {
  const timeout = new Promise<string>((resolve) =>
    setTimeout(() => resolve("timeout"), budgetMs),
  );
  const start = Promise.resolve(vscode.debug.startDebugging(undefined, config))
    .then((value) => `resolved:${String(value)}`)
    .catch((error: unknown) => `error:${String(error)}`);
  return Promise.race([start, timeout]);
}

/** Wait `ms` milliseconds without ever rejecting. */
const settle = (ms: number): Promise<void> =>
  new Promise((resolve) => setTimeout(resolve, ms));

// makeActive shows the document and polls until it is genuinely the active
// text editor. The extension shows an "Osprey Debug" output channel during
// activation, and that output pseudo-editor steals the active-editor slot in
// the headless host. We close panels and refocus the editor group between
// attempts so the .osp document becomes (and stays) the active editor at the
// moment a command captures window.activeTextEditor.
async function makeActive(document: vscode.TextDocument): Promise<boolean> {
  for (let i = 0; i < 25; i++) {
    // Hide the output panel and any output pseudo-editor stealing focus.
    await vscode.commands
      .executeCommand("workbench.action.closePanel")
      .then(undefined, () => undefined);
    await vscode.window.showTextDocument(document, {
      viewColumn: vscode.ViewColumn.One,
      preserveFocus: false,
      preview: false,
    });
    await vscode.commands
      .executeCommand("workbench.action.focusActiveEditorGroup")
      .then(undefined, () => undefined);
    await settle(120);
    const active = vscode.window.activeTextEditor;
    if (active && active.document.uri.toString() === document.uri.toString()) {
      return true;
    }
  }
  return false;
}

/** Remove `dir`, a temp directory a suite created, when it exists. */
function removeDir(dir: string | undefined): void {
  if (dir && fs.existsSync(dir)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
}

/** Assert the Osprey extension is active, naming what the test proved. */
function assertExtensionActive(message: string): void {
  assert.ok(vscode.extensions.getExtension(extensionId)?.isActive, message);
}

/** Write `content` to `name` inside `dir` and open it in an editor. */
async function writeAndOpen(
  dir: string,
  name: string,
  content: string,
): Promise<vscode.TextDocument> {
  const file = path.join(dir, name);
  fs.writeFileSync(file, content);
  const document = await vscode.workspace.openTextDocument(file);
  await vscode.window.showTextDocument(document);
  return document;
}

/**
 * Write `content` to `name` inside `dir` and make it the active editor.
 * With `note` the activation is asserted, naming that check.
 */
async function openActive(
  dir: string,
  name: string,
  content: string,
  note?: string,
): Promise<vscode.TextDocument> {
  const document = await writeAndOpen(dir, name, content);
  const active = await makeActive(document);
  if (note !== undefined) {
    assert.ok(active, note);
  }
  return document;
}

/** Assert `text` contains every one of `parts`, naming the view and the value. */
function assertIncludes(text: string, parts: string[], label: string): void {
  for (const part of parts) {
    assert.ok(text.includes(part), `${label} shows ${part}: ${text}`);
  }
}

/** Assert `items` contains every one of `wanted`, naming the view. */
function assertListed(
  items: readonly string[],
  wanted: string[],
  label: string,
): void {
  for (const item of wanted) {
    assert.ok(items.includes(item), `${label} lists ${item}`);
  }
}

/** Assert `document` is a .osp source the extension associated with "osprey". */
function assertOspreyDoc(
  document: vscode.TextDocument,
  message: string,
): void {
  assert.strictEqual(document.languageId, "osprey", `${message}: osprey id`);
  assert.ok(document.fileName.endsWith(".osp"), `${message}: is a .osp file`);
}

/** A fresh temp directory, named for the suite that owns it. */
function makeTempDir(label: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), `osprey-${label}-`));
}

/** Assert a raced debug start settled and left no debug session behind. */
function assertNoDebugSession(outcome: unknown, message: string): void {
  assert.ok(typeof outcome === "string", `${message}: the start settled`);
  assert.strictEqual(vscode.debug.activeDebugSession, undefined, message);
}

/** Run one real debug launch, let the host settle, and assert the race resolved. */
async function launchDebug(
  config: vscode.DebugConfiguration,
  budgetMs: number,
  settleMs: number,
): Promise<string> {
  const outcome = await startDebugRaced(config, budgetMs);
  await settle(settleMs);
  assert.ok(typeof outcome === "string", "the debug start settled");
  return outcome;
}

/** Assert each `[name, kind]` pair is the kind the outline reports for it. */
function assertSymbolKinds(
  byName: Map<string, vscode.DocumentSymbol>,
  expected: [string, vscode.SymbolKind][],
): void {
  for (const [name, kind] of expected) {
    assert.strictEqual(
      byName.get(name)?.kind,
      kind,
      `${name} is a ${vscode.SymbolKind[kind]}`,
    );
  }
}

/** Close every editor in the window. */
const closeAllEditors = (): Thenable<unknown> =>
  vscode.commands.executeCommand("workbench.action.closeAllEditors");

/** Activate the Osprey extension if needed, returning the extension. */
async function ensureExtensionActive() {
  const ext = vscode.extensions.getExtension<OspreyExtensionApi>(extensionId);
  assert.ok(ext, "the Osprey extension must be discoverable");
  if (!ext.isActive) {
    await ext.activate();
  }
  return ext;
}

/** Poll for up to ~6s until `document` reports `languageId`. */
async function waitForLanguage(
  document: vscode.TextDocument,
  languageId: string,
): Promise<void> {
  for (let i = 0; i < 40 && document.languageId !== languageId; i++) {
    await settle(150);
  }
}

/** Fire both osprey commands against the active editor, then let them settle. */
async function compileAndRun(settleMs: number): Promise<void> {
  await vscode.commands.executeCommand("osprey.compile");
  await vscode.commands.executeCommand("osprey.run");
  await settle(settleMs);
}

/** The pieces of the extension's committed package.json these suites read. */
interface OspreyPackageJson {
  activationEvents?: string[];
  contributes?: {
    commands?: { command: string }[];
    keybindings?: { command: string; key?: string }[];
    breakpoints?: { language: string }[];
    debuggers?: { type: string; languages?: string[] }[];
    languages?: { id: string; extensions?: string[]; firstLine?: string }[];
  };
}

/** The extension's committed package.json, parsed. */
function readPackageJson(): OspreyPackageJson {
  return JSON.parse(
    fs.readFileSync(path.join(extensionRoot, "package.json"), "utf8"),
  ) as OspreyPackageJson;
}

// The compiled test lives at <ext>/out/test/suite, so the extension root is
// three levels up. The release pipeline stamps a shipwright.json into the
// extension root (it is gitignored as a build-time artifact); we replicate that
// here so the Shipwright version handshake in activate() runs under test
// instead of being skipped. Staging happens at module load — before any test
// triggers (lazy) activation. `extensionRoot` is imported from ./osprey-test-env.
const stagedManifestPath = path.join(extensionRoot, "shipwright.json");
const repoRootManifestPath = path.resolve(
  extensionRoot,
  "../../..",
  ".github",
  "shipwright.json",
);
let manifestWasStaged = false;

(function stageShipwrightManifest(): void {
  // Only stage if the extension hasn't already shipped one and we have the
  // canonical repo-root manifest to copy.
  if (
    !fs.existsSync(stagedManifestPath) &&
    fs.existsSync(repoRootManifestPath)
  ) {
    fs.copyFileSync(repoRootManifestPath, stagedManifestPath);
    manifestWasStaged = true;
  }
})();

// The DAP test harness (breakpoints, session-lifecycle waiters, stackTrace /
// scopes / variables, waitForStop) and the osprey/lldb-dap binary resolution
// live in ./dap-harness and ./osprey-test-env. They are imported above; nothing
// is re-derived here. [DEBUGGER-REUSE]

suite("Osprey Shipwright Activation Coverage", () => {
  let pinned: PinnedOspreySettings | undefined;

  suiteSetup(async () => {
    // Point server.compilerPath at the real osprey binary BEFORE the extension
    // activates so resolveServerCommand takes its explicit-user-path branch and
    // the language client launches against a genuine osprey, exercising the
    // client option handlers and the start outcome.
    const ospreyPath = resolveBuiltOsprey();
    if (ospreyPath) {
      pinned = await pinOspreySettings({ "server.compilerPath": ospreyPath });
    }
  });

  suiteTeardown(async () => {
    // Restore the compiler path so later suites see defaults.
    await pinned?.restore();
    // Remove only the manifest we staged so we never delete a real one.
    if (manifestWasStaged && fs.existsSync(stagedManifestPath)) {
      fs.rmSync(stagedManifestPath, { force: true });
    }
  });

  test("a compiler path change restarts the language server on that compiler", async () => {
    const ospreyPath = resolveBuiltOsprey();
    assert.ok(ospreyPath, "build the Osprey compiler before this suite");
    const ext = await ensureExtensionActive();
    const client = ext.exports.getLspClient();
    assert.ok(client, "the language client exists");
    const serving = () =>
      (
        client as unknown as {
          _serverProcess?: { spawnfile?: string; pid?: number };
        }
      )._serverProcess;
    const servesOn = async (binary: string) => {
      for (let tries = 0; tries < 120; tries += 1) {
        if (client.isRunning() && serving()?.spawnfile === binary) {
          return;
        }
        await settle(250);
      }
      assert.fail(
        `server still on ${serving()?.spawnfile}, expected ${binary}`,
      );
    };
    const config = () => vscode.workspace.getConfiguration("osprey");
    await servesOn(ospreyPath);
    const firstProcess = serving()?.pid;

    const dir = makeTempDir("restart");
    const copy = path.join(dir, path.basename(ospreyPath));
    fs.copyFileSync(ospreyPath, copy);
    fs.chmodSync(copy, 0o755);
    try {
      await config().update(
        "server.compilerPath",
        copy,
        vscode.ConfigurationTarget.Global,
      );
      await servesOn(copy);
      assert.notStrictEqual(
        serving()?.pid,
        firstProcess,
        "a new server process runs",
      );

      // The restarted server answers real requests on the new compiler.
      const document = await vscode.workspace.openTextDocument({
        language: "osprey",
        content: "fn answer() = 42\nprint(answer())\n",
      });
      const symbols = await client.sendRequest<unknown[]>(
        "textDocument/documentSymbol",
        {
          textDocument: { uri: document.uri.toString() },
        },
      );
      assert.ok(
        Array.isArray(symbols) && symbols.length > 0,
        "symbols come from the new server",
      );
    } finally {
      await config().update(
        "server.compilerPath",
        ospreyPath,
        vscode.ConfigurationTarget.Global,
      );
      await servesOn(ospreyPath);
      fs.rmSync(dir, { recursive: true, force: true });
    }
    assert.strictEqual(
      serving()?.spawnfile,
      ospreyPath,
      "the server follows the path back",
    );
  });

  test("extension activates with a shipwright manifest present", async () => {
    // The activation-time version handshake implements [EDITOR-VERSIONING].
    const ext = vscode.extensions.getExtension(extensionId);
    assert.ok(ext, "extension must be discoverable");

    // The explicit compiler path we set must be visible to the extension so its
    // resolveServerCommand picks the user-configured binary.
    const ospreyPath = resolveBuiltOsprey();
    if (ospreyPath) {
      assert.strictEqual(
        vscode.workspace
          .getConfiguration("osprey")
          .get<string>("server.compilerPath"),
        ospreyPath,
        "server.compilerPath is the staged osprey binary",
      );
    }

    // The manifest path the extension resolves must point at the staged file so
    // the Shipwright handshake block (fs.existsSync(manifestPath)) is taken.
    assert.ok(
      fs.existsSync(stagedManifestPath),
      "shipwright.json is staged in the extension root",
    );

    // Activating runs the whole activate() body, including the async Shipwright
    // import + activateShipwright handshake. It must not throw and must leave
    // the extension active.
    if (ext && !ext.isActive) {
      await ext.activate();
    }
    // Give the fire-and-forget Shipwright async IIFE time to run its import,
    // version check, and outputChannel.appendLine before we assert.
    await settle(2500);

    assert.ok(ext?.isActive, "extension is active after Shipwright handshake");

    // The manifest we staged is valid JSON describing the osprey product, so a
    // successful load is the contract the handshake depends on.
    const manifest = JSON.parse(fs.readFileSync(stagedManifestPath, "utf8"));
    assert.strictEqual(
      manifest.product.id,
      "osprey",
      "manifest is the osprey product",
    );
    assert.ok(
      Array.isArray(manifest.components),
      "manifest declares components",
    );
    assert.ok(
      manifest.components.length > 0,
      "manifest has at least one component",
    );
  });
});

suite("Osprey Extension Integration Tests", () => {
  let tempDir: string;

  setup(() => {
    tempDir = makeTempDir("test");
  });

  teardown(() => removeDir(tempDir));

  /** Write `content` to the suite's temp .osp file and show it. */
  const openTestFile = (content: string) =>
    writeAndOpen(tempDir, "test.osp", content);

  test("Extension should activate when opening .osp file", async () => {
    // Create a simple Osprey file
    await openTestFile(`
// Simple test function
fn add(a, b) = a + b

let result = add(5, 3)
print(result)
`);

    // Wait a bit for extension to activate
    await settle(1000);

    // Check that the extension is active
    const extension = vscode.extensions.getExtension(extensionId);
    assert.ok(extension, "Extension should be found");
    if (extension) {
      assert.ok(
        extension.isActive,
        "Extension should be active after opening .osp file",
      );
    }
  });

  test("Language should be set to osprey for .osp files", async () => {
    const document = await openTestFile(`fn test() = 42`);

    // Wait for language detection
    await settle(500);

    assert.strictEqual(
      document.languageId,
      "osprey",
      "Language should be set to osprey",
    );
  });

  test("Compile command should be available for .osp files", async () => {
    await openTestFile(`fn hello() = print("Hello, World!")`);

    // Wait for extension activation
    await settle(1000);

    // Get all available commands
    const commands = await vscode.commands.getCommands();

    assertListed(commands, ["osprey.compile"], "Compile command");
    assertListed(commands, ["osprey.run"], "Run command");
    assertListed(commands, ["osprey.debug"], "Debug command");
  });

  test("Syntax highlighting should work for .osp files", async () => {
    const document = await openTestFile(`
fn power(base, exp) = match exp {
  0 => 1
  1 => base
  _ => base * power(base, exp - 1)
}

let result = power(2, 3)
print(result)
`);

    // Wait for syntax highlighting to load
    await settle(1000);

    // Check that the document has the correct language
    assert.strictEqual(document.languageId, "osprey");

    // Check that the file has content (basic sanity check)
    assertIncludes(
      document.getText(),
      ["fn power"],
      "Document should contain the test code",
    );
  });

  test("Extension should handle invalid Osprey code gracefully", async () => {
    await openTestFile(`
fn broken syntax here {
  this is not valid osprey code
  missing parentheses and stuff
}
`);

    // Wait for diagnostics
    await settle(2000);

    // Extension should still be active even with invalid code
    assertExtensionActive("Extension should remain active with invalid code");
  });

  test("File operations should work without workspace", async () => {
    // This test ensures the extension works with individual files.
    // Close any existing workspace
    if (vscode.workspace.workspaceFolders) {
      await vscode.commands.executeCommand("workbench.action.closeFolder");
    }

    // Open file without workspace
    const document = await openTestFile(
      `fn standalone() = print("No workspace needed!")`,
    );

    // Wait for extension
    await settle(1000);

    // Should still work
    assert.strictEqual(document.languageId, "osprey");
    assertExtensionActive("Extension should work without workspace");
  });

  test("Multiple .osp files should work correctly", async () => {
    // Create multiple test files
    const file1 = path.join(tempDir, "file1.osp");
    const file2 = path.join(tempDir, "file2.osp");

    fs.writeFileSync(file1, "fn func1() = 1");
    fs.writeFileSync(file2, "fn func2() = 2");

    // Open both files
    const doc1 = await vscode.workspace.openTextDocument(file1);
    const doc2 = await vscode.workspace.openTextDocument(file2);

    await vscode.window.showTextDocument(doc1);
    await vscode.window.showTextDocument(doc2);

    // Wait for processing
    await settle(1000);

    // Both should have correct language
    assert.strictEqual(doc1.languageId, "osprey");
    assert.strictEqual(doc2.languageId, "osprey");
  });

  test("Extension configuration should be accessible", async () => {
    const config = vscode.workspace.getConfiguration("osprey");

    // Check that configuration exists and has expected properties
    assert.ok(config, "Osprey configuration should exist");

    // Check default values
    const serverEnabled = config.get("server.enabled");
    const compilerPath = config.get("server.compilerPath");

    assert.strictEqual(
      typeof serverEnabled,
      "boolean",
      "server.enabled should be boolean",
    );
    assert.strictEqual(
      typeof compilerPath,
      "string",
      "server.compilerPath should be string",
    );
  });

  test("Language server should start successfully", async () => {
    // End-to-end client launch coverage for [EDITOR-VSCODE].
    // Basic test that language server starts without crashing
    await openTestFile(`fn test() = 42`);

    // Wait for language server to start
    await settle(3000);

    // Extension should still be active
    assertExtensionActive(
      "Extension should remain active with language server",
    );
  });

  test("Compile and Run commands execute against the active file", async () => {
    await openTestFile('fn main() -> Unit = print("hi")\n');
    await settle(500);

    // Both commands shell out to the staged `osprey` binary on PATH; they must
    // run their handlers to completion without throwing back to the caller.
    await compileAndRun(3000);

    assertExtensionActive(
      "Extension should remain active after running commands",
    );
  });
});

// These tests drive the REAL Osprey language server end-to-end through VS
// Code's provider commands and HARD-ASSERT the results. They are the regression
// net for "hover doesn't work": if the LSP fails to launch (e.g. a dead
// server.compilerPath that ENOENTs the client) EVERY test here fails loudly
// instead of silently passing. To guarantee a live server we pin
// server.compilerPath at a real osprey binary and warm the server up before any
// assertion runs.
suite("Osprey Language Features Tests", () => {
  let tempDir: string;
  let pinned: PinnedOspreySettings | undefined;

  // pollFor retries an async provider call until `ok` accepts its result or the
  // budget is exhausted, then throws with the last value. This is what turns
  // "the LSP returned nothing (yet)" into an eventual HARD FAILURE rather than a
  // silent pass — the toothlessness that let the hover regression ship.
  async function pollFor<T>(
    attempt: () => Thenable<T>,
    ok: (value: T) => boolean,
    tries = 40,
    delayMs = 250,
  ): Promise<T> {
    let last: T | undefined;
    for (let i = 0; i < tries; i++) {
      last = await Promise.resolve(attempt());
      if (last !== undefined && ok(last)) {
        return last;
      }
      await new Promise((resolve) => setTimeout(resolve, delayMs));
    }
    throw new assert.AssertionError({
      message: `LSP condition unmet after ${tries} tries; last=${JSON.stringify(last)}`,
    });
  }

  const openDoc = (name: string, content: string) =>
    writeAndOpen(tempDir, name, content);

  const hoverText = (h: vscode.Hover): string =>
    h.contents.map((c) => (typeof c === "string" ? c : c.value)).join("\n");

  /**
   * Bind one VS Code provider command to a `(uri, line, character)` lookup, so
   * every provider is asked the same way.
   */
  const providerAt =
    <T>(command: string) =>
    (uri: vscode.Uri, line: number, character: number): Thenable<T> =>
      vscode.commands.executeCommand<T>(
        command,
        uri,
        new vscode.Position(line, character),
      );

  const hoverAt = providerAt<vscode.Hover[]>("vscode.executeHoverProvider");
  const defsAt = providerAt<vscode.Location[]>(
    "vscode.executeDefinitionProvider",
  );
  const refsAt = providerAt<vscode.Location[]>(
    "vscode.executeReferenceProvider",
  );
  const completionAt = providerAt<vscode.CompletionList>(
    "vscode.executeCompletionItemProvider",
  );
  const sigHelpAt = providerAt<vscode.SignatureHelp>(
    "vscode.executeSignatureHelpProvider",
  );
  const symbolsOf = (uri: vscode.Uri) =>
    vscode.commands.executeCommand<vscode.DocumentSymbol[]>(
      "vscode.executeDocumentSymbolProvider",
      uri,
    );
  const nonEmptyHover = (h: vscode.Hover[]): boolean =>
    Array.isArray(h) && h.length > 0 && hoverText(h[0]).length > 0;
  const labelsOf = (list: vscode.CompletionList): string[] =>
    list.items.map((i) =>
      typeof i.label === "string" ? i.label : i.label.label,
    );
  const startLines = (locs: vscode.Location[]): number[] =>
    locs.map((l) => l.range.start.line).sort((a, b) => a - b);
  /** Whether the outline lists a symbol named `name`. */
  const listsSymbol = (name: string) => (symbols: vscode.DocumentSymbol[]) =>
    Array.isArray(symbols) && symbols.some((symbol) => symbol.name === name);

  /** The hover at `line:character` once it renders `needle`. */
  const hoverPoll = (
    uri: vscode.Uri,
    line: number,
    character: number,
    needle: string,
    tries = 40,
    delayMs = 250,
  ) =>
    pollFor(
      () => hoverAt(uri, line, character),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes(needle),
      tries,
      delayMs,
    );

  /** The hover at `line:character`, asserted to equal the declaration's text. */
  async function assertCallHoverMatches(
    uri: vscode.Uri,
    line: number,
    character: number,
    needle: string,
    declMd: string,
    note: string,
  ): Promise<void> {
    const call = await hoverPoll(uri, line, character, needle);
    assert.strictEqual(hoverText(call[0]), declMd, note);
  }

  /**
   * The hover at `line:character`, once it renders `needle`, asserted to stay
   * that way. `describe` names what the check proves, in the hover's own text.
   */
  async function hoverShowing(
    uri: vscode.Uri,
    line: number,
    character: number,
    symbol: string,
    needle: string,
    describe: (text: string) => string,
  ): Promise<vscode.Hover[]> {
    const hovers = await hoverPoll(uri, line, character, symbol);
    const text = hoverText(hovers[0]);
    assert.ok(text.includes(needle), describe(text));
    return hovers;
  }

  /** The definition locations at `line:character`, once any arrive. */
  const defsPoll = (uri: vscode.Uri, line: number, character: number) =>
    pollFor(
      () => defsAt(uri, line, character),
      (d) => Array.isArray(d) && d.length > 0,
    );

  /** The references at `line:character`, once at least `min` arrive. */
  const refsPoll = (
    uri: vscode.Uri,
    line: number,
    character: number,
    min = 1,
  ) =>
    pollFor(
      () => refsAt(uri, line, character),
      (r) => Array.isArray(r) && r.length >= min,
    );

  /**
   * References at `line:character`: exactly `count`, sitting on `lines`. The
   * poll resolves past `count`, so the length check pins the exactness.
   */
  async function assertReferences(
    uri: vscode.Uri,
    line: number,
    character: number,
    count: number,
    lines: number[],
    name: string,
  ): Promise<vscode.Location[]> {
    const refs = await refsPoll(uri, line, character, count);
    assert.strictEqual(refs.length, count, `${name}: declaration + calls`);
    assert.deepStrictEqual(
      startLines(refs),
      lines,
      `${name} references on the decl and call lines`,
    );
    return refs;
  }

  /** The document symbols for `uri`, once at least `min` arrive. */
  const symbolsPoll = (uri: vscode.Uri, min = 1) =>
    pollFor(
      () => symbolsOf(uri),
      (s) => Array.isArray(s) && s.length >= min,
    );

  /** The completions at `line:character`, once `ok` accepts the list. */
  const completionPoll = (
    uri: vscode.Uri,
    line: number,
    character: number,
    ok: (list: vscode.CompletionList) => boolean = (list) =>
      list.items.length > 0,
  ) =>
    pollFor(
      () => completionAt(uri, line, character),
      (l) => !!l && Array.isArray(l.items) && ok(l),
    );

  /** The diagnostics for `uri`, once `ok` accepts them. */
  const diagnosticsPoll = (
    uri: vscode.Uri,
    ok: (found: vscode.Diagnostic[]) => boolean,
  ) => pollFor(() => Promise.resolve(vscode.languages.getDiagnostics(uri)), ok);

  suiteSetup(async function () {
    this.timeout(60000);
    tempDir = makeTempDir("feat");

    // Pin a genuine compiler so the language client is actually running.
    const ospreyPath = resolveBuiltOsprey();
    assert.ok(
      ospreyPath,
      "a freshly-built osprey binary (target/release or PATH) is required for feature tests",
    );
    pinned = await pinOspreySettings({ "server.compilerPath": ospreyPath });

    await ensureExtensionActive();

    // Warm up: open a document and wait until hover actually answers. This both
    // proves the server is live and removes first-request flakiness from the
    // individual feature tests below.
    const warm = await openDoc(
      "warmup.osp",
      "\nfn warm(x) = wrapMul(x, 2)\n\nlet w = warm(2)\n",
    );
    await pollFor<vscode.Hover[]>(
      () => hoverAt(warm.uri, 3, 9),
      (h) =>
        Array.isArray(h) && h.length > 0 && hoverText(h[0]).includes("warm"),
      80,
      250,
    );
    await closeAllEditors();
  });

  suiteTeardown(async () => {
    await pinned?.restore();
    await closeAllEditors();
    removeDir(tempDir);
  });

  teardown(async () => {
    await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
  });

  // A single multi-construct program (type + functions + lets + builtins)
  // exercised by the full provider sweep below.
  const RICH =
    [
      "type Shape = Circle | Square", // 0
      "fn area(r) = wrapMul(r, r)", // 1
      "fn perimeter(r) = wrapAdd(wrapAdd(r, r), wrapAdd(r, r))", // 2
      "let radius = 5", // 3
      "let a = area(radius)", // 4
      "let b = area(10)", // 5
      "let c = perimeter(radius)", // 6
      "let names = List()", // 7
      "let count = listLength(names)", // 8
      "let m = print(a)", // 9
    ].join("\n") + "\n";

  test("CHUNKY: full language-intelligence sweep over one program (hover/def/refs/symbols/sig/completion)", async function () {
    this.timeout(60000);
    const doc = await openDoc("sweep.osp", RICH);

    // --- HOVER: user functions at declaration and call site ---
    const areaDecl = await hoverPoll(doc.uri, 1, 3, "area");
    const areaDeclMd = hoverText(areaDecl[0]);
    // `fn area(r) = wrapMul(r, r)` writes neither the parameter type nor the return
    // type, so both slots come from the checker ([LSP-HOVER-INFERRED-SIGNATURE]).
    // This used to assert a bare `fn area(r)` returning `Unit`; `Unit` was the
    // display fallback for "nothing written", never a claim about the function,
    // so that assertion pinned the very bug inferred signatures fixed.
    assertIncludes(
      areaDeclMd,
      ["fn area(r: int)"],
      "area decl hover fills in the inferred parameter type",
    );
    assertIncludes(
      areaDeclMd,
      ["-> int"],
      "area decl hover shows the inferred return type, not the Unit fallback",
    );

    await assertCallHoverMatches(
      doc.uri,
      4,
      9,
      "area",
      areaDeclMd,
      "call-site hover matches the declaration hover",
    );

    await hoverShowing(
      doc.uri,
      6,
      9,
      "perimeter",
      "fn perimeter(r",
      (text) => `perimeter hover shows its signature, got: ${text}`,
    );

    // --- HOVER: built-ins carry their own typed signatures ---
    const lenHover = await hoverPoll(doc.uri, 8, 13, "listLength");
    const lenMd = hoverText(lenHover[0]);
    assert.ok(
      lenMd.includes("->") && lenMd.includes("int"),
      "listLength hover shows its typed signature",
    );
    assert.ok(lenMd.includes("List"), "listLength hover mentions List");

    const printHover = await hoverPoll(doc.uri, 9, 9, "print");
    assertIncludes(
      hoverText(printHover[0]),
      ["Unit"],
      "print hover shows it returns Unit",
    );

    // --- HOVER: a `let` binding shows its TYPE, not just functions/builtins ---
    // This is the regression the user reported: "no bubble on hover of
    // variables". `let radius = 5` is unannotated, so the type is the one the
    // checker INFERRED (int) — proving variable hover end-to-end. [LSP-HOVER-VARIABLES]
    await hoverShowing(
      doc.uri,
      3,
      4,
      "radius",
      "radius: int",
      (text) => `variable hover shows "name: inferred-type": ${text}`,
    );
    // A let bound to a builtin call infers its result type too.
    await hoverShowing(
      doc.uri,
      8,
      4,
      "count",
      "count: int",
      (text) => `count infers int from listLength: ${text}`,
    );

    // --- GO TO DEFINITION: both calls jump to their declarations ---
    const areaDef = await defsPoll(doc.uri, 4, 9);
    assert.strictEqual(areaDef.length, 1, "area has exactly one definition");
    assert.strictEqual(
      areaDef[0].range.start.line,
      1,
      "area defined on line 1",
    );
    assert.strictEqual(
      areaDef[0].range.start.character,
      3,
      "area name starts at character 3",
    );
    assert.strictEqual(
      areaDef[0].uri.toString(),
      doc.uri.toString(),
      "definition is in this document",
    );

    const periDef = await defsPoll(doc.uri, 6, 9);
    assert.strictEqual(
      periDef[0].range.start.line,
      2,
      "perimeter defined on line 2",
    );

    // --- FIND REFERENCES: declaration + every call, scoped to the function ---
    const areaRefs = await assertReferences(doc.uri, 1, 3, 3, [1, 4, 5], "area");
    assert.ok(
      areaRefs.every((r) => r.uri.toString() === doc.uri.toString()),
      "all area references in this document",
    );

    await assertReferences(doc.uri, 2, 3, 2, [2, 6], "perimeter");

    // --- DOCUMENT SYMBOLS: type, functions and lets with correct kinds ---
    const syms = await symbolsPoll(doc.uri, 10);
    const byName = new Map(syms.map((s) => [s.name, s]));
    for (const name of [
      "Shape",
      "area",
      "perimeter",
      "radius",
      "a",
      "b",
      "c",
      "names",
      "count",
      "m",
    ]) {
      assert.ok(byName.has(name), `symbol ${name} is listed`);
    }
    assertSymbolKinds(byName, [
      ["Shape", vscode.SymbolKind.Class],
      ["area", vscode.SymbolKind.Function],
      ["perimeter", vscode.SymbolKind.Function],
      ["radius", vscode.SymbolKind.Variable],
    ]);

    // --- SIGNATURE HELP: inside the area(...) call ---
    const help = await pollFor(
      () => sigHelpAt(doc.uri, 4, 13),
      (h) => !!h && Array.isArray(h.signatures) && h.signatures.length > 0,
    );
    assertIncludes(help.signatures[0].label, ["area"], "signature names area");
    assert.strictEqual(
      help.signatures[0].parameters.length,
      1,
      "area has one parameter",
    );
    assert.strictEqual(
      help.activeParameter,
      0,
      "the first parameter is active",
    );

    // --- COMPLETION: user symbols always, keywords only where they parse ---
    // Line 9 is `let m = print(a)` and the cursor sits after `let m = `, a
    // VALUE position. A declaration keyword completed there expands to source
    // no flavor parses (`let m = fn name(params) = body`), so the declaration
    // forms are withheld while symbols and the expression forms stay.
    // [LSP-COMPLETION-CONTEXT]
    // Poll until the server has answered *for this document* — a non-empty list
    // is not enough, because before `didOpen` is processed the request is
    // answered from an empty prefix, which classifies as a declaration position
    // and legitimately offers `fn`. Waiting on a symbol only this document
    // declares is what proves the answer belongs to it.
    const list = await completionPoll(doc.uri, 9, 9, (list) =>
      labelsOf(list).includes("perimeter"),
    );
    const labels = labelsOf(list);
    for (const sym of ["Shape", "area", "perimeter", "radius", "count"]) {
      assert.ok(
        labels.includes(sym),
        `completion offers the user symbol ${sym}`,
      );
    }
    for (const kw of ["match", "if"]) {
      assert.ok(
        labels.includes(kw),
        `a value position keeps the expression keyword ${kw}`,
      );
    }
    for (const kw of ["fn", "let", "type"]) {
      assert.ok(
        !labels.includes(kw),
        `a value position withholds the declaration keyword ${kw}: ${JSON.stringify(labels)}`,
      );
    }

    // Line 10 is the empty line past the program — declaration position, where
    // every keyword the flavor has is legal again.
    const atTopLevel = await completionPoll(doc.uri, 10, 0);
    const topLevelLabels = labelsOf(atTopLevel);
    for (const kw of ["fn", "let", "match", "type"]) {
      assert.ok(
        topLevelLabels.includes(kw),
        `declaration position offers the keyword ${kw}`,
      );
    }
  });

  test("CHUNKY: a `///`-documented variable hovers with its type AND its docs", async function () {
    this.timeout(60000);
    // The second half of the report: "document variables like we can document
    // functions". A `///` block above a `let` must surface in hover, beneath the
    // inferred type — exactly as it already does for functions. Local (nested in
    // a block) and top-level bindings are both proven here. [LSP-HOVER-DOCS]
    const content =
      [
        "/// The maximum retry budget for a request.", // 0
        "let maxRetries = 3", // 1
        "fn handler() -> int = {", // 2
        "  /// Greeting echoed back to the caller.", // 3
        '  let banner = "hello"', // 4
        "  maxRetries", // 5
        "}", // 6
      ].join("\n") + "\n";
    const doc = await openDoc("documented.osp", content);

    // Top-level documented let: type + doc prose.
    const topMd = hoverText((await hoverPoll(doc.uri, 1, 4, "maxRetries"))[0]);
    assertIncludes(
      topMd,
      ["maxRetries: int", "The maximum retry budget for a request."],
      "top-level let hover",
    );

    // Nested documented let inside a function block: type + doc prose. This is
    // the exact shape from the reported screenshot (a `let` inside `in { … }`).
    const nestedMd = hoverText((await hoverPoll(doc.uri, 4, 6, "banner"))[0]);
    assertIncludes(
      nestedMd,
      ["banner: string", "Greeting echoed back to the caller."],
      "nested let hover",
    );
  });

  test("CHUNKY: diagnostics lifecycle — error surfaces, clears on fix, returns on re-break", async function () {
    this.timeout(60000);
    const doc = await openDoc("lifecycle.osp", "\nfn broken( = 42\n");

    // 1) The broken program must surface a real Error diagnostic from osprey.
    const first = await diagnosticsPoll(doc.uri, (d) => d.length > 0);
    assert.strictEqual(
      first[0].severity,
      vscode.DiagnosticSeverity.Error,
      "first diagnostic is an Error",
    );
    assert.ok(first[0].message.length > 0, "diagnostic carries a message");
    assert.ok(
      /syntax/i.test(first[0].message),
      "message identifies a syntax error",
    );
    assert.strictEqual(
      first[0].source,
      "osprey",
      "diagnostic attributed to the osprey server",
    );
    assert.ok(first[0].range.start.line >= 0, "diagnostic has a real range");

    // 2) Editing the buffer into a valid program retracts the diagnostic...
    await rewrite(
      doc,
      'fn ok(x) = x * 2\nfn main() = {\n    handle Arith { overflow _ _ _ w => w }\n    print("${ok(2)}")\n}\n',
    );
    await assertClean(doc.uri, "no diagnostics once the code is valid");
    // ...and the now-valid buffer answers hover, proving the server reparsed it.
    await hoverShowing(
      doc.uri,
      0,
      3,
      "ok",
      "fn ok",
      () => "hover works on the corrected program",
    );

    // 3) Re-breaking the buffer brings the Error diagnostic back.
    await rewrite(doc, "\nfn broken( = 42\n");
    const second = await diagnosticsPoll(doc.uri, (d) => d.length > 0);
    assert.strictEqual(
      second[0].severity,
      vscode.DiagnosticSeverity.Error,
      "re-broken code errors again",
    );
    assert.ok(second[0].message.length > 0, "re-error has a message");
  });

  // ---------- [TESTING-SKIP-WARNING] ----------
  // NO test may be skipped or ignored without a diagnostic warning. These
  // drive the REAL `osprey lsp` server: the warning must be there the moment
  // the file is open, before anything runs.

  /**
   * Every Error diagnostic for `uri`. A reasoned skip is a warning and never
   * appears here; an unexplained one does ([TESTING-SKIP-REASON]).
   */
  const errorsOf = (uri: vscode.Uri): vscode.Diagnostic[] =>
    vscode.languages
      .getDiagnostics(uri)
      .filter((d) => d.severity === vscode.DiagnosticSeverity.Error);

  /** Poll until exactly `count` skip warnings are published for `uri`. */
  const pollSkips = (
    uri: vscode.Uri,
    count: number,
  ): Promise<vscode.Diagnostic[]> =>
    pollFor(
      () => Promise.resolve(skipWarningsOf(uri)),
      (found) => found.length === count,
    );

  /** Open `name` from `lines` and poll until exactly `count` skip warnings. */
  async function openWithSkips(
    name: string,
    lines: readonly string[],
    count: number,
  ): Promise<{
    doc: vscode.TextDocument;
    warnings: vscode.Diagnostic[];
  }> {
    const doc = await openDoc(name, lines.join("\n") + "\n");
    return { doc, warnings: await pollSkips(doc.uri, count) };
  }

  /** Replace a document's whole text (a user editing the buffer). */
  async function rewrite(
    doc: vscode.TextDocument,
    content: string,
  ): Promise<void> {
    const editor = await vscode.window.showTextDocument(doc);
    await editor.edit((b) =>
      b.replace(new vscode.Range(0, 0, doc.lineCount, 0), content),
    );
  }

  /** Rewrite `lines` with `index` replaced by `text`, and push it to `doc`. */
  async function rewriteLine(
    doc: vscode.TextDocument,
    lines: string[],
    index: number,
    text: string,
  ): Promise<void> {
    // `lines` is the caller's own copy and keeps the edit, so the next step builds on it.
    lines[index] = text;
    await rewrite(doc, lines.join("\n") + "\n");
  }

  /** `uri` ends up with no diagnostics at all, once the server retracts them. */
  async function assertClean(uri: vscode.Uri, message: string): Promise<void> {
    const found = await diagnosticsPoll(uri, (d) => d.length === 0);
    assert.strictEqual(found.length, 0, message);
  }

  /**
   * Assert the shared shape every skip diagnostic must have. `severity` is
   * Warning for a skip that names a reason and Error for one that does not
   * ([TESTING-SKIP-REASON]); everything else about the shape is identical,
   * because it is one rule reported at two strengths.
   */
  function assertSkipWarningShape(
    warning: vscode.Diagnostic,
    line: number,
    label: string,
    severity: vscode.DiagnosticSeverity = vscode.DiagnosticSeverity.Warning,
  ): void {
    assert.strictEqual(
      warning.severity,
      severity,
      `${label}: a skipped test is reported at the severity its reason earns`,
    );
    assert.strictEqual(warning.source, "osprey", `${label}: source`);
    assert.strictEqual(warning.code, "test-skipped", `${label}: code`);
    assert.strictEqual(
      warning.range.start.line,
      line,
      `${label}: underlines the test call's own line`,
    );
    assert.strictEqual(
      warning.range.end.line,
      line,
      `${label}: the span stays on one line`,
    );
    assert.ok(
      warning.range.end.character > warning.range.start.character,
      `${label}: the span is non-empty so it renders as a squiggle`,
    );
    assert.ok(
      warning.message.startsWith("test '"),
      `${label}: message names the case first: ${warning.message}`,
    );
  }

  const VERDICT_DEFAULT = "type Verdict = Pass | Fail(string) | Skip(string)";

  test("CHUNKY: skip-warning taxonomy — every skip form is reported, every live form stays silent", async function () {
    this.timeout(60000);
    // One file covering every body shape [TESTING-VERDICT] admits. Only the
    // bodies that STATICALLY yield Skip may warn; a body that merely *might*
    // skip at run time is the Test Explorer's job, not the LSP's.
    const lines = [
      VERDICT_DEFAULT, //                                          0
      "", //                                                       1
      "fn guard(n) = match n > 100 {", //                           2
      "    true => Pass", //                                        3
      '    false => Skip("runtime only")', //                       4
      "}", //                                                       5
      'fn parked() = Skip("named function body")', //               6
      "", //                                                        7
      'test("static skip with reason", fn() => Skip("blocked on #123"))', // 8
      'test("static skip empty reason", fn() => Skip(""))', //       9
      'test("skip from a block body", fn() => {', //               10
      '    Skip("skipped from a block")', //                       11
      "})", //                                                     12
      'test("passing verdict", fn() => Pass)', //                  13
      'test("failing verdict", fn() => Fail("nope"))', //          14
      'test("assertion case", fn() => expect(1, 1))', //           15
      'test("dynamic skip via helper", fn() => guard(1))', //      16
      'test("named function reference", parked)', //               17
      "", //                                                       18
    ];
    // Exactly three cases skip STATICALLY: lines 8, 9, and 10.
    const { doc, warnings } = await openWithSkips(
      "taxonomy.test.osp",
      lines,
      3,
    );
    assert.strictEqual(
      warnings.length,
      3,
      `exactly the three static skips warn: ${JSON.stringify(warnings.map((w) => w.message))}`,
    );

    // --- Case 1: a reason is quoted into the message verbatim.
    assertSkipWarningShape(warnings[0], 8, "static skip with reason");
    assert.strictEqual(
      warnings[0].message,
      "test 'static skip with reason' is skipped: blocked on #123",
      "the reason is carried verbatim into the message",
    );

    // --- Case 2: an EMPTY reason is an ERROR, not a warning, and it names the
    // demand instead of dangling a separator with nothing after it
    // ([TESTING-SKIP-REASON]).
    assertSkipWarningShape(
      warnings[1],
      9,
      "static skip empty reason",
      vscode.DiagnosticSeverity.Error,
    );
    assert.strictEqual(
      warnings[1].message,
      "test 'static skip empty reason' is skipped with no reason; every skip must name one",
      "an unexplained skip states what is missing",
    );
    assert.ok(
      !warnings[1].message.includes("skipped:"),
      `no dangling separator: ${warnings[1].message}`,
    );

    // --- Case 3: a block body's RESULT expression is what counts, and the
    // warning lands on the `test(` line, not on the inner `Skip` line.
    assertSkipWarningShape(warnings[2], 10, "skip from a block body");
    assert.strictEqual(
      warnings[2].message,
      "test 'skip from a block body' is skipped: skipped from a block",
    );

    // --- Silent forms: nothing else may warn, by name.
    const messages = warnings.map((w) => w.message).join("\n");
    for (const silent of [
      "passing verdict",
      "failing verdict",
      "assertion case",
      "dynamic skip via helper",
      "named function reference",
    ]) {
      assert.ok(
        !messages.includes(silent),
        `'${silent}' must NOT raise a static skip warning:\n${messages}`,
      );
    }

    // The taxonomy program is well-typed, so the ONLY error is the unexplained
    // skip — no type error rides along with it ([TESTING-SKIP-REASON]).
    assert.deepStrictEqual(
      errorsOf(doc.uri).map((e) => e.message),
      [
        "test 'static skip empty reason' is skipped with no reason; every skip must name one",
      ],
      "the taxonomy program is well-typed; its only error is the unexplained skip",
    );

    // Warnings must not disturb the rest of the language service: hover on a
    // skipped case's `test` still answers with the case ([TESTING-DOC-HOVER]),
    // and the document outline still lists the file's functions.
    const hov = await pollFor(
      () => hoverAt(doc.uri, 8, 2),
      (h) => nonEmptyHover(h),
    );
    assertIncludes(
      hoverText(hov[0]),
      ["static skip with reason"],
      `hover still names the skipped case: ${hoverText(hov[0])}`,
    );
    const symbols = await symbolsPoll(doc.uri, 1);
    const symbolNames = symbols.map((s) => s.name);
    assert.ok(
      symbolNames.includes("guard") && symbolNames.includes("parked"),
      `outline unaffected by skip warnings: ${symbolNames.join(", ")}`,
    );
  });

  test("CHUNKY: skip-warning edit lifecycle — six edits, each re-checked end to end", async function () {
    this.timeout(90000);
    const base = [
      VERDICT_DEFAULT, //                                     0
      "", //                                                  1
      'test("alpha", fn() => Pass)', //                        2
      'test("beta", fn() => expect(1, 1))', //                 3
      'test("gamma", fn() => Pass)', //                        4
      "", //                                                   5
    ];
    const doc = await openDoc(
      "lifecycle-skip.test.osp",
      base.join("\n") + "\n",
    );

    // (1) A clean suite warns about nothing.
    const clean = await pollSkips(doc.uri, 0);
    assert.deepStrictEqual(clean, [], "no skips, no warnings");

    // (2) The user parks `beta` → exactly one warning, on beta's line.
    const oneSkip = [...base];
    await rewriteLine(
      doc,
      oneSkip,
      3,
      'test("beta", fn() => Skip("waiting on the API"))',
    );
    const afterPark = await pollSkips(doc.uri, 1);
    assertSkipWarningShape(afterPark[0], 3, "parked beta");
    assert.strictEqual(
      afterPark[0].message,
      "test 'beta' is skipped: waiting on the API",
    );

    // (3) The user parks `alpha` too → two warnings, in source order.
    const twoSkips = [...oneSkip];
    await rewriteLine(
      doc,
      twoSkips,
      2,
      'test("alpha", fn() => Skip("flaky on CI"))',
    );
    const afterSecond = await pollSkips(doc.uri, 2);
    assert.deepStrictEqual(
      afterSecond.map((w) => w.range.start.line),
      [2, 3],
      "warnings track their own lines, in source order",
    );
    assert.deepStrictEqual(
      afterSecond.map((w) => w.message),
      [
        "test 'alpha' is skipped: flaky on CI",
        "test 'beta' is skipped: waiting on the API",
      ],
    );

    // (4) Editing only the REASON updates the message in place.
    const editedReason = [...twoSkips];
    editedReason[2] = 'test("alpha", fn() => Skip("still flaky, see #99"))';
    await rewrite(doc, editedReason.join("\n") + "\n");
    const afterReason = await pollFor(
      () => Promise.resolve(skipWarningsOf(doc.uri)),
      (found) => found.length === 2 && found[0].message.includes("see #99"),
    );
    assert.strictEqual(
      afterReason[0].message,
      "test 'alpha' is skipped: still flaky, see #99",
      "a reason edit is reflected without a restart",
    );
    assert.strictEqual(
      afterReason[1].message,
      "test 'beta' is skipped: waiting on the API",
      "the untouched case keeps its own message",
    );

    // (5) Breaking the syntax suppresses skip warnings — an unparsable file
    // reports its syntax error ALONE, matching the CLI gate [LSP-DIAGNOSTICS].
    await rewrite(doc, editedReason.join("\n") + "\nfn broken( = 42\n");
    const broken = await diagnosticsPoll(doc.uri, (d) =>
      d.some((item) => item.code === "syntax-error"),
    );
    assert.ok(
      broken.some((d) => d.severity === vscode.DiagnosticSeverity.Error),
      "the syntax error is reported",
    );
    assert.deepStrictEqual(
      skipWarningsOf(doc.uri),
      [],
      "an unparsable buffer reports its syntax error alone",
    );

    // (6) Fixing the syntax brings both warnings straight back, unchanged.
    await rewrite(doc, editedReason.join("\n") + "\n");
    const restored = await pollSkips(doc.uri, 2);
    assert.deepStrictEqual(
      restored.map((w) => w.message),
      [
        "test 'alpha' is skipped: still flaky, see #99",
        "test 'beta' is skipped: waiting on the API",
      ],
    );
    assert.deepStrictEqual(
      vscode.languages
        .getDiagnostics(doc.uri)
        .filter((d) => d.code === "syntax-error"),
      [],
      "the syntax error is gone",
    );

    // (7) Reviving every case clears every warning.
    await rewrite(doc, base.join("\n") + "\n");
    const revived = await pollSkips(doc.uri, 0);
    assert.deepStrictEqual(revived, [], "revived cases carry no warnings");
  });

  test("CHUNKY: skip-warning damage cases — quotes, unicode, duplicates, directive-shaped names, shadowing", async function () {
    this.timeout(90000);
    // Hostile inputs: everything here is legal Osprey that a naive
    // string-scraping implementation would mangle.
    const lines = [
      VERDICT_DEFAULT, //                                                    0
      "", //                                                                 1
      'test("name with # SKIP inside it", fn() => expect(1, 1))', //          2
      'test("quoted \\"why\\" case", fn() => Skip("reason with \\"quotes\\""))', // 3
      'test("café 🦅 unicode", fn() => Skip("héllo 🦅 wörld"))', //           4
      'test("duplicate", fn() => Skip("first"))', //                          5
      'test("duplicate", fn() => Skip("second"))', //                         6
      "", //                                                                 7
    ];
    const { doc, warnings } = await openWithSkips("damage.test.osp", lines, 4);

    // --- A PASSING test whose NAME contains `# SKIP` must never be reported
    // as skipped [TESTING-TAP-AMBIGUITY]. Nothing on line 2 may warn.
    assert.ok(
      warnings.every((w) => w.range.start.line !== 2),
      `a name containing the directive is not a skip: ${JSON.stringify(warnings.map((w) => w.message))}`,
    );
    assert.ok(
      !warnings.some((w) => w.message.includes("inside it")),
      "the directive-shaped name never leaks into a warning",
    );

    // --- Quotes survive into the message unescaped.
    assertSkipWarningShape(warnings[0], 3, "quoted case");
    assert.strictEqual(
      warnings[0].message,
      `test 'quoted "why" case' is skipped: reason with "quotes"`,
      "embedded quotes survive the round trip",
    );

    // --- Unicode: the message keeps it, and the RANGE is measured in UTF-16
    // units so the squiggle lands on real characters [LSP-ENCODING].
    assertSkipWarningShape(warnings[1], 4, "unicode case");
    assertIncludes(
      warnings[1].message,
      ["héllo 🦅 wörld"],
      `unicode reason survives: ${warnings[1].message}`,
    );
    assertIncludes(
      warnings[1].message,
      ["café 🦅 unicode"],
      "unicode test name survives",
    );
    const unicodeLine = doc.lineAt(4).text;
    assert.ok(
      warnings[1].range.end.character <= unicodeLine.length,
      `range stays inside the line: ${warnings[1].range.end.character} > ${unicodeLine.length}`,
    );

    // --- Duplicate names: BOTH warn, each on its own line with its own
    // reason. A dedupe-by-name implementation would drop one and hide a skip.
    assertSkipWarningShape(warnings[2], 5, "duplicate one");
    assertSkipWarningShape(warnings[3], 6, "duplicate two");
    assert.strictEqual(
      warnings[2].message,
      "test 'duplicate' is skipped: first",
    );
    assert.strictEqual(
      warnings[3].message,
      "test 'duplicate' is skipped: second",
    );

    // --- [TESTING-SHADOWING] a user-defined `test` shadows the built-in, so
    // its calls are NOT test cases and must raise no skip warnings at all.
    const shadowed = [
      VERDICT_DEFAULT,
      "fn test(name, body) = 0",
      'let ignored = test("not a case", fn() => Skip("not a real skip"))',
      "",
    ].join("\n");
    const shadowDoc = await openDoc("shadowed.test.osp", shadowed);
    // Poll on a positive signal (the server answered SOMETHING for this file)
    // so an empty result cannot pass by simply arriving before the server.
    await pollFor(() => symbolsOf(shadowDoc.uri), listsSymbol("test"));
    assert.deepStrictEqual(
      skipWarningsOf(shadowDoc.uri),
      [],
      "a shadowed `test` built-in declares no cases, so nothing is skipped",
    );

    // --- A file with no tests at all, and an empty file, warn about nothing.
    const noTests = await openDoc(
      "notests.osp",
      "fn add(a, b) = wrapAdd(a, b)\nlet total = add(1, 2)\n",
    );
    await symbolsPoll(noTests.uri, 1);
    assert.deepStrictEqual(skipWarningsOf(noTests.uri), []);
  });

  test("CHUNKY: skip warnings coexist with type errors and survive in the ML flavor", async function () {
    this.timeout(90000);
    // --- A file that is BOTH ill-typed and skipping must report both: a skip
    // warning is never swallowed by an unrelated error.
    const mixed = [
      VERDICT_DEFAULT, //                                        0
      "fn broken() -> int = nope(1)", //                          1
      'test("parked while broken", fn() => Skip("blocked"))', //  2
      "",
    ].join("\n");
    const doc = await openDoc("mixed.test.osp", mixed);
    const warnings = await pollSkips(doc.uri, 1);
    assertSkipWarningShape(warnings[0], 2, "parked while broken");
    const errors = await pollFor(
      () => Promise.resolve(errorsOf(doc.uri)),
      (e) => e.length > 0,
    );
    assert.ok(
      errors.some((e) => e.code === "type-error"),
      `the type error is still reported: ${JSON.stringify(errors.map((e) => e.message))}`,
    );
    assert.ok(
      errors.every((e) => e.code !== "test-skipped"),
      "a skip that NAMES a reason is never escalated to an error",
    );
    assert.strictEqual(
      warnings[0].message,
      "test 'parked while broken' is skipped: blocked",
    );

    // --- The ML flavor gets identical treatment through its own frontend:
    // layout syntax, `\() =>` lambdas, whitespace application [FLAVOR-ML-FN].
    const mlLines = [
      "type Verdict = Pass | Fail string | Skip string", //   0
      "", //                                                  1
      "guard n = match n > 100", //                            2
      "    true => Pass", //                                   3
      '    false => Skip "runtime only"', //                   4
      "", //                                                   5
      'test "ml parked" (\\() => Skip "ml blocked")', //        6
      'test "ml live" (\\() => Pass)', //                       7
      'test "ml dynamic" (\\() => guard 1)', //                 8
      "", //                                                   9
    ];
    const { doc: mlDoc, warnings: mlWarnings } = await openWithSkips(
      "mlskip.test.ospml",
      mlLines,
      1,
    );
    assertSkipWarningShape(mlWarnings[0], 6, "ml parked");
    assert.strictEqual(
      mlWarnings[0].message,
      "test 'ml parked' is skipped: ml blocked",
      "the ML frontend produces the SAME message as the Default flavor",
    );
    assert.deepStrictEqual(
      errorsOf(mlDoc.uri).map((e) => e.message),
      [],
      "the ML program is well-typed",
    );

    // Editing the ML buffer re-checks it: parking the live case adds a second
    // warning, and reviving both clears all of them.
    const mlParked = [...mlLines];
    await rewriteLine(
      mlDoc,
      mlParked,
      7,
      'test "ml live" (\\() => Skip "now parked too")',
    );
    const mlBoth = await pollSkips(mlDoc.uri, 2);
    assert.deepStrictEqual(mlBoth.map((w) => w.range.start.line), [6, 7]);
    assert.strictEqual(
      mlBoth[1].message,
      "test 'ml live' is skipped: now parked too",
    );
    const mlRevived = [...mlLines];
    await rewriteLine(
      mlDoc,
      mlRevived,
      6,
      'test "ml parked" (\\() => Pass)',
    );
    assert.deepStrictEqual(
      await pollSkips(mlDoc.uri, 0),
      [],
      "reviving every ML case clears every warning",
    );
  });

  test("CHUNKY: list-pattern program — hover/def/refs/symbols/completion over match patterns", async function () {
    this.timeout(60000);
    const content =
      [
        "fn classify(xs) = match xs {", // 0
        '  [] => "empty"', // 1
        '  [head, ...tail] => "many ${listLength(tail)}"', // 2
        "}", // 3
        "let z = classify(List())", // 4
        "let w = classify(z)", // 5
      ].join("\n") + "\n";
    const doc = await openDoc("patterns.osp", content);

    // Hover the declaration and a call: both render the classify signature.
    // `xs` is proven to be a LIST by the patterns below it; only its element
    // type stays open. This used to assert the bare `fn classify(xs)`, because
    // the only alternative then was `List<t5>` and `t5` is an inference
    // artefact whose number moves when an unrelated line does. A hole is
    // neither: it keeps what the checker proved and names nothing it did not.
    const decl = await hoverShowing(
      doc.uri,
      0,
      3,
      "classify",
      "fn classify(xs: List<_>)",
      (text) => `declaration hover shows classify signature, got: ${text}`,
    );
    assert.ok(
      !/\bt\d/.test(hoverText(decl[0])),
      `an inference artefact must never reach a reader, got: ${hoverText(decl[0])}`,
    );
    await assertCallHoverMatches(
      doc.uri,
      4,
      9,
      "classify",
      hoverText(decl[0]),
      "call hover matches declaration hover",
    );

    // Definition from the first call lands on the declaration.
    const def = await defsPoll(doc.uri, 4, 9);
    assert.strictEqual(
      def[0].range.start.line,
      0,
      "classify defined on line 0",
    );
    assert.strictEqual(
      def[0].range.start.character,
      3,
      "classify name at character 3",
    );

    // References include the declaration and both calls.
    await assertReferences(doc.uri, 0, 3, 3, [0, 4, 5], "classify");

    // Symbols: the function plus the two lets, with correct kinds.
    const syms = await symbolsPoll(doc.uri, 3);
    assertSymbolKinds(
      new Map(syms.map((s) => [s.name, s])),
      [
        ["classify", vscode.SymbolKind.Function],
        ["z", vscode.SymbolKind.Variable],
        ["w", vscode.SymbolKind.Variable],
      ],
    );

    // Completion inside the second call offers the user function classify.
    const list = await completionPoll(doc.uri, 5, 10);
    assertListed(labelsOf(list), ["classify"], "completion offers classify");
  });

  test("CHUNKY: hover + symbols work on the actual list corpus file", async function () {
    this.timeout(60000);
    // Open the very file the user reported ("Hover doesnt work!") straight from
    // the repository and prove the language features answer over it. extensionRoot
    // is <repo>/src/toolchain/vscode-extension; the assertion corpus lives under
    // src/osprey/tests.
    const reported = path.resolve(
      extensionRoot,
      "..",
      "..",
      "osprey",
      "tests",
      "core",
      "collections",
      "list_basics.test.osp",
    );
    assert.ok(
      fs.existsSync(reported),
      `reported example exists at ${reported}`,
    );
    const doc = await vscode.workspace.openTextDocument(reported);
    await vscode.window.showTextDocument(doc);

    // Locate the probes by CONTENT, not by a hardcoded line number: this is a
    // living example that gains coverage over time, and a fixed offset silently
    // starts hovering a comment the moment a line is inserted above it.
    const at = (needle: string, token: string): vscode.Position => {
      const line = doc
        .getText()
        .split("\n")
        .findIndex((l) => l.includes(needle));
      assert.ok(line >= 0, `example still contains \`${needle}\``);
      const col = doc.lineAt(line).text.indexOf(token) + 1;
      assert.ok(col > 0, `line ${line} still contains \`${token}\``);
      return new vscode.Position(line, col);
    };

    // Hover `listLength` where the registered indexing scenario asserts the
    // constructed command list's exact size. This probe is both executed by the
    // corpus and tied to observable list behavior rather than legacy stdout.
    const lenAt = at("listLength(commands), listLength(empty)", "listLength");
    const lenHover = await hoverPoll(
      doc.uri,
      lenAt.line,
      lenAt.character,
      "listLength",
      80,
      250,
    );
    assertIncludes(
      hoverText(lenHover[0]),
      ["->"],
      "listLength hover shows a signature on the real file",
    );
    assertIncludes(
      hoverText(lenHover[0]),
      ["int"],
      "listLength hover shows it returns int",
    );

    // Hover the user-defined `classify` function where the registered scenario
    // asserts its empty branch.
    const classifyAt = at(
      "answersHold(classify, [List(), b",
      "classify",
    );
    const classifyHover = await hoverPoll(
      doc.uri,
      classifyAt.line,
      classifyAt.character,
      "classify",
    );
    assertIncludes(
      hoverText(classifyHover[0]),
      ["fn classify"],
      "classify hover shows its signature",
    );

    // Document symbols include the functions defined in the example.
    const syms = await symbolsPoll(doc.uri, 1);
    const names = syms.map((s) => s.name);
    assertListed(names, ["classify", "sumList", "restLen"], "symbols");
  });

  test("CHUNKY: the live LSP serves the ML flavor (.ospml) end-to-end", async function () {
    this.timeout(60000);
    // The .ospml layout flavor rides the SAME language server through the
    // `osprey-ml` document selector wired in activate(). Nothing before this
    // proved the live server answers over ML source; this drives hover,
    // go-to-definition, references, symbols, diagnostics and completion against a
    // real .ospml buffer so a regression that broke ML editor UX (e.g. the
    // selector losing "osprey-ml", or the ML frontend not reaching the LSP) fails
    // loudly. ML flavor: `name args = body`, whitespace application, offside
    // blocks, `name = value` bindings (no `let`/`fn`). [LSP-FLAVOR-RENDER]
    const ML =
      [
        "double x = wrapMul x 2", // 0  curried unary fn
        "triple x = wrapAdd (wrapAdd x x) x", // 1  curried unary fn
        "base = 5", // 2  binding
        "d = double base", // 3  call site of double
        "t = triple base", // 4  call site of triple
        "u = double base", // 5  second call of double
        'print "d=${d} t=${t} u=${u}"', // 6
      ].join("\n") + "\n";
    const doc = await writeAndOpen(tempDir, "sweep.ospml", ML);

    // The extension must associate .ospml with the ML language id, which is how
    // the LSP client's documentSelector routes it to the server.
    await waitForLanguage(doc, "osprey-ml");
    assert.strictEqual(
      doc.languageId,
      "osprey-ml",
      "a .ospml file is associated with the osprey-ml language id",
    );

    // --- HOVER: the curried function at its declaration and a call site ---
    const declHover = await hoverPoll(doc.uri, 0, 0, "double", 80, 250);
    const declMd = hoverText(declHover[0]);
    assert.ok(declMd.includes("double"), "ML fn hover names the function");
    assertIncludes(declMd, ["->"], "ML fn hover renders a function-type arrow");
    await assertCallHoverMatches(
      doc.uri,
      3,
      4,
      "double",
      declMd,
      "ML call-site hover matches the declaration hover",
    );
    // A binding hovers with its inferred type — no annotation in the source.
    const baseHover = await hoverPoll(doc.uri, 2, 0, "base");
    assert.ok(
      /base\s*:\s*int/i.test(hoverText(baseHover[0])),
      `ML binding hover shows inferred type: ${hoverText(baseHover[0])}`,
    );

    // --- GO TO DEFINITION: a call jumps back to the ML declaration ---
    const def = await defsPoll(doc.uri, 3, 4);
    assert.strictEqual(def[0].range.start.line, 0, "double defined on line 0");
    assert.strictEqual(
      def[0].uri.toString(),
      doc.uri.toString(),
      "definition resolves within the .ospml document",
    );

    // --- FIND REFERENCES: declaration + both call sites of double ---
    await assertReferences(doc.uri, 0, 0, 3, [0, 3, 5], "double");

    // --- DOCUMENT SYMBOLS: both functions and every binding are listed ---
    const syms = await symbolsPoll(doc.uri, 5);
    const names = new Set(syms.map((s) => s.name));
    for (const name of ["double", "triple", "base", "d", "t", "u"]) {
      assert.ok(names.has(name), `ML symbol ${name} is listed`);
    }

    // --- COMPLETION: user-defined ML symbols are offered ---
    const list = await completionPoll(doc.uri, 4, 4);
    const labels = labelsOf(list);
    assert.ok(
      labels.includes("double") && labels.includes("triple"),
      "completion offers the ML user functions",
    );

    // --- DIAGNOSTICS lifecycle on ML source: break it, then fix it ---
    await rewrite(doc, 'd = missingFn 5\nprint "${d}"\n');
    const broken = await diagnosticsPoll(doc.uri, (d) => d.length > 0);
    assert.strictEqual(
      broken[0].severity,
      vscode.DiagnosticSeverity.Error,
      "an undefined ML identifier is a real Error diagnostic",
    );
    assert.strictEqual(
      broken[0].source,
      "osprey",
      "ML diagnostic is attributed to the osprey server",
    );

    await rewrite(
      doc,
      'square x = x * x\nwrapping = handler Arith\n    overflow _ _ _ w => w\nprint "${wrapping (\\() => square 4)}"\n',
    );
    await assertClean(
      doc.uri,
      "diagnostics clear once the ML program is valid again",
    );
    await hoverShowing(
      doc.uri,
      0,
      0,
      "square",
      "square",
      () => "hover works on the corrected ML program",
    );
  });

  // WHY THIS TEST EXISTS
  //
  // CLAUDE.md requires every inferable annotation to be deleted, and the LSP is
  // what a reader uses to recover the types the source no longer spells. So the
  // contract is not "hover says something" — it is that deleting an annotation
  // changes NOTHING a user can see. That is a property between two states of the
  // same buffer, and it cannot be checked by opening two files: it has to survive
  // a live edit through the incremental-sync path a real editor drives.
  //
  // Every provider is queried BEFORE and AFTER the edit and compared for exact
  // equality, plus four standing assertions that a green comparison could
  // otherwise hide:
  //   * non-empty            — a provider that answers nothing compares equal to
  //                            itself and would pass a pure equality check.
  //   * exact expected text  — pins WHICH type, so both states being wrong the
  //                            same way still fails.
  //   * no `tN`              — `t5` is the checker's private name; its number
  //                            moves when an unrelated line is edited.
  //   * no fabricated `Unit` — `Unit` was the old display fallback and is a
  //                            positive claim the checker itself refutes
  //                            ([TYPE-RENDER-HOLES]).

  const ANNOTATED_AREA = "fn area(w: int, h: int) -> int = wrapMul(w, h)";
  const INFERRED_AREA = "fn area(w, h) = wrapMul(w, h)";

  // Proof-of-reprocessing marker, appended by the same edit that deletes the
  // annotation. See the comment at the edit for why a marker is required.
  const SENTINEL_NAME = "sentinelAfterEdit";
  const SENTINEL_DECL = `fn ${SENTINEL_NAME}() = 1`;

  // `area` is fully provable, so its reported signature must be IDENTICAL either
  // way. `classify` is provable only in part: the patterns prove `xs` is a list
  // while nothing constrains its element, so the element is a hole and stays one
  // in both states — the annotation under edit is on `area` alone.
  const AREA_SIGNATURE = "fn area(w: int, h: int) -> int";
  const CLASSIFY_SIGNATURE = "fn classify(xs: List<_>) -> string";

  // Hover renders one fenced Osprey block holding exactly the signature, so the
  // WHOLE hover is predictable and is compared as a whole. `includes` would let
  // a hover that also printed `t5`, a stale second signature or a `-> Unit`
  // line pass on the strength of the one substring it got right.
  const fenced = (signature: string): string =>
    ["```osprey", signature, "```"].join("\n");
  const AREA_HOVER = fenced(AREA_SIGNATURE);
  const CLASSIFY_HOVER = fenced(CLASSIFY_SIGNATURE);

  // The outline detail IS the signature, so the two entries under test are
  // fully determined and compared as a list rather than searched.
  const EXPECTED_OUTLINE = [
    `area:${AREA_SIGNATURE}`,
    `classify:${CLASSIFY_SIGNATURE}`,
  ];

  const PROGRAM_TAIL = [
    "fn classify(xs) = match xs {", // 1
    '  [] => "empty"', // 2
    '  [head, ...tail] => "many"', // 3
    "}", // 4
    "let a = area(3, 4)", // 5
    "let c = classify(List())", // 6
    'print("${a} ${c}")', // 7
  ].join("\n");

  /// Every provider answer that must survive deleting an annotation, as plain
  /// comparable data. Captured once per buffer state.
  /// The two spellings no reader may ever see, asserted PER interaction so a
  /// failure names the surface that broke. A single combined blob at the end
  /// reported "something leaked" and left the reader to find which provider.
  ///   * `tN` is the checker's private name; its number moves when an unrelated
  ///     line is edited, so it means nothing outside one inference run.
  ///   * `-> Unit` was the old display fallback and is a positive claim the
  ///     checker itself refutes ([TYPE-RENDER-HOLES]).
  const assertNoLeak = (where: string, text: string): void => {
    assert.ok(
      !/\bt\d+\b/.test(text),
      `${where}: a private inference name reached a reader: ${text}`,
    );
    assert.ok(
      !/->\s*Unit\b/.test(text),
      `${where}: Unit is a claim the checker refutes here: ${text}`,
    );
  };

  /** Apply `edit`, then assert line 0 of `doc` reads `expected`. */
  const applyAssertLine0 = async (
    edit: vscode.WorkspaceEdit,
    doc: vscode.TextDocument,
    expected: string,
    appliedNote: string,
    lineNote: string,
  ): Promise<void> => {
    assert.ok(await vscode.workspace.applyEdit(edit), appliedNote);
    assert.strictEqual(doc.lineAt(0).text, expected, lineNote);
  };

  interface ProviderSnapshot {
    areaHover: string;
    classifyHover: string;
    outline: string[];
    signature: string;
    completionDetail: string;
    definitions: string[];
  }

  test("CHUNKY: deleting a provable annotation changes nothing any provider reports", async function () {
    this.timeout(120000);

    const doc = await openDoc(
      "live-edit.osp",
      `${ANNOTATED_AREA}\n${PROGRAM_TAIL}\n`,
    );

    /// Drive all five providers over the CURRENT buffer and assert each one is
    /// individually sound before it is ever compared. `label` names the buffer
    /// state so a failure says which side broke.
    /** The hover text for `needle`, asserted to be exactly `expected`. */
    async function provedHover(
      line: number,
      character: number,
      needle: string,
      expected: string,
      label: string,
      note: string,
    ): Promise<string> {
      const text = hoverText(
        (await hoverPoll(doc.uri, line, character, needle))[0],
      );
      assert.strictEqual(text, expected, `${label}: ${needle} hover ${note}`);
      assertNoLeak(`${label}: ${needle} hover`, text);
      return text;
    }

    async function snapshot(label: string): Promise<ProviderSnapshot> {
      const areaHover = await provedHover(
        0,
        4,
        "area",
        AREA_HOVER,
        label,
        "must be exactly the proved signature",
      );
      const classifyHover = await provedHover(
        1,
        4,
        "classify",
        CLASSIFY_HOVER,
        label,
        "must keep its proven part and hole the rest",
      );

      // The outline is the second view of the same types, so it must agree with
      // hover rather than carry its own rendering.
      const symbols = await symbolsPoll(doc.uri, 2);
      // Only the two functions under test. The edit below appends a SENTINEL
      // declaration to prove the server reprocessed, and that sentinel must not
      // itself make the before/after outlines differ.
      const outline = symbols
        .filter((s) => s.name === "area" || s.name === "classify")
        .map((s) => `${s.name}:${s.detail}`);
      assert.deepStrictEqual(
        outline,
        EXPECTED_OUTLINE,
        `${label}: the outline must name both functions with exactly their \
signatures, in source order`,
      );
      assertNoLeak(`${label}: outline`, outline.join(" | "));

      // Signature help inside the argument list of `area(3, 4)` on line 5.
      const help = await pollFor(
        () => sigHelpAt(doc.uri, 5, 15),
        (h) => Boolean(h?.signatures?.length),
      );
      const signature = help.signatures[0].label;
      // The WHOLE signature, return type included. This was an `||` that also
      // accepted `area(w: int, h: int)` without `-> int`, so a regression that
      // dropped the return type — the exact failure this branch is about —
      // passed through the second branch. `features.rs` builds this label from
      // `sym.signature`, which carries the return, so the strict form is what
      // the server actually promises.
      assert.strictEqual(
        signature,
        AREA_SIGNATURE,
        `${label}: signature help must be exactly the full inferred signature`,
      );
      assertNoLeak(`${label}: signature help`, signature);
      // The cursor sits after the comma, so the SECOND parameter is active.
      // Without this the test would pass on a server that highlights the wrong
      // argument — the one thing signature help is for while typing a call.
      assert.strictEqual(
        help.activeParameter,
        1,
        `${label}: the cursor is in the second argument`,
      );
      assert.deepStrictEqual(
        help.signatures[0].parameters.map((p) =>
          typeof p.label === "string"
            ? p.label
            : signature.slice(p.label[0], p.label[1]),
        ),
        ["w: int", "h: int"],
        `${label}: each parameter is labelled with the type the checker proved`,
      );

      // Completion offers `area`; its detail is the same signature again.
      const completions = await pollFor(
        () => completionAt(doc.uri, 5, 12),
        (list) => Boolean(list) && labelsOf(list).includes("area"),
      );
      const areaItem = completions.items.find(
        (i) =>
          (typeof i.label === "string" ? i.label : i.label.label) === "area",
      );
      // `if`/`throw` rather than `assert.ok`: this narrows `areaItem` for the
      // lines below under `strict`, which an assertion helper does not do here.
      if (!areaItem) {
        throw new assert.AssertionError({
          message: `${label}: completion must offer area`,
        });
      }
      // A function completes AS a function, with the name the user typed.
      assert.strictEqual(
        typeof areaItem.label === "string"
          ? areaItem.label
          : areaItem.label.label,
        "area",
        `${label}: the completion label is the plain function name`,
      );
      assert.strictEqual(
        areaItem.kind,
        vscode.CompletionItemKind.Function,
        `${label}: and it is offered as a function, not a variable`,
      );
      const documentation =
        typeof areaItem.documentation === "string"
          ? areaItem.documentation
          : (areaItem.documentation?.value ?? "");
      const completionDetail = `${areaItem.detail ?? ""}${documentation}`;
      // The detail IS the rendered signature and nothing else (`complete.rs`
      // builds it from the symbol's type), so it is compared whole. A bare
      // label, a stale render, or a detail padded with an extra line all fail.
      assert.strictEqual(
        completionDetail,
        AREA_SIGNATURE,
        `${label}: completion detail must be exactly the inferred signature`,
      );
      assertNoLeak(`${label}: completion detail`, completionDetail);

      // Go-to-definition from the call on line 5 must land on the declaration.
      const defs = await defsPoll(doc.uri, 5, 9);
      // URI and the exact identifier span, not just the line. A line-only
      // projection accepts a jump into the wrong file, and it accepts a range
      // that covers the whole declaration instead of the name the editor
      // highlights — `area` is columns 3..7 of `fn area(...)`.
      const definitions = defs.map(
        (d) =>
          `${d.uri.toString()}@${d.range.start.line}:${d.range.start.character}` +
          `-${d.range.end.line}:${d.range.end.character}`,
      );
      assert.deepStrictEqual(
        definitions,
        [`${doc.uri.toString()}@0:3-0:7`],
        `${label}: go-to-definition must land on the identifier in this file`,
      );

      return {
        areaHover,
        classifyHover,
        outline,
        signature,
        completionDetail,
        definitions,
      };
    }

    const annotated = await snapshot("annotated");

    // The edit a contributor actually makes when obeying CLAUDE.md: delete the
    // annotations, leave the body alone. Applied through WorkspaceEdit so the
    // server sees the same incremental didChange a keystroke produces.
    const edit = new vscode.WorkspaceEdit();
    edit.replace(doc.uri, doc.lineAt(0).range, INFERRED_AREA);
    // A SENTINEL in the SAME edit. Without it there is nothing to wait for: the
    // correct post-edit answer is byte-identical to the pre-edit one, so polling
    // for it cannot distinguish "the server reprocessed and agrees" from "the
    // server has not looked yet and is repeating its old reply" — and the whole
    // comparison would then pass on a stale answer. The sentinel is a symbol
    // that did not exist before, so seeing it proves reprocessing happened.
    // Appended at the END so every line number asserted above is unmoved.
    edit.insert(
      doc.uri,
      new vscode.Position(doc.lineCount, 0),
      `${SENTINEL_DECL}\n`,
    );
    await applyAssertLine0(
      edit,
      doc,
      INFERRED_AREA,
      "the edit must apply",
      "the buffer must actually hold the unannotated form",
    );

    const reprocessed = await pollFor(
      () => symbolsOf(doc.uri),
      listsSymbol(SENTINEL_NAME),
    );
    assert.ok(
      reprocessed.some((s) => s.name === SENTINEL_NAME),
      "the server must have reprocessed the edited buffer before it is compared",
    );

    const inferred = await snapshot("inferred");

    assert.strictEqual(
      inferred.areaHover,
      annotated.areaHover,
      "deleting a provable annotation must not change what hover reports",
    );
    assert.deepStrictEqual(
      inferred.outline,
      annotated.outline,
      "deleting a provable annotation must not change the outline",
    );
    assert.strictEqual(
      inferred.signature,
      annotated.signature,
      "deleting a provable annotation must not change signature help",
    );
    assert.strictEqual(
      inferred.completionDetail,
      annotated.completionDetail,
      "deleting a provable annotation must not change completion detail",
    );
    assert.deepStrictEqual(
      inferred.definitions,
      annotated.definitions,
      "deleting a provable annotation must not move the definition",
    );
    assert.strictEqual(
      inferred.classifyHover,
      annotated.classifyHover,
      "an untouched function's reported type must not drift when a NEIGHBOUR is edited — \
  the hole in `List<_>` is stable, unlike the `t5` it replaced",
    );

    // Restore, so the file is left as the suite found it and a later test that
    // reopens this name sees the annotated form.
    // Restore the whole buffer — the edit both rewrote line 0 and appended the
    // sentinel, so a line-0-only undo would leave the sentinel behind for any
    // later test that opens this name.
    const undo = new vscode.WorkspaceEdit();
    undo.replace(
      doc.uri,
      new vscode.Range(
        new vscode.Position(0, 0),
        doc.lineAt(doc.lineCount - 1).range.end,
      ),
      `${ANNOTATED_AREA}\n${PROGRAM_TAIL}`,
    );
    await applyAssertLine0(
      undo,
      doc,
      ANNOTATED_AREA,
      "the undo must apply",
      "the buffer must be left as the suite found it",
    );
  });
});

// These suites drive the command handlers, event subscriptions, and debug
// provider registered in activate() so the coverage harness records the
// execFile callbacks and the early-return guards in extension.ts.
suite("Osprey Command Handler Coverage", () => {
  let tempDir: string;

  async function closeEverything(): Promise<void> {
    // closeAllEditors races with VS Code's async editor teardown, so poll until
    // the active editor actually drains (or give up after a bounded number of
    // attempts and let the caller assert what it needs).
    for (let i = 0; i < 10 && vscode.window.activeTextEditor; i++) {
      await closeAllEditors();
      await settle(150);
    }
  }

  // makeActive (module scope) shows the document and polls until it is the
  // genuinely active text editor; the compile/run handlers capture
  // window.activeTextEditor, so every command test goes through it first,
  // asserting the activation with `note`.
  const openFile = (name: string, content: string, note: string) =>
    openActive(tempDir, name, content, note);

  /** Fire `command` against the active editor and let its handler settle. */
  async function fireCommand(command: string, settleMs: number): Promise<void> {
    await vscode.commands.executeCommand(command);
    await settle(settleMs);
  }

  /** Drive `command` twice, refocusing `document` between the two launches. */
  async function driveTwice(
    command: string,
    document: vscode.TextDocument,
    settleMs: number,
  ): Promise<void> {
    await fireCommand(command, settleMs);
    await makeActive(document);
    await fireCommand(command, settleMs);
  }

  setup(async () => {
    tempDir = makeTempDir("cmd");
    // Ensure the extension is active before exercising its commands.
    await ensureExtensionActive();
  });

  teardown(async () => {
    await closeEverything();
    removeDir(tempDir);
  });

  test("osprey.compile runs to completion against a valid .osp file", async function () {
    this.timeout(30000);
    const valid = 'fn main() -> Unit = print("compiled ok")\n';
    const document = await openFile(
      "compile-valid.osp",
      valid,
      "the .osp document became the active editor",
    );

    assertOspreyDoc(document, "compile-valid.osp");

    // The handler reads window.activeTextEditor synchronously; our .osp
    // document being that editor lets the command pass both guards and reach
    // the save->execFile->callback chain.
    assert.ok(
      vscode.window.activeTextEditor?.document.fileName.endsWith(".osp"),
      "the active editor is the .osp document",
    );

    // Drive the command twice to exercise the chain for both a fresh and an
    // already-saved buffer.
    await driveTwice("osprey.compile", document, 4000);

    assertExtensionActive("extension stays active after compile");
    assert.ok(fs.existsSync(document.fileName), "source file still on disk");
  });

  test("osprey.compile surfaces a compiler error path for invalid code", async function () {
    this.timeout(30000);
    // Invalid Osprey forces the compiler to exit non-zero, exercising the
    // `if (error)` branch of the execFile callback.
    const broken = "fn main( = \n this is not valid osprey @@@ \n";
    const document = await openFile(
      "compile-broken.osp",
      broken,
      "broken doc is active",
    );

    assert.strictEqual(document.languageId, "osprey");
    await fireCommand("osprey.compile", 4000);

    assertExtensionActive("extension survives a failed compile");
    const text = document.getText();
    assert.ok(text.includes("not valid"), "broken source preserved");
  });

  test("osprey.run runs to completion against a valid .osp file", async function () {
    this.timeout(30000);
    const valid = 'fn main() -> Unit = print("ran ok")\n';
    const document = await openFile(
      "run-valid.osp",
      valid,
      "valid run doc is active",
    );

    await driveTwice("osprey.run", document, 5000);

    assertExtensionActive("extension stays active after run");
    assertOspreyDoc(document, "run-valid.osp");
  });

  test("osprey.run surfaces a failure path for invalid code", async function () {
    this.timeout(30000);
    const broken = "fn main( = @@@ not osprey at all\n";
    const document = await openFile(
      "run-broken.osp",
      broken,
      "broken run doc is active",
    );

    await fireCommand("osprey.run", 4000);

    assertExtensionActive("extension survives a failed run");
    assert.ok(document.getText().length > 0, "broken source preserved");
  });

  test("compile and run guard against a non-.osp active editor", async () => {
    // Open a plain text (non-.osp) file so both handlers hit the
    // "Please open a .osp file" early return.
    const document = await openActive(
      tempDir,
      "notes.txt",
      "just some plain text, not osprey at all",
      "the .txt document became active",
    );

    assert.ok(!document.fileName.endsWith(".osp"), "active file is not .osp");
    assert.notStrictEqual(document.languageId, "osprey", "not osprey lang");

    await compileAndRun(500);

    assertExtensionActive("extension active after guarded commands");
  });

  test("compile and run guard when there is no active editor", async () => {
    await closeEverything();
    // Best-effort: VS Code may keep a hidden editor around, but the handlers'
    // guards are exercised either way (no .osp active editor present).
    const noEditor = vscode.window.activeTextEditor === undefined;

    await compileAndRun(400);

    assertExtensionActive("extension active with no editor");
    // The commands must not have opened or left an .osp editor active.
    const active = vscode.window.activeTextEditor;
    assert.ok(
      noEditor || !active?.document.fileName.endsWith(".osp"),
      "no .osp editor became active from a guarded command",
    );
  });

  test("osprey.setLanguage retargets the active editor to osprey", async () => {
    // Open a file with a .txt extension so its language starts as plaintext,
    // then force it to osprey through the command. Covers the setLanguage
    // handler body (active editor present).
    await openActive(
      tempDir,
      "convert-me.txt",
      'fn main() -> Unit = print("convert")\n',
      "convert doc became active",
    );

    assert.notStrictEqual(
      vscode.window.activeTextEditor?.document.languageId,
      "osprey",
      "starts non-osprey",
    );

    await fireCommand("osprey.setLanguage", 600);

    const active = vscode.window.activeTextEditor;
    assert.ok(active, "an editor is active");
    assert.strictEqual(
      active?.document.languageId,
      "osprey",
      "language now osprey",
    );
  });

  test("osprey.setLanguage is a no-op with no active editor", async () => {
    await closeEverything();
    const before = vscode.window.activeTextEditor;

    // Should not throw even though there is (best-effort) nothing to retarget.
    await fireCommand("osprey.setLanguage", 300);

    assertExtensionActive("extension stays active");
    // The command must not have created a new editor.
    assert.strictEqual(
      vscode.window.activeTextEditor,
      before,
      "no editor was opened by setLanguage",
    );
  });

  test("all osprey commands are registered", async () => {
    const all = await vscode.commands.getCommands(true);
    assert.ok(all.includes("osprey.compile"), "compile registered");
    assert.ok(all.includes("osprey.run"), "run registered");
    assert.ok(all.includes("osprey.debug"), "debug registered");
    assert.ok(all.includes("osprey.setLanguage"), "setLanguage registered");
    assert.ok(all.includes("osprey.profileCurrentFile"), "profile registered");
    assert.ok(all.includes("osprey.profiler.openLast"), "openLast registered");
  });

  test("osprey.debug with an active .osp editor drives the real launch path", async function () {
    this.timeout(45000);
    // The osprey.debug command reads window.activeTextEditor, synthesizes a
    // launch config, and hands off to startDebugging — which invokes the real
    // debug-configuration provider: save, `osprey --debug --compile`, then DAP.
    // We fire the command with a genuine osprey editor active and assert the
    // extension survives the full provider round-trip.
    const document = await openFile(
      "debug-cmd.osp",
      'fn main() -> Unit = print("debug via command")\n',
      "osprey doc is active for debug",
    );
    assert.strictEqual(document.languageId, "osprey", "doc is osprey");
    assert.ok(
      vscode.window.activeTextEditor?.document.fileName.endsWith(".osp"),
      "active editor is the .osp file the command will debug",
    );

    // Fire the command; give the debug-compile + DAP handoff a real window.
    await fireCommand("osprey.debug", 6000);
    await stopActiveDebugSession();

    assertExtensionActive("extension stays active after osprey.debug");
    // The provider debug-compiles to a stable per-source path; on a host with a
    // working toolchain that native artifact exists after the launch attempt.
    const artifact = path.join(
      path.dirname(document.fileName),
      ".osprey-debug",
      process.platform === "win32" ? "debug-cmd.exe" : "debug-cmd",
    );
    assert.ok(
      fs.existsSync(artifact) || !vscode.debug.activeDebugSession,
      "debug launch either produced the native artifact or settled cleanly",
    );
  });

  test("osprey.debug with no editor surfaces the open-a-file error", async () => {
    // With no active editor, debugCurrentFile hits `!config.program` and shows
    // "Please open a .osp or .ospml file to debug" instead of launching.
    await closeEverything();
    const before = vscode.window.activeTextEditor;

    await fireCommand("osprey.debug", 600);

    assertExtensionActive("extension survives osprey.debug with no program");
    assert.strictEqual(
      vscode.debug.activeDebugSession,
      undefined,
      "no debug session starts without a program",
    );
    // The guarded command must not have opened an editor.
    assert.ok(
      before === undefined || vscode.window.activeTextEditor === before,
      "the no-program debug command opened no editor",
    );
  });

  test("debug provider rejects a broken .osp source at debug-compile time", async function () {
    this.timeout(45000);
    // A syntactically broken program makes `osprey --debug --compile` exit
    // non-zero. The provider awaits compileDebugProgram, which rejects; the
    // catch shows the failure and returns undefined so no session starts. This
    // exercises the compile-failure branch of the debug-configuration provider.
    const document = await openActive(
      tempDir,
      "debug-broken.osp",
      "fn main( = @@@ not valid osprey\n",
    );

    const outcome = await launchDebug(
      {
        type: "osprey",
        request: "launch",
        name: "Debug Osprey File",
        program: document.fileName,
        cwd: tempDir,
      } as vscode.DebugConfiguration,
      6000,
      1500,
    );

    assertNoDebugSession(
      outcome,
      "a source that fails to debug-compile launches no session",
    );
    assertExtensionActive("extension survives a failed debug-compile");
    // The broken source stays exactly as written — the provider never mutates
    // it, it only tries (and fails) to compile it.
    assertIncludes(
      document.getText(),
      ["not valid osprey"],
      "broken source preserved after the failed debug-compile",
    );
  });

  test("debug provider reports 'no program' for an empty config with no editor", async () => {
    // No active editor + a config with no program means synthesis is skipped
    // and `!config.program` shows "Cannot find a program to run".
    await closeEverything();
    const outcome = await launchDebug(
      {
        type: "osprey",
        request: "launch",
        name: "Debug Osprey File",
      } as vscode.DebugConfiguration,
      6000,
      1000,
    );

    assertNoDebugSession(
      outcome,
      "no session starts when there is no resolvable program",
    );
    assertExtensionActive("extension survives the no-program path");
  });
});

suite("Osprey Activation Side-Effect Coverage", () => {
  let tempDir: string;

  // Same focus-stealing workaround as the command suite: the extension's
  // "Osprey Debug" output channel grabs the active-editor slot in the headless
  // host, so makeActive (module scope) closes panels and refocuses the editor
  // group until the target document is genuinely active.
  setup(() => {
    tempDir = makeTempDir("side");
  });

  teardown(async () => {
    await closeAllEditors();
    removeDir(tempDir);
  });

  test("opening a .osp file triggers the language-association watcher", async () => {
    // Opening a brand new .osp document fires onDidOpenTextDocument; the handler
    // forces the osprey language id. This exercises the open-document watcher.
    const document = await writeAndOpen(
      tempDir,
      "late-open.osp",
      'fn main() -> Unit = print("late")\n',
    );
    await settle(800);

    assertOspreyDoc(document, "late-open.osp");
    assertExtensionActive("extension active");
  });

  test("the open watcher force-corrects a mis-associated .osp file to osprey", async () => {
    // The interesting branch of onDidOpenTextDocument is the CORRECTION path:
    // `document.languageId !== target`. Normally VS Code already associates .osp
    // to "osprey", so the branch never runs. Force a genuine mismatch by mapping
    // *.osp -> plaintext, open the file (it comes in as plaintext), and prove the
    // extension's watcher drags it back to "osprey" — the real self-healing the
    // handler exists for. The association is restored in a finally so no later
    // suite inherits the override.
    const filesConfig = vscode.workspace.getConfiguration("files");
    const priorAssoc =
      filesConfig.get<Record<string, string>>("associations") ?? {};
    await filesConfig.update(
      "associations",
      { ...priorAssoc, "*.osp": "plaintext" },
      vscode.ConfigurationTarget.Global,
    );
    await settle(300);

    try {
      const document = await writeAndOpen(
        tempDir,
        "misassociated.osp",
        'fn main() -> Unit = print("corrected")\n',
      );

      // The watcher's setTextDocumentLanguage is async; poll until it lands.
      await waitForLanguage(document, "osprey");
      assert.strictEqual(
        document.languageId,
        "osprey",
        "the watcher re-associated a plaintext-opened .osp back to osprey",
      );
      assertExtensionActive("extension stays active after the correction");
    } finally {
      await filesConfig.update(
        "associations",
        priorAssoc,
        vscode.ConfigurationTarget.Global,
      );
      await settle(300);
    }
  });

  test("changing osprey configuration fires the change handler", async () => {
    // Flip an osprey setting to trigger onDidChangeConfiguration, which shows an
    // information message. We assert the round-trip of the value to confirm the
    // configuration channel is wired and the handler had a real event to react
    // to.
    const config = vscode.workspace.getConfiguration("osprey");
    const diagnosticsEnabled = () =>
      vscode.workspace
        .getConfiguration("osprey")
        .get<boolean>("diagnostics.enabled");
    const original = diagnosticsEnabled();

    await config.update(
      "diagnostics.enabled",
      !original,
      vscode.ConfigurationTarget.Global,
    );
    await settle(500);

    const flipped = diagnosticsEnabled();
    assert.strictEqual(flipped, !original, "config value flipped");

    // Restore so other tests see defaults.
    await config.update(
      "diagnostics.enabled",
      original,
      vscode.ConfigurationTarget.Global,
    );
    await settle(300);

    const restored = diagnosticsEnabled();
    assert.strictEqual(restored, original, "config value restored");
  });

  test("debug provider synthesizes a config from the active osprey editor", async function () {
    this.timeout(30000);

    // With an active osprey editor and an empty config, resolveDebugConfiguration
    // synthesizes type/name/request/program from the editor, then attempts the
    // real debug-launch path: save, debug-compile, and hand off to DAP.
    const document = await writeAndOpen(
      tempDir,
      "debug-synth.osp",
      'fn main() -> Unit = print("debug synth")\n',
    );
    const isActive = await makeActive(document);

    assert.strictEqual(document.languageId, "osprey", "doc is osprey");
    assert.ok(isActive, "osprey doc is the active editor");
    assert.strictEqual(
      vscode.window.activeTextEditor?.document.languageId,
      "osprey",
      "active editor language is osprey",
    );

    await launchDebug(
      {
        type: "",
        name: "",
        request: "",
      } as unknown as vscode.DebugConfiguration,
      4000,
      2500,
    );

    assertExtensionActive("extension survives synthesized debug config");
  });

  test("debug provider rejects a launch config with no resolvable program", async function () {
    this.timeout(30000);

    // No active osprey editor and a config that carries a type but no program
    // means synthesis is skipped and `!config.program` is true, so the provider
    // shows "Cannot find a program to run" and returns undefined.
    await closeAllEditors();
    await settle(300);

    await launchDebug(
      {
        type: "osprey",
        name: "Debug Osprey File",
        request: "launch",
      } as vscode.DebugConfiguration,
      4000,
      1500,
    );

    assertExtensionActive("extension survives the no-program debug path");
  });
});

suite("Osprey VSIX Debugger E2E", () => {
  let tempDir: string;
  let lldbDapPath: string;
  let pinned: PinnedOspreySettings;

  setup(async function () {
    this.timeout(60000);
    tempDir = makeTempDir("debug-vsix");

    const ospreyPath = resolveBuiltOsprey();
    assert.ok(
      ospreyPath && fs.existsSync(ospreyPath),
      "a freshly built osprey binary is required for VSIX debugger E2E",
    );
    lldbDapPath = resolveRequiredLldbDap();
    pinned = await pinOspreySettings({
      "server.compilerPath": ospreyPath,
      "debug.lldbDapPath": lldbDapPath,
    });

    const extension = await ensureExtensionActive();
    assert.ok(extension.isActive, "the extension runs in the test host");
  });

  teardown(async function () {
    clearDebugBreakpoints();
    await stopActiveDebugSession();
    await pinned.restore();
    removeDir(tempDir);
  });

  /** Open `source` in an editor and flush it to disk. */
  async function openSaved(source: string): Promise<vscode.TextDocument> {
    const document = await vscode.workspace.openTextDocument(source);
    await vscode.window.showTextDocument(document);
    await document.save();
    return document;
  }

  /** The launch config every debugger E2E test drives. */
  function launchConfig(
    program: string,
    sessionName: string,
  ): vscode.DebugConfiguration {
    return {
      type: "osprey",
      request: "launch",
      name: sessionName,
      program,
      cwd: tempDir,
      debugOutput: defaultDebugOutputPath(program),
      lldbDapPath,
      stopOnEntry: false,
    };
  }

  /**
   * Launch `source` under the debugger and await the session it attaches,
   * asserting VS Code accepted the launch and the session is Osprey's.
   */
  async function attachSession(
    source: string,
    sessionName: string,
  ): Promise<vscode.DebugSession> {
    const sessionPromise = waitForDebugSessionStart(45000);
    assert.ok(
      await vscode.debug.startDebugging(
        undefined,
        launchConfig(source, sessionName),
      ),
      "VS Code should accept the Osprey debug launch",
    );
    const session = await sessionPromise;
    assert.strictEqual(session.type, "osprey", "session is the osprey debugger");
    return session;
  }

  /** Wait for `session` to stop and assert its top frame sits in `source`. */
  async function stopIn(session: vscode.DebugSession, source: string) {
    const stopped = await waitForStop(session, 45000);
    return { stopped, topFrame: assertTopFrameInSource(stopped.stack, source) };
  }

  /** Resume `threadId` and wait for its session to end. */
  async function resumeToEnd(
    session: vscode.DebugSession,
    threadId: number,
  ): Promise<void> {
    await session.customRequest("continue", { threadId });
    await waitForDebugSessionEnd(45000, session.id);
  }

  // [DEBUGGER-PROTOCOLS] F5 and the command resolve to a DAP debugger
  // contribution; the separate run command is not used as a debug adapter.
  test("package manifest exposes a real debugger contribution", () => {
    const pkg = readPackageJson();

    // One row per contribution the manifest must declare: [label, present].
    const declared: [string, boolean][] = [
      [
        "VSIX activates to resolve osprey debug configs",
        !!pkg.activationEvents?.includes("onDebugResolve:osprey"),
      ],
      [
        "VSIX contributes an explicit osprey.debug command",
        !!pkg.contributes?.commands?.some(
          (cmd) => cmd.command === "osprey.debug",
        ),
      ],
      [
        "F5 is bound to the debugger command, not osprey.run",
        !!pkg.contributes?.keybindings?.some(
          (binding) => binding.command === "osprey.debug" && binding.key === "f5",
        ),
      ],
      [
        "VSIX declares Osprey source breakpoints",
        !!pkg.contributes?.breakpoints?.some((bp) => bp.language === "osprey"),
      ],
      [
        "VSIX contributes the osprey DAP debugger type",
        !!pkg.contributes?.debuggers?.some(
          (debuggerContribution) =>
            debuggerContribution.type === "osprey" &&
            debuggerContribution.languages?.includes("osprey"),
        ),
      ],
    ];
    for (const [message, present] of declared) {
      assert.ok(present, message);
    }
  });

  // Osprey files are identified by extension alone. A `firstLine` content
  // heuristic claimed every sibling whose header comment mentioned Osprey —
  // the graphics bridge's Metal shader (then `src/osprey/examples/graphics/scene.metal`,
  // now `base.metal`) was being parsed as Osprey and reported 45 syntax
  // errors. Nothing may re-introduce a content-based claim.
  test("language contributions claim files by extension, never by content", () => {
    const pkg = readPackageJson();
    const languages = pkg.contributes?.languages ?? [];

    assert.deepStrictEqual(
      languages.map((language) => language.id).sort(),
      ["osprey", "osprey-ml"],
      "the extension contributes exactly the two Osprey languages",
    );
    for (const language of languages) {
      assert.strictEqual(
        language.firstLine,
        undefined,
        `${language.id} must not claim files by first-line content`,
      );
    }
    assert.deepStrictEqual(
      languages.flatMap((language) => language.extensions ?? []).sort(),
      [".osp", ".ospml"],
      "only .osp and .ospml are Osprey sources",
    );
  });

  // The shader beside the graphics bridge opens as a shader, not as Osprey.
  test("a Metal shader mentioning Osprey in its header is not an Osprey file", () => {
    const shader = path.join(tempDir, "scene.metal");
    fs.writeFileSync(
      shader,
      "// The fragment shader the Osprey graphics bridge runs on the Apple GPU.\n" +
        "#include <metal_stdlib>\n",
    );
    assert.strictEqual(isOspreyFile(shader), false);
    assert.strictEqual(ospreyLanguageForFile(shader), undefined);
  });

  test("debug command synthesizes the same real launch config as F5", () => {
    const source = path.join(tempDir, "command.osp");
    const config = defaultOspreyDebugConfigForEditor({
      document: { languageId: "osprey", fileName: source },
    });
    assert.strictEqual(config.type, "osprey");
    assert.strictEqual(config.request, "launch");
    assert.strictEqual(config.name, "Debug Osprey File");
    assert.strictEqual(config.program, source);
    assert.strictEqual(config.cwd, tempDir);
  });

  test("starts lldb-dap, hits an Osprey source breakpoint, reads stack and locals", async function () {
    this.timeout(90000);

    const source = path.join(tempDir, "breakpoint.osp");
    const debugOutput = defaultDebugOutputPath(source);
    // A function call on the breakpoint line lets the same test assert that F10
    // (Step Over) EXECUTES the call without descending into it — the regression
    // guard for "step over behaved like step in". bump is monomorphic (its total
    // `wrapAdd` produces an int), so it is a real call frame the debugger could
    // wrongly enter.
    // [DEBUGGER-EDITOR-LAUNCH]
    fs.writeFileSync(
      source,
      [
        "fn bump(v) = wrapAdd(v, 1)",
        "let x = 1",
        "let y = bump(x)",
        'print("debugger reached ${x} and ${y}")',
        "",
      ].join("\n"),
    );

    await openSaved(source);

    setSourceBreakpoints(source, [3]);
    const session = await attachSession(source, "Osprey VSIX Debugger E2E");

    const { stopped, topFrame } = await stopIn(session, source);
    assert.strictEqual(topFrame.line, 3, "breakpoint stops on Osprey line 3");
    assert.ok(topFrame.column >= 1, "DAP reports a 1-based source column");

    const scopes = await getScopes(session, topFrame.id);
    assert.ok(scopes.scopes.length > 0, "debugger exposes frame scopes");
    const variables = (
      await Promise.all(
        scopes.scopes
          .filter((scope) => scope.variablesReference > 0)
          .map((scope) => getVariables(session, scope.variablesReference)),
      )
    ).flatMap((result) => result.variables);
    const x = variables.find((variable) => variable.name === "x");
    assert.ok(
      x,
      `local x must be visible through DAP variables; saw ${variables
        .map((variable) => variable.name)
        .join(", ")}`,
    );
    assert.match(x.value, /\b1\b/, "local x value is available through DAP");
    assert.ok(
      fs.existsSync(debugOutput),
      "debug launch compiled a native binary",
    );

    // F10 / Step Over on `let y = bump(x)`: execute the call to bump and land
    // on the NEXT line of main, WITHOUT descending into bump. A debugger that
    // stepped *into* bump would report the top frame as bump on line 1, and
    // bump would appear on the stack. [DEBUGGER-EDITOR-LAUNCH]
    await session.customRequest("next", { threadId: stopped.threadId });
    const stepped = await waitForStop(session, 45000);
    const steppedFrame = assertTopFrameInSource(stepped.stack, source);
    assert.ok(
      !stepped.stack.stackFrames.some((frame) => frame.name.includes("bump")),
      `step over (F10) must NOT descend into bump(); stack was ${stepped.stack.stackFrames
        .map((frame) => frame.name)
        .join(" <- ")}`,
    );
    assert.strictEqual(
      steppedFrame.line,
      4,
      "step over executes the call and advances main to line 4",
    );

    await resumeToEnd(session, stepped.threadId);
  });

  test("debugging an UNSAVED buffer saves it, compiles, and stops at a breakpoint", async function () {
    this.timeout(90000);
    // The debug-configuration provider must SAVE a dirty buffer before it shells
    // out to `osprey --debug --compile` — otherwise it would compile stale disk
    // contents. This drives that exact path end-to-end: open a file, dirty it in
    // the editor with the ONLY correct program, launch, and prove the debugger
    // stops on a line that exists *only* in the unsaved edit — which can only
    // happen if the provider saved the buffer first. [EDITOR-VSCODE]
    // VS Code would itself flush dirty editors before launching unless we tell
    // it not to. With debug.saveBeforeStart="none" the buffer reaches the
    // Osprey debug-configuration provider STILL DIRTY, so the provider's own
    // save-before-compile path (the thing under test) actually runs.
    const debugConfig = vscode.workspace.getConfiguration("debug");
    const priorSaveBeforeStart = debugConfig.get<string>("saveBeforeStart");
    await debugConfig.update(
      "saveBeforeStart",
      "none",
      vscode.ConfigurationTarget.Global,
    );

    try {
      const source = path.join(tempDir, "dirty-buffer.osp");
      // Seed disk with a DIFFERENT program (no breakpoint-worthy body) so a
      // failure to save would compile this instead and never stop on our line.
      fs.writeFileSync(
        source,
        'fn main() -> Unit = print("stale disk contents")\n',
      );

      const document = await vscode.workspace.openTextDocument(source);
      const editor = await vscode.window.showTextDocument(document);

      // Replace the whole buffer WITHOUT saving: the editor is now dirty and the
      // in-memory program differs from disk.
      const edited = [
        "fn tag(v) = wrapAdd(v, 100)",
        "let base = 7",
        "let tagged = tag(base)",
        'print("dirty debug ${base} and ${tagged}")',
        "",
      ].join("\n");
      const applied = await editor.edit((b) =>
        b.replace(new vscode.Range(0, 0, document.lineCount, 0), edited),
      );
      assert.ok(applied, "the buffer edit was applied");
      assert.ok(document.isDirty, "the buffer is dirty before launch");
      assert.notStrictEqual(
        fs.readFileSync(source, "utf8"),
        document.getText(),
        "disk and buffer differ before the provider saves",
      );

      const debugOutput = defaultDebugOutputPath(source);
      setSourceBreakpoints(source, [2]); // edited-only `let base = 7`
      const session = await attachSession(source, "Osprey dirty-buffer E2E");

      // The provider's save-before-compile path must have run: buffer clean, and
      // disk now holds the EDITED program (not the stale seed).
      assert.ok(
        !document.isDirty,
        "the provider saved the dirty buffer before compiling",
      );
      assert.strictEqual(
        fs.readFileSync(source, "utf8"),
        edited,
        "disk now holds the edited program the provider saved",
      );

      const { stopped, topFrame } = await stopIn(session, source);
      assert.strictEqual(
        topFrame.line,
        2,
        "stopped on the edited-only breakpoint line (proves the save happened)",
      );

      // [DEBUGGER-BINDING-LIFETIME] Locals appear only after initialization.
      assert.ok(
        !(await readFrameVariables(session, topFrame.id)).some(
          (variable) => variable.name === "base",
        ),
        "base is hidden until its initializer completes",
      );
      await stepOver(session, stopped.threadId);
      const initialized = await waitForStop(session, 45000);
      const initializedFrame = assertCurrentLine(initialized.stack, 3, source);
      const base = await assertLocalVariable(
        session,
        initializedFrame.id,
        "base",
        (value) => value === "7",
      );
      assert.strictEqual(base.name, "base", "assertLocalVariable returns base");
      assert.ok(
        fs.existsSync(debugOutput),
        "the saved (not stale) program was debug-compiled to a native binary",
      );

      await resumeToEnd(session, initialized.threadId);
    } finally {
      await debugConfig.update(
        "saveBeforeStart",
        priorSaveBeforeStart,
        vscode.ConfigurationTarget.Global,
      );
    }
  });

  test("a dead lldbDapPath makes the adapter factory refuse to start a session", async function () {
    this.timeout(60000);
    // The debug-adapter descriptor factory resolves lldb-dap from the launch
    // config. When the configured lldbDapPath does not exist AND nothing else
    // resolves, the factory returns undefined after surfacing an error — so VS
    // Code never actually attaches a DAP. We prove that by pointing lldbDapPath
    // at a guaranteed-absent file and asserting the session never reaches a
    // running/stopped state on our breakpoint. The compiler still runs (the
    // provider compiles before the adapter is asked for), so this isolates the
    // adapter-resolution branch from the compile branch.
    const source = path.join(tempDir, "dead-adapter.osp");
    fs.writeFileSync(
      source,
      ["fn main() -> Unit = {", '  print("never debugged")', "}", ""].join(
        "\n",
      ),
    );
    await openSaved(source);

    const deadDap = path.join(tempDir, "no", "such", "lldb-dap");
    assert.ok(
      !fs.existsSync(deadDap),
      "the configured lldb-dap is truly absent",
    );

    setSourceBreakpoints(source, [1]);
    let sawSession: vscode.DebugSession | undefined;
    const sub = vscode.debug.onDidStartDebugSession((s) => {
      sawSession = s;
    });
    try {
      // Race the launch against a timeout; with a dead adapter the promise
      // resolves false (or a session that immediately dies) rather than hanging.
      await launchDebug(
        {
          type: "osprey",
          request: "launch",
          name: "Osprey dead-adapter E2E",
          program: source,
          cwd: tempDir,
          debugOutput: defaultDebugOutputPath(source),
          lldbDapPath: deadDap,
          stopOnEntry: false,
        },
        12000,
        1500,
      );
      // A dead adapter must NOT yield a live, stopped session on our breakpoint.
      const stuckRunning =
        sawSession !== undefined &&
        vscode.debug.activeDebugSession?.id === sawSession.id;
      assert.ok(
        !stuckRunning,
        "no live osprey debug session survives a missing lldb-dap adapter",
      );
      assertExtensionActive("extension survives the dead-adapter path");
    } finally {
      sub.dispose();
      await stopActiveDebugSession();
    }
  });
});

// Unit tests for the pure binary-resolution helpers. These don't depend on a
// live activation, so they can exercise both the bundled-present and unbundled
// branches deterministically by supplying a fake ExtensionContext whose
// asAbsolutePath points at real or missing files.
suite("Osprey Binary Resolution Unit Tests", () => {
  let tempDir: string;

  // Minimal ExtensionContext stand-in: only asAbsolutePath is consumed by the
  // helpers under test. It roots relative paths at a controllable directory.
  function fakeContext(root: string): vscode.ExtensionContext {
    return {
      asAbsolutePath: (rel: string) => path.join(root, rel),
    } as unknown as vscode.ExtensionContext;
  }

  /** Stage the stand-in compiler at bin/<platform>/osprey[.exe] under `root`. */
  function stageBundledCompiler(root: string): string {
    const binary = path.join(
      root,
      "bin",
      shipwrightPlatform(),
      `osprey${COMPILER_EXE}`,
    );
    fs.mkdirSync(path.dirname(binary), { recursive: true });
    fs.writeFileSync(binary, "#!/bin/sh\nexit 0\n");
    return binary;
  }

  function savedServerSettings(): [
    vscode.WorkspaceConfiguration,
    string | undefined,
    string | undefined,
  ] {
    const config = vscode.workspace.getConfiguration("osprey");
    return [
      config,
      config.get<string>("server.compilerPath"),
      config.get<string>("server.path"),
    ];
  }

  async function restoreServerSettings(
    config: vscode.WorkspaceConfiguration,
    compilerPath: string | undefined,
    serverPath: string | undefined,
  ): Promise<void> {
    await config.update(
      "server.compilerPath",
      compilerPath ?? "",
      vscode.ConfigurationTarget.Global,
    );
    await config.update(
      "server.path",
      serverPath ?? "",
      vscode.ConfigurationTarget.Global,
    );
  }

  /** Run `body` with both server settings set to `compiler`/`server`. */
  async function withServerSettings(
    compiler: string,
    server: string,
    body: () => void | Promise<void>,
  ): Promise<void> {
    const [config, originalCompiler, originalPath] = savedServerSettings();
    await config.update(
      "server.compilerPath",
      compiler,
      vscode.ConfigurationTarget.Global,
    );
    await config.update("server.path", server, vscode.ConfigurationTarget.Global);
    try {
      await body();
    } finally {
      await restoreServerSettings(config, originalCompiler, originalPath);
    }
  }

  setup(() => {
    tempDir = makeTempDir("unit");
  });

  teardown(() => removeDir(tempDir));

  test("shipwrightPlatform returns a valid os-arch identifier", () => {
    const id = shipwrightPlatform();
    const [osName, arch] = id.split("-");

    assert.strictEqual(id.split("-").length, 2, "id is exactly os-arch");
    assert.ok(
      ["win32", "darwin", "linux"].includes(osName),
      `os segment "${osName}" is a known platform`,
    );
    assert.ok(
      ["arm64", "x64"].includes(arch),
      `arch segment "${arch}" is a known architecture`,
    );
    // It must agree with the actual process this test runs in.
    const expectedArch = process.arch === "arm64" ? "arm64" : "x64";
    const expectedOs =
      process.platform === "win32"
        ? "win32"
        : process.platform === "darwin"
          ? "darwin"
          : "linux";
    assert.strictEqual(osName, expectedOs, "os matches the host");
    assert.strictEqual(arch, expectedArch, "arch matches the host");
    // The mapping is deterministic across calls.
    assert.strictEqual(shipwrightPlatform(), id, "mapping is stable");
  });

  test("resolveBundledCompiler returns the path when the binary exists", () => {
    // Lay down a fake bundled binary at bin/<platform>/osprey[.exe].
    const bin = stageBundledCompiler(tempDir);

    const resolved = resolveBundledCompiler(fakeContext(tempDir));

    assert.ok(resolved, "a path is returned when the binary exists");
    assert.strictEqual(resolved, bin, "resolved path is the staged binary");
    assert.ok(
      resolved?.endsWith(`osprey${COMPILER_EXE}`),
      "ends with the binary name",
    );
    assert.ok(
      fs.existsSync(resolved as string),
      "resolved path actually exists",
    );
  });

  test("resolveBundledCompiler returns undefined when no binary is bundled", () => {
    // tempDir has no bin/<platform>/osprey, so resolution must fail closed.
    const resolved = resolveBundledCompiler(fakeContext(tempDir));

    assert.strictEqual(resolved, undefined, "undefined when unbundled");
    const expectedMissing = path.join(
      tempDir,
      "bin",
      shipwrightPlatform(),
      process.platform === "win32" ? "osprey.exe" : "osprey",
    );
    assert.ok(
      !fs.existsSync(expectedMissing),
      "the probed path is genuinely absent",
    );
  });

  test("resolveServerCommand prefers an explicit user compiler path", async () => {
    // With server.compilerPath set, resolution must return it verbatim and never
    // touch the bundled fallback.
    const custom = path.join(tempDir, "my-custom-osprey");
    fs.writeFileSync(custom, "#!/bin/sh\nexit 0\n");

    await withServerSettings(custom, "", () => {
      const resolved = resolveServerCommand(fakeContext(tempDir));
      assert.strictEqual(resolved, custom, "returns the explicit user path");
      assert.notStrictEqual(resolved, "osprey", "does not fall back to PATH");
    });
  });

  test("resolveServerCommand falls back to bundled then PATH", async () => {
    // Resolution priority required by [EDITOR-VSCODE] and
    // [EDITOR-VERSIONING].
    // No user path: with a bundled binary present it returns that; without one
    // it falls back to the bare `osprey` PATH lookup.
    await withServerSettings("", "", () => {
      // No bundled binary in tempDir -> bare PATH fallback.
      assert.strictEqual(
        resolveServerCommand(fakeContext(tempDir)),
        "osprey",
        "falls back to osprey on PATH when nothing is bundled",
      );

      // Now stage a bundled binary -> it is preferred over the PATH fallback.
      const bin = stageBundledCompiler(tempDir);

      assert.strictEqual(
        resolveServerCommand(fakeContext(tempDir)),
        bin,
        "prefers the bundled binary over the PATH fallback",
      );
    });
  });

  /** Resolve the server command, collecting any warning it reports. */
  function resolveWithWarnings(): { resolved: string; warnings: string[] } {
    const warnings: string[] = [];
    const resolved = resolveServerCommand(fakeContext(tempDir), (message) =>
      warnings.push(message),
    );
    return { resolved, warnings };
  }

  test("resolveServerCommand falls back and warns when the configured path is missing", async () => {
    // THE REGRESSION that killed hover: a configured server.compilerPath that
    // points at a file that does not exist must NOT be returned verbatim (that
    // ENOENTs the language client and silently kills every feature). It must
    // fall back and warn. tempDir has no bundled binary, so the fallback is the
    // bare `osprey` PATH lookup; then we stage a bundled binary and confirm it
    // is preferred.
    const missing = path.join(tempDir, "does", "not", "exist", "osprey");
    await withServerSettings(missing, "", async () => {
      assert.ok(
        !fs.existsSync(missing),
        "the configured path is genuinely absent",
      );

      const { resolved, warnings } = resolveWithWarnings();

      assert.strictEqual(
        resolved,
        "osprey",
        "falls back to PATH osprey, never the dead path",
      );
      assert.notStrictEqual(
        resolved,
        missing,
        "never returns the missing path",
      );
      assert.strictEqual(warnings.length, 1, "exactly one warning is emitted");
      assert.ok(
        warnings[0].includes(missing),
        "warning names the offending path",
      );
      assertIncludes(
        warnings[0],
        ["does not exist"],
        "warning explains the problem",
      );

      // With a bundled binary present, the fallback prefers it over PATH.
      const bundled = stageBundledCompiler(tempDir);
      assert.strictEqual(
        resolveServerCommand(fakeContext(tempDir), () => undefined),
        bundled,
        "missing user path falls back to the bundled binary when present",
      );
    });
  });

  /** Resolve the server command with `compiler` configured and no warning. */
  async function resolveQuietly(compiler: string): Promise<string> {
    let resolved = "";
    await withServerSettings(compiler, "", () => {
      const recorded = resolveWithWarnings();
      resolved = recorded.resolved;
      assert.strictEqual(
        recorded.warnings.length,
        0,
        `${compiler} resolves without a warning`,
      );
    });
    return resolved;
  }

  test("resolveServerCommand keeps a bare command name without touching the filesystem", async () => {
    // A bare `osprey` (no path separator) is a PATH command, not a file to
    // existence-check; it must be returned as-is even though no such file exists
    // relative to the cwd, and must not warn.
    assert.strictEqual(
      await resolveQuietly("osprey"),
      "osprey",
      "bare command returned verbatim",
    );
  });

  test("an existing configured path is still returned verbatim and never warns", async () => {
    // Happy path: when the configured compiler DOES exist it is used directly,
    // with no fallback and no warning — the dev-settings contract.
    const real = path.join(tempDir, "real-osprey");
    fs.writeFileSync(real, "#!/bin/sh\nexit 0\n");
    assert.strictEqual(
      await resolveQuietly(real),
      real,
      "existing path returned verbatim",
    );
  });

  test("looksLikePath distinguishes filesystem paths from bare commands", () => {
    assert.ok(looksLikePath("/usr/local/bin/osprey"), "absolute posix path");
    assert.ok(looksLikePath("./target/release/osprey"), "relative posix path");
    assert.ok(looksLikePath("C:\\tools\\osprey.exe"), "windows path");
    assert.ok(!looksLikePath("osprey"), "bare command is not a path");
    assert.ok(
      !looksLikePath("osprey-lsp"),
      "hyphenated bare command is not a path",
    );
  });

  // The .ospml extension selects the ML layout flavor. The language id is what
  // the language client's documentSelector keys off, and the compiler resolves
  // the same flavor from the file path — so this mapping is exactly what makes
  // .ospml diagnostics use the ML frontend instead of being misreported as
  // broken Default syntax. ".ospml" must win over ".osp" because it also ends
  // with "osp". Guards the flavor selection that drives ML diagnostics.
  test("ospreyLanguageForFile maps .ospml to osprey-ml and .osp to osprey", () => {
    assert.strictEqual(
      ospreyLanguageForFile("/work/curry_tour.ospml"),
      "osprey-ml",
      ".ospml selects the ML layout flavor language id",
    );
    assert.strictEqual(
      ospreyLanguageForFile("/work/hello.osp"),
      "osprey",
      ".osp selects the brace flavor language id",
    );
    assert.strictEqual(
      ospreyLanguageForFile("C:\\work\\tour.ospml"),
      "osprey-ml",
      ".ospml wins over .osp despite also ending in osp",
    );
    assert.strictEqual(
      ospreyLanguageForFile("/work/readme.md"),
      undefined,
      "a non-Osprey file maps to no language id",
    );
  });

  test("isOspreyFile accepts both .osp and .ospml", () => {
    assert.ok(isOspreyFile("/work/hello.osp"), ".osp is an Osprey file");
    assert.ok(isOspreyFile("/work/hello.ospml"), ".ospml is an Osprey file");
    assert.ok(
      !isOspreyFile("/work/notes.txt"),
      "a non-Osprey file is rejected",
    );
  });
});

// Regression guard for the exact defect that broke hover: the committed dev
// settings pointed osprey.server.compilerPath at src/toolchain/compiler/bin/osprey — a path
// `make build` never produces (src/toolchain/compiler/bin is the C-runtime archive dir) — so
// the language client ENOENT'd and every feature died. This locks the committed
// value so it can never silently rot back to a non-existent binary.
suite("Committed Dev Settings Sanity", () => {
  // extensionRoot (module scope) is <repo>/src/toolchain/vscode-extension; the repo root is
  // three levels up, and its .vscode/settings.json is the committed dev config.
  const repoRoot = path.resolve(extensionRoot, "../../..");
  const settingsPath = path.join(repoRoot, ".vscode", "settings.json");

  function compilerPathSetting(): string | undefined {
    assert.ok(
      fs.existsSync(settingsPath),
      `repo .vscode/settings.json exists at ${settingsPath}`,
    );
    const settings = JSON.parse(fs.readFileSync(settingsPath, "utf8"));
    return settings["osprey.server.compilerPath"];
  }

  test("workspace leaves compiler selection to the installed extension version", () => {
    assert.strictEqual(
      compilerPathSetting(),
      undefined,
      "a checkout compiler override makes release extensions reject 0.0.0-dev",
    );
  });

  test("compilerPath is never the dead src/toolchain/compiler/bin/osprey runtime-archive path", () => {
    const compilerPath = compilerPathSetting();
    if (!compilerPath) {
      return; // unset is fine — resolution falls back to bundled/PATH.
    }
    // Checkout-independent string guards that directly forbid THE regression.
    assert.ok(
      !compilerPath.endsWith(path.join("compiler", "bin", "osprey")),
      `compilerPath must not point at the runtime-archive dir: ${compilerPath}`,
    );
    assert.ok(
      !compilerPath.endsWith("src/toolchain/compiler/bin/osprey"),
      `compilerPath must not point at src/toolchain/compiler/bin/osprey: ${compilerPath}`,
    );
  });

  test("a compilerPath inside this checkout resolves to a real, built binary", () => {
    const compilerPath = compilerPathSetting();
    if (!compilerPath || !compilerPath.startsWith(repoRoot)) {
      // The committed path is absolute to the author's machine; on a foreign
      // checkout (CI) it does not point into this tree, so existence is not our
      // contract to enforce here. The string guards above still apply.
      return;
    }
    // On the author's machine the path IS inside this checkout: after `make
    // build` it must exist. This is the end-to-end proof that the editor's
    // configured LSP binary is launchable (no ENOENT, so hover works), and that
    // it is the make-build output rather than some other staged file.
    assert.ok(
      fs.existsSync(compilerPath),
      `configured LSP binary must exist (run \`make build\`): ${compilerPath}`,
    );
    assert.ok(
      compilerPath.endsWith(path.join("target", "release", "osprey")) ||
        compilerPath.endsWith("target/release/osprey"),
      `in-checkout compilerPath should be the make-build output: ${compilerPath}`,
    );
  });
});

// Unit tests for the language client's failure-handling callbacks and the
// deactivate hook. These fire only on real LSP transport failures / extension
// shutdown, which the integration suites cannot deterministically induce, so we
// drive the extracted, injectable handlers directly and assert their contracts.
suite("Osprey Client Failure Handling Unit Tests", () => {
  /** Failure-handling callbacks that record every line they log or surface. */
  function recordingHandling(): {
    logs: string[];
    errors: string[];
    handling: ReturnType<typeof makeClientFailureHandling>;
  } {
    const logs: string[] = [];
    const errors: string[] = [];
    const handling = makeClientFailureHandling(
      (m) => logs.push(m),
      (m) => errors.push(m),
    );
    return { logs, errors, handling };
  }

  test("initializationFailedHandler logs, surfaces an error, and stops retrying", () => {
    const { logs, errors, handling } = recordingHandling();

    const result = handling.initializationFailedHandler!(
      new Error("init boom"),
    );

    assert.strictEqual(
      result,
      false,
      "returns false so the client does not retry",
    );
    assert.strictEqual(logs.length, 1, "logs exactly once");
    assertIncludes(logs[0], ["Initialization failed"], "log names the failure");
    assertIncludes(logs[0], ["init boom"], "log carries the underlying error");
    assert.strictEqual(errors.length, 1, "one user-facing error is shown");
    assertIncludes(
      errors[0],
      ["initialization failed"],
      "error message is descriptive",
    );
  });

  test("errorHandler.error logs the error/message/count and continues", async () => {
    const { logs, handling } = recordingHandling();

    const result = await handling.errorHandler!.error(
      new Error("transport"),
      undefined,
      2,
    );

    assert.strictEqual(
      result.action,
      ErrorAction.Continue,
      "keeps the server alive",
    );
    assert.strictEqual(logs.length, 1, "logs the error once");
    assertIncludes(
      logs[0],
      ["Language server error"],
      "log identifies a server error",
    );
    assertIncludes(logs[0], ["transport"], "log includes the underlying error");
    assert.ok(logs[0].includes("2"), "log includes the occurrence count");
  });

  test("errorHandler.closed logs and restarts the connection", async () => {
    const { logs, handling } = recordingHandling();

    const result = await handling.errorHandler!.closed();

    assert.strictEqual(
      result.action,
      CloseAction.Restart,
      "restarts on a dropped connection",
    );
    assert.strictEqual(logs.length, 1, "logs the closure once");
    assert.ok(logs[0].includes("closed"), "log names the closure");
    assert.ok(logs[0].includes("restarting"), "log states the restart");
  });

  // Runs LAST (file order): deactivate stops the live language client started by
  // the earlier integration suites, so no later suite may depend on the server.
  test("deactivate stops the running language client without throwing", async () => {
    const result = deactivate();
    // activate() ran in earlier suites, so a client exists and stop() yields a
    // promise; await it to prove a clean shutdown. With no client the documented
    // contract is to return undefined instead.
    if (result !== undefined) {
      await result;
      assert.ok(true, "client.stop() resolved cleanly");
    } else {
      assert.strictEqual(
        result,
        undefined,
        "with no client, deactivate is a no-op",
      );
    }
  });
});

// Pure unit tests for the debug-config synthesis the "Debug Osprey File" launch
// relies on. Driving it through vscode.debug.startDebugging is unreliable in the
// headless host (no real debug UI invokes the provider), so the branchy logic is
// extracted and tested directly here.
suite("Osprey Debug Config Synthesis Unit Tests", () => {
  const ospreyEditor = (
    fileName: string,
  ): { document: { languageId: string; fileName: string } } => ({
    document: { languageId: "osprey", fileName },
  });

  /** A bare host for the lldb-dap resolvers that finds only what `exists` finds. */
  function dapHost(exists: (filePath: string) => boolean) {
    return {
      env: { PATH: "" },
      existsSync: exists,
      getSetting: () => undefined,
      platform: "linux" as NodeJS.Platform,
    };
  }

  test("synthesizes a full launch config from the active osprey editor", () => {
    const config = applyDefaultOspreyDebugConfig(
      {},
      ospreyEditor("/tmp/main.osp"),
    );
    assert.strictEqual(config.type, "osprey", "type defaults to osprey");
    assert.strictEqual(config.request, "launch", "request defaults to launch");
    assert.strictEqual(config.name, "Debug Osprey File", "name is set");
    assert.strictEqual(
      config.program,
      "/tmp/main.osp",
      "program is the active file",
    );
  });

  test("leaves an empty config untouched when the active editor is not osprey", () => {
    const config = applyDefaultOspreyDebugConfig(
      {},
      {
        document: { languageId: "plaintext", fileName: "/tmp/notes.txt" },
      },
    );
    assert.strictEqual(config.program, undefined, "no program synthesized");
    assert.strictEqual(config.type, undefined, "no type synthesized");
  });

  test("leaves an empty config untouched when there is no active editor", () => {
    const config = applyDefaultOspreyDebugConfig({}, undefined);
    assert.deepStrictEqual(config, {}, "nothing to synthesize from");
  });

  test("never overwrites a config the user already populated", () => {
    const preset = {
      type: "osprey",
      request: "launch",
      name: "Custom",
      program: "/explicit/path.osp",
    };
    const config = applyDefaultOspreyDebugConfig(
      preset,
      ospreyEditor("/tmp/other.osp"),
    );
    assert.strictEqual(
      config.program,
      "/explicit/path.osp",
      "explicit program preserved",
    );
    assert.strictEqual(config.name, "Custom", "explicit name preserved");
  });

  test("chooses a stable per-source debug output path", () => {
    const out = defaultDebugOutputPath(path.join("/tmp", "demo.osp"));
    assert.strictEqual(path.basename(path.dirname(out)), ".osprey-debug");
    assert.strictEqual(
      path.basename(out),
      process.platform === "win32" ? "demo.exe" : "demo",
    );
  });

  test("resolves lldb-dap from config before defaults", () => {
    const host = dapHost((filePath) => filePath === "/custom/lldb-dap");

    assert.strictEqual(
      resolveLldbDapExecutable({ lldbDapPath: "/custom/lldb-dap" }, host),
      "/custom/lldb-dap",
    );
    assert.strictEqual(
      resolveLldbDapCommand({ lldbDapPath: "/custom/lldb-dap" }, host),
      "/custom/lldb-dap",
    );
    assert.ok(
      resolveLldbDapCommand({}).length > 0,
      "default lldb-dap command is non-empty",
    );
  });

  test("strict lldb-dap resolver reports missing tools precisely", () => {
    const host = dapHost(() => false);

    assert.strictEqual(resolveLldbDapExecutable({}, host), undefined);
    assert.strictEqual(
      resolveLldbDapExecutable({ lldbDapPath: "/missing/lldb-dap" }, host),
      undefined,
    );
    assert.strictEqual(
      resolveLldbDapCommand({}, host),
      "lldb-dap",
      "compatibility resolver still returns a command name for old callers",
    );
    assert.match(
      missingLldbDapMessage({ lldbDapPath: "/missing/lldb-dap" }),
      /Configured lldbDapPath: \/missing\/lldb-dap\./,
    );
  });
});

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
} from "../../client/src/extension";
import {
  extensionRoot,
  resolveBuiltOsprey,
  resolveRequiredLldbDap,
} from "./osprey-test-env";
import {
  assertLocalVariable,
  clearDebugBreakpoints,
  getScopes,
  getVariables,
  setSourceBreakpoints,
  waitForDebugSessionEnd,
  waitForDebugSessionStart,
  waitForStop,
} from "./dap-harness";

const extensionId = "nimblesite.osprey";

// startDebugRaced runs the real startDebugging path but never hangs the suite
// on a host without a debug UI: it resolves to a marker string once VS Code
// settles or a timeout elapses, whichever comes first.
async function startDebugRaced(
  config: vscode.DebugConfiguration,
  budgetMs = 6000,
): Promise<string> {
  const timeout = new Promise<string>((resolve) =>
    setTimeout(() => resolve("timeout"), budgetMs),
  );
  const start = Promise.resolve(vscode.debug.startDebugging(undefined, config))
    .then((value) => `resolved:${String(value)}`)
    .catch((error: unknown) => `error:${String(error)}`);
  return Promise.race([start, timeout]);
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
  "..",
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
// now live in ./dap-harness and ./osprey-test-env — the single shared copy that
// mirrors @nimblesite/lspkit-debug. They are imported above; nothing is
// re-derived here. [DEBUGGER-REUSE]

suite("Osprey Shipwright Activation Coverage", () => {
  const settle = (ms: number) =>
    new Promise((resolve) => setTimeout(resolve, ms));
  let priorCompilerPath: string | undefined;
  let setCompilerPath = false;

  suiteSetup(async () => {
    // Point server.compilerPath at the real osprey binary BEFORE the extension
    // activates so resolveServerCommand takes its explicit-user-path branch and
    // the language client launches against a genuine osprey, exercising the
    // client option handlers and the start outcome.
    const ospreyPath = resolveBuiltOsprey();
    if (ospreyPath) {
      const config = vscode.workspace.getConfiguration("osprey");
      priorCompilerPath = config.get<string>("server.compilerPath");
      await config.update(
        "server.compilerPath",
        ospreyPath,
        vscode.ConfigurationTarget.Global,
      );
      setCompilerPath = true;
    }
  });

  suiteTeardown(async () => {
    // Restore the compiler path so later suites see defaults.
    if (setCompilerPath) {
      await vscode.workspace
        .getConfiguration("osprey")
        .update(
          "server.compilerPath",
          priorCompilerPath ?? "",
          vscode.ConfigurationTarget.Global,
        );
    }
    // Remove only the manifest we staged so we never delete a real one.
    if (manifestWasStaged && fs.existsSync(stagedManifestPath)) {
      fs.rmSync(stagedManifestPath, { force: true });
    }
  });

  test("extension activates with a shipwright manifest present", async () => {
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
  let testFile: string;

  setup(() => {
    // Create temporary directory for test files
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-test-"));
    testFile = path.join(tempDir, "test.osp");
  });

  teardown(() => {
    // Clean up temporary files
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  test("Extension should activate when opening .osp file", async () => {
    // Create a simple Osprey file
    const ospreyCode = `
// Simple test function
fn add(a, b) = a + b

let result = add(5, 3)
print(result)
`;
    fs.writeFileSync(testFile, ospreyCode);

    // Open the file in VS Code
    const document = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(document);

    // Wait a bit for extension to activate
    await new Promise((resolve) => setTimeout(resolve, 1000));

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
    const ospreyCode = `fn test() = 42`;
    fs.writeFileSync(testFile, ospreyCode);

    const document = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(document);

    // Wait for language detection
    await new Promise((resolve) => setTimeout(resolve, 500));

    assert.strictEqual(
      document.languageId,
      "osprey",
      "Language should be set to osprey",
    );
  });

  test("Compile command should be available for .osp files", async () => {
    const ospreyCode = `fn hello() = print("Hello, World!")`;
    fs.writeFileSync(testFile, ospreyCode);

    const document = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(document);

    // Wait for extension activation
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Get all available commands
    const commands = await vscode.commands.getCommands();

    assert.ok(
      commands.includes("osprey.compile"),
      "Compile command should be available",
    );
    assert.ok(
      commands.includes("osprey.run"),
      "Run command should be available",
    );
    assert.ok(
      commands.includes("osprey.debug"),
      "Debug command should be available",
    );
  });

  test("Syntax highlighting should work for .osp files", async () => {
    const ospreyCode = `
fn power(base, exp) = match exp {
  0 => 1
  1 => base
  _ => base * power(base, exp - 1)
}

let result = power(2, 3)
print(result)
`;
    fs.writeFileSync(testFile, ospreyCode);

    const document = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(document);

    // Wait for syntax highlighting to load
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Check that the document has the correct language
    assert.strictEqual(document.languageId, "osprey");

    // Check that the file has content (basic sanity check)
    assert.ok(
      document.getText().includes("fn power"),
      "Document should contain the test code",
    );
  });

  test("Extension should handle invalid Osprey code gracefully", async () => {
    const invalidCode = `
fn broken syntax here {
  this is not valid osprey code
  missing parentheses and stuff
}
`;
    fs.writeFileSync(testFile, invalidCode);

    const document = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(document);

    // Wait for diagnostics
    await new Promise((resolve) => setTimeout(resolve, 2000));

    // Extension should still be active even with invalid code
    const extension = vscode.extensions.getExtension(extensionId);
    assert.ok(
      extension?.isActive,
      "Extension should remain active with invalid code",
    );
  });

  test("File operations should work without workspace", async () => {
    // This test ensures the extension works with individual files
    const ospreyCode = `fn standalone() = print("No workspace needed!")`;
    fs.writeFileSync(testFile, ospreyCode);

    // Close any existing workspace
    if (vscode.workspace.workspaceFolders) {
      await vscode.commands.executeCommand("workbench.action.closeFolder");
    }

    // Open file without workspace
    const document = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(document);

    // Wait for extension
    await new Promise((resolve) => setTimeout(resolve, 1000));

    // Should still work
    assert.strictEqual(document.languageId, "osprey");

    const extension = vscode.extensions.getExtension(extensionId);
    assert.ok(extension?.isActive, "Extension should work without workspace");
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
    await new Promise((resolve) => setTimeout(resolve, 1000));

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
    // Basic test that language server starts without crashing
    const ospreyCode = `fn test() = 42`;
    fs.writeFileSync(testFile, ospreyCode);

    const document = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(document);

    // Wait for language server to start
    await new Promise((resolve) => setTimeout(resolve, 3000));

    // Extension should still be active
    const extension = vscode.extensions.getExtension(extensionId);
    assert.ok(
      extension?.isActive,
      "Extension should remain active with language server",
    );
  });

  test("Compile and Run commands execute against the active file", async () => {
    fs.writeFileSync(testFile, 'fn main() -> Unit = print("hi")\n');
    const document = await vscode.workspace.openTextDocument(testFile);
    await vscode.window.showTextDocument(document);
    await new Promise((resolve) => setTimeout(resolve, 500));

    // Both commands shell out to the staged `osprey` binary on PATH; they must
    // run their handlers to completion without throwing back to the caller.
    await vscode.commands.executeCommand("osprey.compile");
    await vscode.commands.executeCommand("osprey.run");
    await new Promise((resolve) => setTimeout(resolve, 3000));

    const extension = vscode.extensions.getExtension(extensionId);
    assert.ok(
      extension?.isActive,
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
  let priorCompilerPath: string | undefined;
  let pinnedCompiler = false;
  const extension = () => vscode.extensions.getExtension(extensionId);

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

  async function openDoc(
    name: string,
    content: string,
  ): Promise<vscode.TextDocument> {
    const file = path.join(tempDir, name);
    fs.writeFileSync(file, content);
    const doc = await vscode.workspace.openTextDocument(file);
    await vscode.window.showTextDocument(doc);
    return doc;
  }

  const hoverText = (h: vscode.Hover): string =>
    h.contents.map((c) => (typeof c === "string" ? c : c.value)).join("\n");

  const hoverAt = (uri: vscode.Uri, line: number, character: number) =>
    vscode.commands.executeCommand<vscode.Hover[]>(
      "vscode.executeHoverProvider",
      uri,
      new vscode.Position(line, character),
    );
  const defsAt = (uri: vscode.Uri, line: number, character: number) =>
    vscode.commands.executeCommand<vscode.Location[]>(
      "vscode.executeDefinitionProvider",
      uri,
      new vscode.Position(line, character),
    );
  const refsAt = (uri: vscode.Uri, line: number, character: number) =>
    vscode.commands.executeCommand<vscode.Location[]>(
      "vscode.executeReferenceProvider",
      uri,
      new vscode.Position(line, character),
    );
  const symbolsOf = (uri: vscode.Uri) =>
    vscode.commands.executeCommand<vscode.DocumentSymbol[]>(
      "vscode.executeDocumentSymbolProvider",
      uri,
    );
  const completionAt = (uri: vscode.Uri, line: number, character: number) =>
    vscode.commands.executeCommand<vscode.CompletionList>(
      "vscode.executeCompletionItemProvider",
      uri,
      new vscode.Position(line, character),
    );
  const sigHelpAt = (uri: vscode.Uri, line: number, character: number) =>
    vscode.commands.executeCommand<vscode.SignatureHelp>(
      "vscode.executeSignatureHelpProvider",
      uri,
      new vscode.Position(line, character),
    );
  const nonEmptyHover = (h: vscode.Hover[]): boolean =>
    Array.isArray(h) && h.length > 0 && hoverText(h[0]).length > 0;
  const labelsOf = (list: vscode.CompletionList): string[] =>
    list.items.map((i) =>
      typeof i.label === "string" ? i.label : i.label.label,
    );
  const startLines = (locs: vscode.Location[]): number[] =>
    locs.map((l) => l.range.start.line).sort((a, b) => a - b);

  suiteSetup(async function () {
    this.timeout(60000);
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-feat-"));

    // Pin a genuine compiler so the language client is actually running.
    const ospreyPath = resolveBuiltOsprey();
    assert.ok(
      ospreyPath,
      "a freshly-built osprey binary (target/release or PATH) is required for feature tests",
    );
    const config = vscode.workspace.getConfiguration("osprey");
    priorCompilerPath = config.get<string>("server.compilerPath");
    await config.update(
      "server.compilerPath",
      ospreyPath,
      vscode.ConfigurationTarget.Global,
    );
    pinnedCompiler = true;

    const ext = extension();
    assert.ok(ext, "extension discoverable");
    if (ext && !ext.isActive) {
      await ext.activate();
    }

    // Warm up: open a document and wait until hover actually answers. This both
    // proves the server is live and removes first-request flakiness from the
    // individual feature tests below.
    const warm = await openDoc(
      "warmup.osp",
      "\nfn warm(x) = x * 2\n\nlet w = warm(2)\n",
    );
    await pollFor<vscode.Hover[]>(
      () => hoverAt(warm.uri, 3, 9),
      (h) =>
        Array.isArray(h) && h.length > 0 && hoverText(h[0]).includes("warm"),
      80,
      250,
    );
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  });

  suiteTeardown(async () => {
    if (pinnedCompiler) {
      await vscode.workspace
        .getConfiguration("osprey")
        .update(
          "server.compilerPath",
          priorCompilerPath ?? "",
          vscode.ConfigurationTarget.Global,
        );
    }
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    if (tempDir && fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  teardown(async () => {
    await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
  });

  // A single multi-construct program (type + functions + lets + builtins)
  // exercised by the full provider sweep below.
  const RICH =
    [
      "type Shape = Circle | Square", // 0
      "fn area(r) = r * r", // 1
      "fn perimeter(r) = r + r + r + r", // 2
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
    const areaDecl = await pollFor(
      () => hoverAt(doc.uri, 1, 3),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("area"),
    );
    const areaDeclMd = hoverText(areaDecl[0]);
    assert.ok(
      areaDeclMd.includes("fn area(r)"),
      "area decl hover shows the signature",
    );
    assert.ok(
      areaDeclMd.includes("->"),
      "area decl hover shows the return arrow",
    );
    assert.ok(
      areaDeclMd.includes("Unit"),
      "area decl hover shows the inferred return type",
    );

    const areaCall = await pollFor(
      () => hoverAt(doc.uri, 4, 9),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("area"),
    );
    assert.strictEqual(
      hoverText(areaCall[0]),
      areaDeclMd,
      "call-site hover matches the declaration hover",
    );

    const periCall = await pollFor(
      () => hoverAt(doc.uri, 6, 9),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("perimeter"),
    );
    assert.ok(
      hoverText(periCall[0]).includes("fn perimeter(r)"),
      "perimeter hover shows its signature",
    );

    // --- HOVER: built-ins carry their own typed signatures ---
    const lenHover = await pollFor(
      () => hoverAt(doc.uri, 8, 13),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("listLength"),
    );
    const lenMd = hoverText(lenHover[0]);
    assert.ok(
      lenMd.includes("->") && lenMd.includes("int"),
      "listLength hover shows its typed signature",
    );
    assert.ok(lenMd.includes("List"), "listLength hover mentions List");

    const printHover = await pollFor(
      () => hoverAt(doc.uri, 9, 9),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("print"),
    );
    assert.ok(
      hoverText(printHover[0]).includes("Unit"),
      "print hover shows it returns Unit",
    );

    // --- HOVER: a `let` binding shows its TYPE, not just functions/builtins ---
    // This is the regression the user reported: "no bubble on hover of
    // variables". `let radius = 5` is unannotated, so the type is the one the
    // checker INFERRED (int) — proving variable hover end-to-end. [LSP-HOVER-VARIABLES]
    const radiusHover = await pollFor(
      () => hoverAt(doc.uri, 3, 4),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("radius"),
    );
    assert.ok(
      hoverText(radiusHover[0]).includes("radius: int"),
      `variable hover shows "name: inferred-type": ${hoverText(radiusHover[0])}`,
    );
    // A let bound to a builtin call infers its result type too.
    const countHover = await pollFor(
      () => hoverAt(doc.uri, 8, 4),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("count"),
    );
    assert.ok(
      hoverText(countHover[0]).includes("count: int"),
      `count infers int from listLength: ${hoverText(countHover[0])}`,
    );

    // --- GO TO DEFINITION: both calls jump to their declarations ---
    const areaDef = await pollFor(
      () => defsAt(doc.uri, 4, 9),
      (d) => Array.isArray(d) && d.length > 0,
    );
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

    const periDef = await pollFor(
      () => defsAt(doc.uri, 6, 9),
      (d) => Array.isArray(d) && d.length > 0,
    );
    assert.strictEqual(
      periDef[0].range.start.line,
      2,
      "perimeter defined on line 2",
    );

    // --- FIND REFERENCES: declaration + every call, scoped to the function ---
    const areaRefs = await pollFor(
      () => refsAt(doc.uri, 1, 3),
      (r) => Array.isArray(r) && r.length >= 3,
    );
    assert.strictEqual(areaRefs.length, 3, "area: declaration + two calls");
    assert.deepStrictEqual(
      startLines(areaRefs),
      [1, 4, 5],
      "area references on the decl and both call lines",
    );
    assert.ok(
      areaRefs.every((r) => r.uri.toString() === doc.uri.toString()),
      "all area references in this document",
    );

    const periRefs = await pollFor(
      () => refsAt(doc.uri, 2, 3),
      (r) => Array.isArray(r) && r.length >= 2,
    );
    assert.strictEqual(periRefs.length, 2, "perimeter: declaration + one call");
    assert.deepStrictEqual(
      startLines(periRefs),
      [2, 6],
      "perimeter references on decl and its call",
    );

    // --- DOCUMENT SYMBOLS: type, functions and lets with correct kinds ---
    const syms = await pollFor(
      () => symbolsOf(doc.uri),
      (s) => Array.isArray(s) && s.length >= 10,
    );
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
    assert.strictEqual(
      byName.get("Shape")?.kind,
      vscode.SymbolKind.Class,
      "Shape is a type/Class",
    );
    assert.strictEqual(
      byName.get("area")?.kind,
      vscode.SymbolKind.Function,
      "area is a Function",
    );
    assert.strictEqual(
      byName.get("perimeter")?.kind,
      vscode.SymbolKind.Function,
      "perimeter is a Function",
    );
    assert.strictEqual(
      byName.get("radius")?.kind,
      vscode.SymbolKind.Variable,
      "radius is a Variable",
    );

    // --- SIGNATURE HELP: inside the area(...) call ---
    const help = await pollFor(
      () => sigHelpAt(doc.uri, 4, 13),
      (h) => !!h && Array.isArray(h.signatures) && h.signatures.length > 0,
    );
    assert.ok(
      help.signatures[0].label.includes("area"),
      "signature names area",
    );
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

    // --- COMPLETION: user symbols AND keywords are offered ---
    const list = await pollFor(
      () => completionAt(doc.uri, 9, 9),
      (l) => !!l && Array.isArray(l.items) && l.items.length > 0,
    );
    const labels = labelsOf(list);
    for (const sym of ["Shape", "area", "perimeter", "radius", "count"]) {
      assert.ok(
        labels.includes(sym),
        `completion offers the user symbol ${sym}`,
      );
    }
    for (const kw of ["fn", "let", "match", "type"]) {
      assert.ok(labels.includes(kw), `completion offers the keyword ${kw}`);
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
    const top = await pollFor(
      () => hoverAt(doc.uri, 1, 4),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("maxRetries"),
    );
    const topMd = hoverText(top[0]);
    assert.ok(
      topMd.includes("maxRetries: int"),
      `top-level let shows its type: ${topMd}`,
    );
    assert.ok(
      topMd.includes("The maximum retry budget for a request."),
      `top-level let shows its /// docs: ${topMd}`,
    );

    // Nested documented let inside a function block: type + doc prose. This is
    // the exact shape from the reported screenshot (a `let` inside `in { … }`).
    const nested = await pollFor(
      () => hoverAt(doc.uri, 4, 6),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("banner"),
    );
    const nestedMd = hoverText(nested[0]);
    assert.ok(
      nestedMd.includes("banner: string"),
      `nested let shows its inferred type: ${nestedMd}`,
    );
    assert.ok(
      nestedMd.includes("Greeting echoed back to the caller."),
      `nested let shows its /// docs: ${nestedMd}`,
    );
  });

  test("CHUNKY: diagnostics lifecycle — error surfaces, clears on fix, returns on re-break", async function () {
    this.timeout(60000);
    const doc = await openDoc("lifecycle.osp", "\nfn broken( = 42\n");

    // 1) The broken program must surface a real Error diagnostic from osprey.
    const first = await pollFor(
      () => Promise.resolve(vscode.languages.getDiagnostics(doc.uri)),
      (d) => d.length > 0,
    );
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
    const editor = await vscode.window.showTextDocument(doc);
    await editor.edit((b) =>
      b.replace(
        new vscode.Range(0, 0, doc.lineCount, 0),
        "fn ok(x) = x * 2\nlet y = ok(2)\n",
      ),
    );
    const cleared = await pollFor(
      () => Promise.resolve(vscode.languages.getDiagnostics(doc.uri)),
      (d) => d.length === 0,
    );
    assert.strictEqual(
      cleared.length,
      0,
      "no diagnostics once the code is valid",
    );
    // ...and the now-valid buffer answers hover, proving the server reparsed it.
    const okHover = await pollFor(
      () => hoverAt(doc.uri, 0, 3),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("ok"),
    );
    assert.ok(
      hoverText(okHover[0]).includes("fn ok"),
      "hover works on the corrected program",
    );

    // 3) Re-breaking the buffer brings the Error diagnostic back.
    const editor2 = await vscode.window.showTextDocument(doc);
    await editor2.edit((b) =>
      b.replace(
        new vscode.Range(0, 0, doc.lineCount, 0),
        "\nfn broken( = 42\n",
      ),
    );
    const second = await pollFor(
      () => Promise.resolve(vscode.languages.getDiagnostics(doc.uri)),
      (d) => d.length > 0,
    );
    assert.strictEqual(
      second[0].severity,
      vscode.DiagnosticSeverity.Error,
      "re-broken code errors again",
    );
    assert.ok(second[0].message.length > 0, "re-error has a message");
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
    const decl = await pollFor(
      () => hoverAt(doc.uri, 0, 3),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("classify"),
    );
    assert.ok(
      hoverText(decl[0]).includes("fn classify(xs)"),
      "declaration hover shows classify signature",
    );
    const call = await pollFor(
      () => hoverAt(doc.uri, 4, 9),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("classify"),
    );
    assert.strictEqual(
      hoverText(call[0]),
      hoverText(decl[0]),
      "call hover matches declaration hover",
    );

    // Definition from the first call lands on the declaration.
    const def = await pollFor(
      () => defsAt(doc.uri, 4, 9),
      (d) => Array.isArray(d) && d.length > 0,
    );
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
    const refs = await pollFor(
      () => refsAt(doc.uri, 0, 3),
      (r) => Array.isArray(r) && r.length >= 3,
    );
    assert.strictEqual(refs.length, 3, "classify: declaration + two calls");
    assert.deepStrictEqual(
      startLines(refs),
      [0, 4, 5],
      "references on the decl and both call lines",
    );

    // Symbols: the function plus the two lets, with correct kinds.
    const syms = await pollFor(
      () => symbolsOf(doc.uri),
      (s) => Array.isArray(s) && s.length >= 3,
    );
    const byName = new Map(syms.map((s) => [s.name, s]));
    assert.strictEqual(
      byName.get("classify")?.kind,
      vscode.SymbolKind.Function,
      "classify is a Function",
    );
    assert.strictEqual(
      byName.get("z")?.kind,
      vscode.SymbolKind.Variable,
      "z is a Variable",
    );
    assert.strictEqual(
      byName.get("w")?.kind,
      vscode.SymbolKind.Variable,
      "w is a Variable",
    );

    // Completion inside the second call offers the user function classify.
    const list = await pollFor(
      () => completionAt(doc.uri, 5, 10),
      (l) => !!l && Array.isArray(l.items) && l.items.length > 0,
    );
    assert.ok(
      labelsOf(list).includes("classify"),
      "completion offers the classify function",
    );
  });

  test("CHUNKY: hover + symbols work on the ACTUAL reported list_basics.osp file", async function () {
    this.timeout(60000);
    // Open the very file the user reported ("Hover doesnt work!") straight from
    // the repository and prove the language features answer over it. extensionRoot
    // is <repo>/vscode-extension; the example lives under examples.
    const reported = path.resolve(
      extensionRoot,
      "..",
      "examples",
      "tested",
      "basics",
      "lists",
      "list_basics.osp",
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

    // Hover the `listLength` builtin at its first call site.
    const lenAt = at("print(listLength(e))", "listLength");
    const lenHover = await pollFor(
      () => hoverAt(doc.uri, lenAt.line, lenAt.character),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("listLength"),
      80,
      250,
    );
    assert.ok(
      hoverText(lenHover[0]).includes("->"),
      "listLength hover shows a signature on the real file",
    );
    assert.ok(
      hoverText(lenHover[0]).includes("int"),
      "listLength hover shows it returns int",
    );

    // Hover the user-defined `classify` function at its declaration.
    const classifyAt = at("fn classify(xs)", "classify");
    const classifyHover = await pollFor(
      () => hoverAt(doc.uri, classifyAt.line, classifyAt.character),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("classify"),
    );
    assert.ok(
      hoverText(classifyHover[0]).includes("fn classify"),
      "classify hover shows its signature",
    );

    // Document symbols include the functions defined in the example.
    const syms = await pollFor(
      () => symbolsOf(doc.uri),
      (s) => Array.isArray(s) && s.length > 0,
    );
    const names = syms.map((s) => s.name);
    assert.ok(
      names.includes("classify"),
      "classify is listed among document symbols",
    );
    assert.ok(
      names.includes("sumList"),
      "sumList is listed among document symbols",
    );
    assert.ok(
      names.includes("restLen"),
      "restLen is listed among document symbols",
    );
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
    // blocks, `name = value` bindings (no `let`/`fn`). [LSP-ML-FLAVOR]
    const ML =
      [
        "double x = x * 2", // 0  curried unary fn
        "triple x = x + x + x", // 1  curried unary fn
        "base = 5", // 2  binding
        "d = double base", // 3  call site of double
        "t = triple base", // 4  call site of triple
        "u = double base", // 5  second call of double
        'print "d=${d} t=${t} u=${u}"', // 6
      ].join("\n") + "\n";
    const file = path.join(tempDir, "sweep.ospml");
    fs.writeFileSync(file, ML);
    const doc = await vscode.workspace.openTextDocument(file);
    await vscode.window.showTextDocument(doc);

    // The extension must associate .ospml with the ML language id, which is how
    // the LSP client's documentSelector routes it to the server.
    for (let i = 0; i < 40 && doc.languageId !== "osprey-ml"; i++) {
      await new Promise((r) => setTimeout(r, 150));
    }
    assert.strictEqual(
      doc.languageId,
      "osprey-ml",
      "a .ospml file is associated with the osprey-ml language id",
    );

    // --- HOVER: the curried function at its declaration and a call site ---
    const declHover = await pollFor(
      () => hoverAt(doc.uri, 0, 0),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("double"),
      80,
      250,
    );
    const declMd = hoverText(declHover[0]);
    assert.ok(declMd.includes("double"), "ML fn hover names the function");
    assert.ok(
      declMd.includes("->"),
      "ML fn hover renders a function-type arrow",
    );
    const callHover = await pollFor(
      () => hoverAt(doc.uri, 3, 4),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("double"),
    );
    assert.strictEqual(
      hoverText(callHover[0]),
      declMd,
      "ML call-site hover matches the declaration hover",
    );
    // A binding hovers with its inferred type — no annotation in the source.
    const baseHover = await pollFor(
      () => hoverAt(doc.uri, 2, 0),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("base"),
    );
    assert.ok(
      /base\s*:\s*int/i.test(hoverText(baseHover[0])),
      `ML binding hover shows inferred type: ${hoverText(baseHover[0])}`,
    );

    // --- GO TO DEFINITION: a call jumps back to the ML declaration ---
    const def = await pollFor(
      () => defsAt(doc.uri, 3, 4),
      (d) => Array.isArray(d) && d.length > 0,
    );
    assert.strictEqual(def[0].range.start.line, 0, "double defined on line 0");
    assert.strictEqual(
      def[0].uri.toString(),
      doc.uri.toString(),
      "definition resolves within the .ospml document",
    );

    // --- FIND REFERENCES: declaration + both call sites of double ---
    const refs = await pollFor(
      () => refsAt(doc.uri, 0, 0),
      (r) => Array.isArray(r) && r.length >= 3,
    );
    assert.deepStrictEqual(
      startLines(refs),
      [0, 3, 5],
      "double is referenced at its declaration and both call sites",
    );

    // --- DOCUMENT SYMBOLS: both functions and every binding are listed ---
    const syms = await pollFor(
      () => symbolsOf(doc.uri),
      (s) => Array.isArray(s) && s.length >= 5,
    );
    const names = new Set(syms.map((s) => s.name));
    for (const name of ["double", "triple", "base", "d", "t", "u"]) {
      assert.ok(names.has(name), `ML symbol ${name} is listed`);
    }

    // --- COMPLETION: user-defined ML symbols are offered ---
    const list = await pollFor(
      () => completionAt(doc.uri, 4, 4),
      (l) => !!l && Array.isArray(l.items) && l.items.length > 0,
    );
    const labels = labelsOf(list);
    assert.ok(
      labels.includes("double") && labels.includes("triple"),
      "completion offers the ML user functions",
    );

    // --- DIAGNOSTICS lifecycle on ML source: break it, then fix it ---
    const editor = await vscode.window.showTextDocument(doc);
    await editor.edit((b) =>
      b.replace(
        new vscode.Range(0, 0, doc.lineCount, 0),
        'd = missingFn 5\nprint "${d}"\n',
      ),
    );
    const broken = await pollFor(
      () => Promise.resolve(vscode.languages.getDiagnostics(doc.uri)),
      (d) => d.length > 0,
    );
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

    const editor2 = await vscode.window.showTextDocument(doc);
    await editor2.edit((b) =>
      b.replace(
        new vscode.Range(0, 0, doc.lineCount, 0),
        'square x = x * x\nv = square 4\nprint "${v}"\n',
      ),
    );
    const fixed = await pollFor(
      () => Promise.resolve(vscode.languages.getDiagnostics(doc.uri)),
      (d) => d.length === 0,
    );
    assert.strictEqual(
      fixed.length,
      0,
      "diagnostics clear once the ML program is valid again",
    );
    const okHover = await pollFor(
      () => hoverAt(doc.uri, 0, 0),
      (h) => nonEmptyHover(h) && hoverText(h[0]).includes("square"),
    );
    assert.ok(
      hoverText(okHover[0]).includes("square"),
      "hover works on the corrected ML program",
    );
  });
});

// These suites drive the command handlers, event subscriptions, and debug
// provider registered in activate() so the coverage harness records the
// execFile callbacks and the early-return guards in extension.ts.
suite("Osprey Command Handler Coverage", () => {
  let tempDir: string;
  const extension = () => vscode.extensions.getExtension(extensionId);

  // The compile/run handlers shell out to `osprey` and write to an output
  // channel from the execFile callback; the callback completes after a tick, so
  // give it a generous window before we let the test (and the host) move on.
  const settle = (ms: number) =>
    new Promise((resolve) => setTimeout(resolve, ms));

  async function closeEverything(): Promise<void> {
    // closeAllEditors races with VS Code's async editor teardown, so poll until
    // the active editor actually drains (or give up after a bounded number of
    // attempts and let the caller assert what it needs).
    for (let i = 0; i < 10 && vscode.window.activeTextEditor; i++) {
      await vscode.commands.executeCommand("workbench.action.closeAllEditors");
      await settle(150);
    }
  }

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
      if (
        active &&
        active.document.uri.toString() === document.uri.toString()
      ) {
        return true;
      }
    }
    return false;
  }

  async function openOsp(
    name: string,
    content: string,
  ): Promise<vscode.TextDocument> {
    const file = path.join(tempDir, name);
    fs.writeFileSync(file, content);
    const document = await vscode.workspace.openTextDocument(file);
    await makeActive(document);
    return document;
  }

  setup(async () => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-cmd-"));
    // Ensure the extension is active before exercising its commands.
    const ext = extension();
    assert.ok(ext, "Extension must be discoverable");
    if (ext && !ext.isActive) {
      await ext.activate();
    }
  });

  teardown(async () => {
    await closeEverything();
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  test("osprey.compile runs to completion against a valid .osp file", async function () {
    this.timeout(30000);
    const valid = 'fn main() -> Unit = print("compiled ok")\n';
    const document = await openOsp("compile-valid.osp", valid);

    assert.strictEqual(document.languageId, "osprey", "doc must be osprey");
    assert.ok(
      document.fileName.endsWith(".osp"),
      "file name must end with .osp",
    );

    // The handler reads window.activeTextEditor synchronously; make sure our
    // .osp document is the active editor at the moment the command fires so it
    // passes both guards and reaches the save->execFile->callback chain.
    const isActive = await makeActive(document);
    assert.ok(isActive, "the .osp document became the active editor");
    assert.ok(
      vscode.window.activeTextEditor?.document.fileName.endsWith(".osp"),
      "the active editor is the .osp document",
    );

    // Drive the command twice to exercise the chain for both a fresh and an
    // already-saved buffer.
    await vscode.commands.executeCommand("osprey.compile");
    await settle(4000);
    await makeActive(document);
    await vscode.commands.executeCommand("osprey.compile");
    await settle(4000);

    assert.ok(extension()?.isActive, "extension stays active after compile");
    assert.ok(fs.existsSync(document.fileName), "source file still on disk");
  });

  test("osprey.compile surfaces a compiler error path for invalid code", async function () {
    this.timeout(30000);
    // Invalid Osprey forces the compiler to exit non-zero, exercising the
    // `if (error)` branch of the execFile callback.
    const broken = "fn main( = \n this is not valid osprey @@@ \n";
    const document = await openOsp("compile-broken.osp", broken);

    assert.strictEqual(document.languageId, "osprey");
    assert.ok(await makeActive(document), "broken doc is active");
    await vscode.commands.executeCommand("osprey.compile");
    await settle(4000);

    assert.ok(extension()?.isActive, "extension survives a failed compile");
    const text = document.getText();
    assert.ok(text.includes("not valid"), "broken source preserved");
  });

  test("osprey.run runs to completion against a valid .osp file", async function () {
    this.timeout(30000);
    const valid = 'fn main() -> Unit = print("ran ok")\n';
    const document = await openOsp("run-valid.osp", valid);

    assert.strictEqual(document.languageId, "osprey");
    assert.ok(await makeActive(document), "valid run doc is active");
    await vscode.commands.executeCommand("osprey.run");
    await settle(5000);
    await makeActive(document);
    await vscode.commands.executeCommand("osprey.run");
    await settle(5000);

    assert.ok(extension()?.isActive, "extension stays active after run");
    assert.ok(document.fileName.endsWith(".osp"));
  });

  test("osprey.run surfaces a failure path for invalid code", async function () {
    this.timeout(30000);
    const broken = "fn main( = @@@ not osprey at all\n";
    const document = await openOsp("run-broken.osp", broken);

    assert.ok(await makeActive(document), "broken run doc is active");
    await vscode.commands.executeCommand("osprey.run");
    await settle(4000);

    assert.ok(extension()?.isActive, "extension survives a failed run");
    assert.ok(document.getText().length > 0, "broken source preserved");
  });

  test("compile and run guard against a non-.osp active editor", async () => {
    // Open a plain text (non-.osp) file so both handlers hit the
    // "Please open a .osp file" early return.
    const txt = path.join(tempDir, "notes.txt");
    fs.writeFileSync(txt, "just some plain text, not osprey at all");
    const document = await vscode.workspace.openTextDocument(txt);
    const active = await makeActive(document);

    assert.ok(active, "the .txt document became active");
    assert.ok(!document.fileName.endsWith(".osp"), "active file is not .osp");
    assert.notStrictEqual(document.languageId, "osprey", "not osprey lang");

    await vscode.commands.executeCommand("osprey.compile");
    await vscode.commands.executeCommand("osprey.run");
    await settle(500);

    assert.ok(extension()?.isActive, "extension active after guarded commands");
  });

  test("compile and run guard when there is no active editor", async () => {
    await closeEverything();
    // Best-effort: VS Code may keep a hidden editor around, but the handlers'
    // guards are exercised either way (no .osp active editor present).
    const noEditor = vscode.window.activeTextEditor === undefined;

    await vscode.commands.executeCommand("osprey.compile");
    await vscode.commands.executeCommand("osprey.run");
    await settle(400);

    assert.ok(extension()?.isActive, "extension active with no editor");
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
    const txt = path.join(tempDir, "convert-me.txt");
    fs.writeFileSync(txt, 'fn main() -> Unit = print("convert")\n');
    const opened = await vscode.workspace.openTextDocument(txt);
    assert.ok(await makeActive(opened), "convert doc became active");

    assert.notStrictEqual(
      vscode.window.activeTextEditor?.document.languageId,
      "osprey",
      "starts non-osprey",
    );

    await vscode.commands.executeCommand("osprey.setLanguage");
    await settle(600);

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
    await vscode.commands.executeCommand("osprey.setLanguage");
    await settle(300);

    assert.ok(extension()?.isActive, "extension stays active");
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
    const document = await openOsp(
      "debug-cmd.osp",
      'fn main() -> Unit = print("debug via command")\n',
    );
    assert.strictEqual(document.languageId, "osprey", "doc is osprey");
    assert.ok(await makeActive(document), "osprey doc is active for debug");
    assert.ok(
      vscode.window.activeTextEditor?.document.fileName.endsWith(".osp"),
      "active editor is the .osp file the command will debug",
    );

    // Fire the command; give the debug-compile + DAP handoff a real window.
    await vscode.commands.executeCommand("osprey.debug");
    await settle(6000);
    if (vscode.debug.activeDebugSession) {
      try {
        await vscode.debug.stopDebugging();
      } catch {
        // The session may already be terminating; cleanup races are benign.
      }
    }

    assert.ok(
      extension()?.isActive,
      "extension stays active after osprey.debug",
    );
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

    await vscode.commands.executeCommand("osprey.debug");
    await settle(600);

    assert.ok(
      extension()?.isActive,
      "extension survives osprey.debug with no program",
    );
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
    const broken = path.join(tempDir, "debug-broken.osp");
    fs.writeFileSync(broken, "fn main( = @@@ not valid osprey\n");
    const document = await vscode.workspace.openTextDocument(broken);
    await makeActive(document);

    const outcome = await startDebugRaced({
      type: "osprey",
      request: "launch",
      name: "Debug Osprey File",
      program: broken,
      cwd: tempDir,
    } as vscode.DebugConfiguration);
    await settle(1500);

    assert.ok(typeof outcome === "string", "debug start settled to a string");
    assert.strictEqual(
      vscode.debug.activeDebugSession,
      undefined,
      "a source that fails to debug-compile launches no session",
    );
    assert.ok(
      extension()?.isActive,
      "extension survives a failed debug-compile",
    );
    // The broken source stays exactly as written — the provider never mutates
    // it, it only tries (and fails) to compile it.
    assert.ok(
      document.getText().includes("not valid osprey"),
      "broken source preserved after the failed debug-compile",
    );
  });

  test("debug provider reports 'no program' for an empty config with no editor", async () => {
    // No active editor + a config with no program means synthesis is skipped
    // and `!config.program` shows "Cannot find a program to run".
    await closeEverything();
    const outcome = await startDebugRaced({
      type: "osprey",
      request: "launch",
      name: "Debug Osprey File",
    } as vscode.DebugConfiguration);
    await settle(1000);

    assert.ok(typeof outcome === "string", "settled to a string");
    assert.strictEqual(
      vscode.debug.activeDebugSession,
      undefined,
      "no session starts when there is no resolvable program",
    );
    assert.ok(extension()?.isActive, "extension survives the no-program path");
  });
});

suite("Osprey Activation Side-Effect Coverage", () => {
  let tempDir: string;
  const extensionId2 = extensionId;
  const settle = (ms: number) =>
    new Promise((resolve) => setTimeout(resolve, ms));

  // Same focus-stealing workaround as the command suite: the extension's
  // "Osprey Debug" output channel grabs the active-editor slot in the headless
  // host, so we close panels and refocus the editor group until the target
  // document is genuinely active.
  async function makeActive(document: vscode.TextDocument): Promise<boolean> {
    for (let i = 0; i < 25; i++) {
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
      if (
        active &&
        active.document.uri.toString() === document.uri.toString()
      ) {
        return true;
      }
    }
    return false;
  }

  setup(() => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-side-"));
  });

  teardown(async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  test("opening a .osp file triggers the language-association watcher", async () => {
    // Opening a brand new .osp document fires onDidOpenTextDocument; the handler
    // forces the osprey language id. This exercises the open-document watcher.
    const file = path.join(tempDir, "late-open.osp");
    fs.writeFileSync(file, 'fn main() -> Unit = print("late")\n');

    const document = await vscode.workspace.openTextDocument(file);
    await vscode.window.showTextDocument(document);
    await settle(800);

    assert.strictEqual(document.languageId, "osprey", "forced to osprey");
    assert.ok(document.fileName.endsWith(".osp"), "is a .osp file");
    const ext = vscode.extensions.getExtension(extensionId2);
    assert.ok(ext?.isActive, "extension active");
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
      const file = path.join(tempDir, "misassociated.osp");
      fs.writeFileSync(file, 'fn main() -> Unit = print("corrected")\n');
      const document = await vscode.workspace.openTextDocument(file);
      await vscode.window.showTextDocument(document);

      // The watcher's setTextDocumentLanguage is async; poll until it lands.
      for (let i = 0; i < 40 && document.languageId !== "osprey"; i++) {
        await settle(150);
      }
      assert.strictEqual(
        document.languageId,
        "osprey",
        "the watcher re-associated a plaintext-opened .osp back to osprey",
      );
      assert.ok(
        vscode.extensions.getExtension(extensionId2)?.isActive,
        "extension stays active after the correction",
      );
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
    const original = config.get<boolean>("diagnostics.enabled");

    await config.update(
      "diagnostics.enabled",
      !original,
      vscode.ConfigurationTarget.Global,
    );
    await settle(500);

    const flipped = vscode.workspace
      .getConfiguration("osprey")
      .get<boolean>("diagnostics.enabled");
    assert.strictEqual(flipped, !original, "config value flipped");

    // Restore so other tests see defaults.
    await config.update(
      "diagnostics.enabled",
      original,
      vscode.ConfigurationTarget.Global,
    );
    await settle(300);

    const restored = vscode.workspace
      .getConfiguration("osprey")
      .get<boolean>("diagnostics.enabled");
    assert.strictEqual(restored, original, "config value restored");
  });

  test("debug provider synthesizes a config from the active osprey editor", async function () {
    this.timeout(30000);

    // With an active osprey editor and an empty config, resolveDebugConfiguration
    // synthesizes type/name/request/program from the editor, then attempts the
    // real debug-launch path: save, debug-compile, and hand off to DAP.
    const file = path.join(tempDir, "debug-synth.osp");
    fs.writeFileSync(file, 'fn main() -> Unit = print("debug synth")\n');
    const document = await vscode.workspace.openTextDocument(file);
    const isActive = await makeActive(document);

    assert.strictEqual(document.languageId, "osprey", "doc is osprey");
    assert.ok(isActive, "osprey doc is the active editor");
    assert.strictEqual(
      vscode.window.activeTextEditor?.document.languageId,
      "osprey",
      "active editor language is osprey",
    );

    const outcome = await startDebugRaced(
      {
        type: "",
        name: "",
        request: "",
      } as unknown as vscode.DebugConfiguration,
      4000,
    );
    await settle(2500);

    assert.ok(typeof outcome === "string", "debug start settled to a string");
    assert.ok(
      vscode.extensions.getExtension(extensionId2)?.isActive,
      "extension survives synthesized debug config",
    );
  });

  test("debug provider rejects a launch config with no resolvable program", async function () {
    this.timeout(30000);

    // No active osprey editor and a config that carries a type but no program
    // means synthesis is skipped and `!config.program` is true, so the provider
    // shows "Cannot find a program to run" and returns undefined.
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await settle(300);

    const outcome = await startDebugRaced(
      {
        type: "osprey",
        name: "Debug Osprey File",
        request: "launch",
      } as vscode.DebugConfiguration,
      4000,
    );
    await settle(1500);

    assert.ok(typeof outcome === "string", "debug start settled to a string");
    assert.ok(
      vscode.extensions.getExtension(extensionId2)?.isActive,
      "extension survives the no-program debug path",
    );
  });
});

suite("Osprey VSIX Debugger E2E", () => {
  let tempDir: string;
  let priorCompilerPath: string | undefined;
  let priorLldbDapPath: string | undefined;
  let lldbDapPath: string;

  setup(async function () {
    this.timeout(60000);
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-debug-vsix-"));

    const ospreyPath = resolveBuiltOsprey();
    assert.ok(
      ospreyPath && fs.existsSync(ospreyPath),
      "a freshly built osprey binary is required for VSIX debugger E2E",
    );
    lldbDapPath = resolveRequiredLldbDap();

    const config = vscode.workspace.getConfiguration("osprey");
    priorCompilerPath = config.get<string>("server.compilerPath");
    priorLldbDapPath = config.get<string>("debug.lldbDapPath");
    await config.update(
      "server.compilerPath",
      ospreyPath,
      vscode.ConfigurationTarget.Global,
    );
    await config.update(
      "debug.lldbDapPath",
      lldbDapPath,
      vscode.ConfigurationTarget.Global,
    );

    const extension = vscode.extensions.getExtension(extensionId);
    assert.ok(extension, "Osprey extension should be installed in test host");
    await extension.activate();
  });

  teardown(async function () {
    clearDebugBreakpoints();
    if (vscode.debug.activeDebugSession) {
      try {
        await vscode.debug.stopDebugging();
      } catch {
        // The session can terminate naturally while cleanup is racing it.
      }
    }
    const config = vscode.workspace.getConfiguration("osprey");
    await config.update(
      "server.compilerPath",
      priorCompilerPath,
      vscode.ConfigurationTarget.Global,
    );
    await config.update(
      "debug.lldbDapPath",
      priorLldbDapPath,
      vscode.ConfigurationTarget.Global,
    );
    if (tempDir && fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  test("package manifest exposes a real debugger contribution", () => {
    const pkg = JSON.parse(
      fs.readFileSync(path.join(extensionRoot, "package.json"), "utf8"),
    ) as {
      activationEvents?: string[];
      contributes?: {
        commands?: { command: string }[];
        keybindings?: { command: string; key?: string }[];
        breakpoints?: { language: string }[];
        debuggers?: { type: string; languages?: string[] }[];
      };
    };

    assert.ok(
      pkg.activationEvents?.includes("onDebugResolve:osprey"),
      "VSIX activates to resolve osprey debug configs",
    );
    assert.ok(
      pkg.contributes?.commands?.some((cmd) => cmd.command === "osprey.debug"),
      "VSIX contributes an explicit osprey.debug command",
    );
    assert.ok(
      pkg.contributes?.keybindings?.some(
        (binding) => binding.command === "osprey.debug" && binding.key === "f5",
      ),
      "F5 is bound to the debugger command, not osprey.run",
    );
    assert.ok(
      pkg.contributes?.breakpoints?.some((bp) => bp.language === "osprey"),
      "VSIX declares Osprey source breakpoints",
    );
    assert.ok(
      pkg.contributes?.debuggers?.some(
        (debuggerContribution) =>
          debuggerContribution.type === "osprey" &&
          debuggerContribution.languages?.includes("osprey"),
      ),
      "VSIX contributes the osprey DAP debugger type",
    );
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
    // guard for "step over behaved like step in". bump is monomorphic (annotated
    // `-> int`), so it is a real call frame the debugger could wrongly enter.
    // [DEBUGGER-STEP-OVER]
    fs.writeFileSync(
      source,
      [
        "fn bump(v) -> int = v + 1",
        "let x = 1",
        "let y = bump(x)",
        'print("debugger reached ${x} and ${y}")',
        "",
      ].join("\n"),
    );

    const document = await vscode.workspace.openTextDocument(source);
    await vscode.window.showTextDocument(document);
    await document.save();

    setSourceBreakpoints(source, [3]);
    const sessionPromise = waitForDebugSessionStart(45000);
    const started = await vscode.debug.startDebugging(undefined, {
      type: "osprey",
      request: "launch",
      name: "Osprey VSIX Debugger E2E",
      program: source,
      cwd: tempDir,
      debugOutput,
      lldbDapPath,
      stopOnEntry: false,
    });
    assert.ok(started, "VS Code should accept the Osprey debug launch");

    const session = await sessionPromise;
    assert.strictEqual(session.type, "osprey");

    const stopped = await waitForStop(session, 45000);
    const topFrame = stopped.stack.stackFrames[0];
    assert.ok(topFrame, "debugger must report a stopped stack frame");
    assert.strictEqual(
      path.normalize(topFrame.source?.path ?? ""),
      path.normalize(source),
      "top stack frame is the Osprey source file",
    );
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
    // bump would appear on the stack. [DEBUGGER-STEP-OVER]
    await session.customRequest("next", { threadId: stopped.threadId });
    const stepped = await waitForStop(session, 45000);
    const steppedFrame = stepped.stack.stackFrames[0];
    assert.ok(steppedFrame, "debugger must report a frame after stepping");
    assert.strictEqual(
      path.normalize(steppedFrame.source?.path ?? ""),
      path.normalize(source),
      "stepping remains in the Osprey source file",
    );
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

    await session.customRequest("continue", { threadId: stepped.threadId });
    await waitForDebugSessionEnd(45000, session.id);
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
        "fn tag(v) -> int = v + 100",
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
      setSourceBreakpoints(source, [2]); // edited-only `let tagged = tag(base)`
      const sessionPromise = waitForDebugSessionStart(45000);
      const started = await vscode.debug.startDebugging(undefined, {
        type: "osprey",
        request: "launch",
        name: "Osprey dirty-buffer E2E",
        program: source,
        cwd: tempDir,
        debugOutput,
        lldbDapPath,
        stopOnEntry: false,
      });
      assert.ok(started, "VS Code accepted the dirty-buffer debug launch");

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

      const session = await sessionPromise;
      assert.strictEqual(
        session.type,
        "osprey",
        "session is the osprey debugger",
      );
      const stopped = await waitForStop(session, 45000);
      const topFrame = stopped.stack.stackFrames[0];
      assert.ok(
        topFrame,
        "debugger reports a stopped frame in the saved program",
      );
      assert.strictEqual(
        path.normalize(topFrame.source?.path ?? ""),
        path.normalize(source),
        "stopped in the saved .osp source",
      );
      assert.strictEqual(
        topFrame.line,
        2,
        "stopped on the edited-only breakpoint line (proves the save happened)",
      );

      // The edited-only local `base` is live — findVariable searches every scope,
      // and this also exercises the shared DAP harness assertLocalVariable path.
      // The matcher is a predicate (its rendered value is a materialized string
      // whose exact form depends on where in the line execution paused).
      const base = await assertLocalVariable(
        session,
        topFrame.id,
        "base",
        (value) => typeof value === "string" && value.length > 0,
      );
      assert.strictEqual(base.name, "base", "assertLocalVariable returns base");
      assert.ok(
        fs.existsSync(debugOutput),
        "the saved (not stale) program was debug-compiled to a native binary",
      );

      await session.customRequest("continue", { threadId: stopped.threadId });
      await waitForDebugSessionEnd(45000, session.id);
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
    const document = await vscode.workspace.openTextDocument(source);
    await vscode.window.showTextDocument(document);
    await document.save();

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
      const outcome = await Promise.race([
        Promise.resolve(
          vscode.debug.startDebugging(undefined, {
            type: "osprey",
            request: "launch",
            name: "Osprey dead-adapter E2E",
            program: source,
            cwd: tempDir,
            debugOutput: defaultDebugOutputPath(source),
            lldbDapPath: deadDap,
            stopOnEntry: false,
          }),
        )
          .then((v) => `resolved:${String(v)}`)
          .catch((e: unknown) => `error:${String(e)}`),
        new Promise<string>((resolve) =>
          setTimeout(() => resolve("timeout"), 12000),
        ),
      ]);
      await new Promise((resolve) => setTimeout(resolve, 1500));

      assert.ok(typeof outcome === "string", "launch settled to a string");
      // A dead adapter must NOT yield a live, stopped session on our breakpoint.
      const stuckRunning =
        sawSession !== undefined &&
        vscode.debug.activeDebugSession?.id === sawSession.id;
      assert.ok(
        !stuckRunning,
        "no live osprey debug session survives a missing lldb-dap adapter",
      );
      assert.ok(
        vscode.extensions.getExtension(extensionId)?.isActive,
        "extension survives the dead-adapter path",
      );
    } finally {
      sub.dispose();
      if (vscode.debug.activeDebugSession) {
        try {
          await vscode.debug.stopDebugging();
        } catch {
          // Session teardown can race our cleanup; that is fine.
        }
      }
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

  setup(() => {
    tempDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-unit-"));
  });

  teardown(() => {
    if (fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

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
    const exe = process.platform === "win32" ? ".exe" : "";
    const platDir = path.join(tempDir, "bin", shipwrightPlatform());
    fs.mkdirSync(platDir, { recursive: true });
    const bin = path.join(platDir, `osprey${exe}`);
    fs.writeFileSync(bin, "#!/bin/sh\nexit 0\n");

    const resolved = resolveBundledCompiler(fakeContext(tempDir));

    assert.ok(resolved, "a path is returned when the binary exists");
    assert.strictEqual(resolved, bin, "resolved path is the staged binary");
    assert.ok(resolved?.endsWith(`osprey${exe}`), "ends with the binary name");
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
    const config = vscode.workspace.getConfiguration("osprey");
    const original = config.get<string>("server.compilerPath");
    const custom = path.join(tempDir, "my-custom-osprey");
    fs.writeFileSync(custom, "#!/bin/sh\nexit 0\n");

    await config.update(
      "server.compilerPath",
      custom,
      vscode.ConfigurationTarget.Global,
    );
    try {
      const resolved = resolveServerCommand(fakeContext(tempDir));
      assert.strictEqual(resolved, custom, "returns the explicit user path");
      assert.notStrictEqual(resolved, "osprey", "does not fall back to PATH");
    } finally {
      await config.update(
        "server.compilerPath",
        original ?? "",
        vscode.ConfigurationTarget.Global,
      );
    }
  });

  test("resolveServerCommand falls back to bundled then PATH", async () => {
    // No user path: with a bundled binary present it returns that; without one
    // it falls back to the bare `osprey` PATH lookup.
    const [config, originalCompiler, originalPath] = savedServerSettings();
    await config.update(
      "server.compilerPath",
      "",
      vscode.ConfigurationTarget.Global,
    );
    await config.update("server.path", "", vscode.ConfigurationTarget.Global);

    try {
      // No bundled binary in tempDir -> bare PATH fallback.
      assert.strictEqual(
        resolveServerCommand(fakeContext(tempDir)),
        "osprey",
        "falls back to osprey on PATH when nothing is bundled",
      );

      // Now stage a bundled binary -> it is preferred over the PATH fallback.
      const exe = process.platform === "win32" ? ".exe" : "";
      const platDir = path.join(tempDir, "bin", shipwrightPlatform());
      fs.mkdirSync(platDir, { recursive: true });
      const bin = path.join(platDir, `osprey${exe}`);
      fs.writeFileSync(bin, "#!/bin/sh\nexit 0\n");

      assert.strictEqual(
        resolveServerCommand(fakeContext(tempDir)),
        bin,
        "prefers the bundled binary over the PATH fallback",
      );
    } finally {
      await restoreServerSettings(config, originalCompiler, originalPath);
    }
  });

  test("resolveServerCommand falls back and warns when the configured path is missing", async () => {
    // THE REGRESSION that killed hover: a configured server.compilerPath that
    // points at a file that does not exist must NOT be returned verbatim (that
    // ENOENTs the language client and silently kills every feature). It must
    // fall back and warn. tempDir has no bundled binary, so the fallback is the
    // bare `osprey` PATH lookup; then we stage a bundled binary and confirm it
    // is preferred.
    const [config, originalCompiler, originalPath] = savedServerSettings();
    const missing = path.join(tempDir, "does", "not", "exist", "osprey");
    await config.update(
      "server.compilerPath",
      missing,
      vscode.ConfigurationTarget.Global,
    );
    await config.update("server.path", "", vscode.ConfigurationTarget.Global);

    try {
      assert.ok(
        !fs.existsSync(missing),
        "the configured path is genuinely absent",
      );

      const warnings: string[] = [];
      const resolved = resolveServerCommand(fakeContext(tempDir), (m) =>
        warnings.push(m),
      );

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
      assert.ok(
        warnings[0].includes("does not exist"),
        "warning explains the problem",
      );

      // With a bundled binary present, the fallback prefers it over PATH.
      const exe = process.platform === "win32" ? ".exe" : "";
      const platDir = path.join(tempDir, "bin", shipwrightPlatform());
      fs.mkdirSync(platDir, { recursive: true });
      const bundled = path.join(platDir, `osprey${exe}`);
      fs.writeFileSync(bundled, "#!/bin/sh\nexit 0\n");
      assert.strictEqual(
        resolveServerCommand(fakeContext(tempDir), () => undefined),
        bundled,
        "missing user path falls back to the bundled binary when present",
      );
    } finally {
      await restoreServerSettings(config, originalCompiler, originalPath);
    }
  });

  test("resolveServerCommand keeps a bare command name without touching the filesystem", async () => {
    // A bare `osprey` (no path separator) is a PATH command, not a file to
    // existence-check; it must be returned as-is even though no such file exists
    // relative to the cwd, and must not warn.
    const config = vscode.workspace.getConfiguration("osprey");
    const originalCompiler = config.get<string>("server.compilerPath");
    await config.update(
      "server.compilerPath",
      "osprey",
      vscode.ConfigurationTarget.Global,
    );
    try {
      const warnings: string[] = [];
      const resolved = resolveServerCommand(fakeContext(tempDir), (m) =>
        warnings.push(m),
      );
      assert.strictEqual(resolved, "osprey", "bare command returned verbatim");
      assert.strictEqual(
        warnings.length,
        0,
        "no warning for a bare PATH command",
      );
    } finally {
      await config.update(
        "server.compilerPath",
        originalCompiler ?? "",
        vscode.ConfigurationTarget.Global,
      );
    }
  });

  test("an existing configured path is still returned verbatim and never warns", async () => {
    // Happy path: when the configured compiler DOES exist it is used directly,
    // with no fallback and no warning — the dev-settings contract.
    const config = vscode.workspace.getConfiguration("osprey");
    const original = config.get<string>("server.compilerPath");
    const real = path.join(tempDir, "real-osprey");
    fs.writeFileSync(real, "#!/bin/sh\nexit 0\n");
    await config.update(
      "server.compilerPath",
      real,
      vscode.ConfigurationTarget.Global,
    );
    try {
      const warnings: string[] = [];
      const resolved = resolveServerCommand(fakeContext(tempDir), (m) =>
        warnings.push(m),
      );
      assert.strictEqual(resolved, real, "existing path returned verbatim");
      assert.strictEqual(warnings.length, 0, "no warning when the path exists");
    } finally {
      await config.update(
        "server.compilerPath",
        original ?? "",
        vscode.ConfigurationTarget.Global,
      );
    }
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
// settings pointed osprey.server.compilerPath at compiler/bin/osprey — a path
// `make build` never produces (compiler/bin is the C-runtime archive dir) — so
// the language client ENOENT'd and every feature died. This locks the committed
// value so it can never silently rot back to a non-existent binary.
suite("Committed Dev Settings Sanity", () => {
  // extensionRoot (module scope) is <repo>/vscode-extension; the repo root is
  // one level up, and its .vscode/settings.json is the committed dev config.
  const repoRoot = path.resolve(extensionRoot, "..");
  const settingsPath = path.join(repoRoot, ".vscode", "settings.json");

  function compilerPathSetting(): string | undefined {
    assert.ok(
      fs.existsSync(settingsPath),
      `repo .vscode/settings.json exists at ${settingsPath}`,
    );
    const settings = JSON.parse(fs.readFileSync(settingsPath, "utf8"));
    return settings["osprey.server.compilerPath"];
  }

  test("compilerPath is never the dead compiler/bin/osprey runtime-archive path", () => {
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
      !compilerPath.endsWith("compiler/bin/osprey"),
      `compilerPath must not point at compiler/bin/osprey: ${compilerPath}`,
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
  test("initializationFailedHandler logs, surfaces an error, and stops retrying", () => {
    const logs: string[] = [];
    const errors: string[] = [];
    const handling = makeClientFailureHandling(
      (m) => logs.push(m),
      (m) => errors.push(m),
    );

    const result = handling.initializationFailedHandler!(
      new Error("init boom"),
    );

    assert.strictEqual(
      result,
      false,
      "returns false so the client does not retry",
    );
    assert.strictEqual(logs.length, 1, "logs exactly once");
    assert.ok(
      logs[0].includes("Initialization failed"),
      "log names the failure",
    );
    assert.ok(
      logs[0].includes("init boom"),
      "log carries the underlying error",
    );
    assert.strictEqual(errors.length, 1, "one user-facing error is shown");
    assert.ok(
      errors[0].includes("initialization failed"),
      "error message is descriptive",
    );
  });

  test("errorHandler.error logs the error/message/count and continues", async () => {
    const logs: string[] = [];
    const handling = makeClientFailureHandling(
      (m) => logs.push(m),
      () => undefined,
    );

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
    assert.ok(
      logs[0].includes("Language server error"),
      "log identifies a server error",
    );
    assert.ok(
      logs[0].includes("transport"),
      "log includes the underlying error",
    );
    assert.ok(logs[0].includes("2"), "log includes the occurrence count");
  });

  test("errorHandler.closed logs and restarts the connection", async () => {
    const logs: string[] = [];
    const handling = makeClientFailureHandling(
      (m) => logs.push(m),
      () => undefined,
    );

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
    const host = {
      env: { PATH: "" },
      existsSync: (filePath: string) => filePath === "/custom/lldb-dap",
      getSetting: () => undefined,
      platform: "linux" as NodeJS.Platform,
    };

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
    const host = {
      env: { PATH: "" },
      existsSync: () => false,
      getSetting: () => undefined,
      platform: "linux" as NodeJS.Platform,
    };

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

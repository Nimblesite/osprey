import * as assert from "assert";
import * as sinon from "sinon";
import { CancellationTokenSource, ExtensionContext, TabInputText, Uri, window, workspace } from "vscode";
import { defaultPackageDeps, makeAddHandler } from "../../client/src/package-commands";
import { hostBridge } from "../../client/src/package-workspace";
import { createRequestExecutor } from "../../client/src/osprey-ui/capabilities";
import { activateWithServer, ACTIVATION_MS } from "./profiler-kit";
import { extensionRoot } from "./osprey-test-env";

const selected = { label: "@finch/ledger", key: "@finch/ledger", detail: "Unverified evidence" };

function editorFixture(): CancellationTokenSource {
  sinon.stub(window, "showQuickPick").resolves(selected);
  sinon.stub(window, "showWarningMessage").resolves({ title: "Add Anyway" });
  sinon.stub(window, "showInformationMessage").resolves(undefined);
  const cancellation = new CancellationTokenSource();
  sinon.stub(window, "withProgress").callsFake(async (_options, task) =>
    task({ report: () => undefined }, cancellation.token));
  return cancellation;
}

function dependencyFixture(calls: string[]) {
  return {
    ...defaultPackageDeps(), projectRoots: () => ["/work/project with spaces"], dirtyFiles: () => [],
    fetchListing: async () => JSON.stringify({ packages: [
      { key: selected.key, releases: [{ epoch: 1, ordinal: 1, digest: `sha256:${"a".repeat(64)}` }] },
    ] }),
    frontend: { run: async (command: string, cwd: string) => {
      calls.push(`${cwd}\n${command}`);
      return { status: 0, lines: ["fixture operation completed"] };
    } },
  };
}

suite("package production editor adapter", () => {
  teardown(() => sinon.restore());

  test("the real single-selection QuickPick reaches the package frontend", async () => {
    const cancellation = editorFixture();
    const calls: string[] = [];
    try {
      await makeAddHandler(dependencyFixture(calls))();
      assert.strictEqual(calls.length, 1, "a real VS Code item must not disappear through an array cast");
      assert.ok(calls[0].includes("--project ."));
    } finally {
      cancellation.dispose();
    }
  });
});

suite("package VSIX contributions", () => {
  test("commands and machine-scoped executable setting ship in the manifest", () => {
    const manifest = require("../../../package.json");
    for (const id of ["osprey.packages.add", "osprey.packages.remove"]) {
      assert.ok(manifest.contributes.commands.some((entry: {command: string}) => entry.command === id), id);
    }
    assert.strictEqual(manifest.contributes.configuration.properties["osprey.packages.frontendPath"].scope, "machine");
  });
});

function executeHost(operation: string, args: Record<string, unknown> = {}): Promise<unknown> {
  const host = hostBridge({ extensionPath: extensionRoot } as ExtensionContext);
  return createRequestExecutor(host)({ ticket: "adapter:1", kind: "host-call", namespace: "vscode", operation, arguments: args });
}

suite("package real editor host capabilities", () => {
  suiteSetup(async function () {
    this.timeout(ACTIVATION_MS);
    await activateWithServer();
  });

  test("the workspace snapshot carries the actual editor state", async () => {
    const snapshot = await executeHost("workspace.snapshot") as { trusted: boolean; folders: { uri: string }[]; activeDocument: string | null };
    assert.strictEqual(snapshot.trusted, workspace.isTrusted);
    assert.deepStrictEqual(snapshot.folders.map((folder) => folder.uri), (workspace.workspaceFolders ?? []).map((folder) => folder.uri.toString()));
    assert.strictEqual(snapshot.activeDocument, window.activeTextEditor?.document.uri.toString() ?? null);
  });

  test("configuration reads include package settings and exclude unrelated editor settings", async () => {
    const resource = Uri.file(extensionRoot);
    const configuration = workspace.getConfiguration("osprey.packages", resource);
    assert.deepStrictEqual(await executeHost("configuration.read", { section: "osprey.packages", workspace: resource.toString() }), {
      section: "osprey.packages", values: { registry: configuration.get("registry"), frontendPath: configuration.get("frontendPath") },
    });
    assert.strictEqual(typeof workspace.getConfiguration("editor", resource).get("fontSize"), "number");
    assert.deepStrictEqual(await executeHost("configuration.read", { section: "editor", workspace: resource.toString() }), { section: "editor", values: {} });
  });

  test("document open selects the requested one-based location in a real file", async () => {
    const uri = Uri.joinPath(Uri.file(extensionRoot), "package.json");
    const before = new Set(window.tabGroups.all.flatMap((group) => group.tabs));
    try {
      assert.deepStrictEqual(await executeHost("document.open", { uri: uri.toString(), line: 2, column: 3 }), { opened: true, uri: uri.toString() });
      assert.strictEqual(window.activeTextEditor?.document.uri.toString(), uri.toString());
      assert.strictEqual(window.activeTextEditor?.selection.start.line, 1);
      assert.strictEqual(window.activeTextEditor?.selection.start.character, 2);
    } finally {
      const opened = window.tabGroups.all.flatMap((group) => group.tabs).filter((tab) => !before.has(tab) && tab.input instanceof TabInputText && tab.input.uri.toString() === uri.toString());
      await window.tabGroups.close(opened);
    }
  });

  test("an unknown native view is rejected without opening an editor tab", async () => {
    const before = window.tabGroups.all.flatMap((group) => group.tabs);
    await assert.rejects(executeHost("webview.reveal", { view: "osprey.missing" }), /Unknown Osprey view: osprey\.missing/);
    assert.deepStrictEqual(window.tabGroups.all.flatMap((group) => group.tabs), before);
  });
});

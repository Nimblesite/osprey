import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as sinon from "sinon";
import { ColorThemeKind, commands, EventEmitter, Uri, window, workspace, type ExtensionContext, type WebviewPanel, type WorkspaceConfiguration } from "vscode";
import * as host from "../../client/src/osprey-ui/panel";
import * as Wasi from "../../client/src/osprey-ui/wasi";
import { registerPackageWorkspace } from "../../client/src/package-workspace";
import { registerWorkbench } from "../../client/src/workbench";
import { shipwrightPlatform } from "../../client/src/client-config";
import { extensionRoot } from "./osprey-test-env";

const context = (): ExtensionContext => ({ extensionPath: extensionRoot, extensionUri: Uri.file(extensionRoot), subscriptions: [] } as unknown as ExtensionContext);
const application = (artifact: string): host.NativeApplication => ({ key: "fixture", artifact, context: {}, execute: async () => ({}) });
const options = (command: string, prepare: host.NativePanelOptions["prepare"]): host.NativePanelOptions => ({ command, prepare, title: "Native host regression", view: command, loading: "Loading fixture" });

suite("native startup appearance", () => {
  for (const [kind, expected] of [[ColorThemeKind.Dark, "dark"], [ColorThemeKind.HighContrast, "dark"],
    [ColorThemeKind.Light, "light"], [ColorThemeKind.HighContrastLight, "light"]] as const) {
    test(`editor theme kind ${kind} supplies ${expected} application context`, () => {
      sinon.stub(window, "activeColorTheme").get(() => ({ kind }));
      try { assert.strictEqual(host.nativeContext([]).theme, expected); }
      finally { sinon.restore(); }
    });
  }
});

function observePanels(): Map<WebviewPanel, sinon.SinonSpy> {
  const created = new Map<WebviewPanel, sinon.SinonSpy>();
  const create = window.createWebviewPanel.bind(window);
  sinon.stub(window, "createWebviewPanel").callsFake((...args) => {
    const panel = create(...args);
    created.set(panel, sinon.spy(panel, "dispose"));
    return panel;
  });
  return created;
}

function fixturePanel() {
  const closed = new EventEmitter<void>(), messages = new EventEmitter<unknown>();
  const panel = { dispose: () => closed.fire(), onDidDispose: closed.event,
    webview: { asWebviewUri: (uri: Uri) => uri, cspSource: "https://fixture.example", html: "",
      postMessage: async () => true, onDidReceiveMessage: messages.event } } as unknown as WebviewPanel;
  sinon.stub(window, "createWebviewPanel").returns(panel);
  return { panel, ready: () => messages.fire({ type: "ready" }), dispose: () => { closed.dispose(); messages.dispose(); } };
}

suite("native shared panel lifecycle", () => {
  let root: string;
  let artifact: string;
  setup(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-native-panel-"));
    artifact = path.join(root, "fixture.wasm");
    fs.writeFileSync(artifact, Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
  });
  teardown(() => { sinon.restore(); fs.rmSync(root, { recursive: true, force: true }); });

  test("closing a panel aborts its pending WASI command", async () => {
    const fixture = fixturePanel();
    const started = new Promise<AbortSignal | undefined>(resolve => {
      sinon.stub(Wasi, "runWasi").callsFake((_module, _input, options) => {
        resolve(options?.signal);
        return new Promise((_resolve, reject) => options?.signal?.addEventListener("abort", () => reject(new Error("cancelled")), { once: true }));
      });
    });
    const command = "osprey.test.native.cancel";
    const registration = host.registerNativePanel(context(), options(command, async () => application(artifact)));
    try {
      await commands.executeCommand(command);
      fixture.ready();
      const signal = await started;
      assert.ok(signal, "each panel must supply its own cancellation signal");
      assert.strictEqual(signal.aborted, false);
      fixture.panel.dispose();
      assert.strictEqual(signal.aborted, true, "closing the panel must stop pending execution");
    } finally { registration.dispose(); fixture.dispose(); }
  });

  test("failed stylesheet construction closes its panel and reports the actual error", async () => {
    const panels = observePanels();
    const errors = sinon.stub(window, "showErrorMessage");
    const command = "osprey.test.native.failed";
    const registration = host.registerNativePanel(context(), options(command,
      async () => ({ ...application(artifact), assets: root, stylesheet: "missing.css" })));
    try {
      await commands.executeCommand(command);
      assert.strictEqual(panels.size, 1);
      for (const disposed of panels.values()) assert.ok(disposed.calledOnce, "failed construction must dispose its orphan panel");
      assert.ok(String(errors.firstCall.args[0]).includes("missing.css"));
    } finally { registration.dispose(); for (const panel of panels.keys()) panel.dispose(); }
  });

  test("disposing a registration while preparation is pending cannot resurrect a panel", async () => {
    const panels = observePanels();
    let release = (_value: host.NativeApplication): void => assert.fail("prepare must initialize its completion");
    const prepared = new Promise<host.NativeApplication>(resolve => { release = resolve; });
    let registration: { dispose(): void };
    registration = host.registerNativePanel(context(), options("osprey.test.native.disposed", async () => {
      registration.dispose();
      return prepared;
    }));
    const pending = commands.executeCommand("osprey.test.native.disposed");
    release(application(artifact));
    try { await pending; assert.strictEqual(panels.size, 0, "disposed registration created a new panel"); }
    finally { registration.dispose(); for (const panel of panels.keys()) panel.dispose(); }
  });

  test("package registry changes produce a fresh application identity", async () => {
    const folder = { name: "fixture", index: 0, uri: Uri.file(root) };
    sinon.stub(workspace, "getWorkspaceFolder").returns(folder);
    let origin = "https://one.example";
    sinon.stub(workspace, "getConfiguration").returns({ get: (key: string) => key === "packages.registry" ? origin : "" } as WorkspaceConfiguration);
    const register = sinon.stub(host, "registerNativePanel").returns({ dispose: () => undefined });
    registerPackageWorkspace(context());
    const prepare = (register.firstCall.args[1] as host.NativePanelOptions).prepare;
    const before = await prepare(folder.uri);
    origin = "https://two.example";
    const after = await prepare(folder.uri);
    assert.ok(before && after);
    assert.notStrictEqual(before.key, after.key, "reopening must stop using the previous registry");
  });
});

async function preparedWorkbench(root: string): Promise<host.NativeApplication> {
  fs.mkdirSync(path.join(root, "bin", shipwrightPlatform(), "workbench"), { recursive: true });
  const register = sinon.stub(host, "registerNativePanel").returns({ dispose: () => undefined });
  registerWorkbench({ ...context(), extensionPath: root, extensionUri: Uri.file(root),
    globalStorageUri: Uri.file(path.join(root, "storage")) });
  const app = await (register.firstCall.args[1] as host.NativePanelOptions).prepare();
  assert.ok(app);
  return app;
}

suite("workbench document host wiring", () => {
  let root: string;
  setup(() => { root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-document-host-")); });
  teardown(() => { sinon.restore(); fs.rmSync(root, { recursive: true, force: true }); });

  test("Open uses a single-file picker and returns selected text or explicit cancellation", async () => {
    const file = path.join(root, "proof.ospml");
    fs.writeFileSync(file, "p ∧ q");
    const open = sinon.stub(window, "showOpenDialog").onFirstCall().resolves([Uri.file(file)]).onSecondCall().resolves(undefined);
    const app = await preparedWorkbench(root);
    const request = { ticket: "open", kind: "choose-file", types: ["ospml"], limit: 7, slice: "document" };
    assert.deepStrictEqual(await app.execute(request, new Map()), { result: { path: file, cancelled: false }, slices: [{ key: "document", text: "p ∧ q" }] });
    assert.deepStrictEqual(open.firstCall.args[0], { canSelectFiles: true, canSelectFolders: false, canSelectMany: false, filters: { Documents: ["ospml"] } });
    assert.deepStrictEqual(await app.execute(request, new Map()), { result: { path: "", cancelled: true }, slices: [] });
  });

  test("Save exports the current Osprey slice through the native save dialog", async () => {
    const file = path.join(root, "export.html");
    const save = sinon.stub(window, "showSaveDialog").resolves(Uri.file(file));
    const app = await preparedWorkbench(root);
    const result = await app.execute({ ticket: "save", kind: "save-file", name: "proof.html", slice: "export" }, new Map([["export", "<math>∧</math>"]]));
    assert.deepStrictEqual(result, { result: { path: file, cancelled: false }, slices: [] });
    assert.strictEqual(fs.readFileSync(file, "utf8"), "<math>∧</math>");
    assert.strictEqual(save.firstCall.args[0]?.defaultUri?.fsPath, path.join(os.homedir(), "proof.html"));
  });

  test("non-file selections fail explicitly before accessing a remote provider", async () => {
    sinon.stub(window, "showOpenDialog").resolves([Uri.parse("vscode-remote://ssh/secret")]);
    const app = await preparedWorkbench(root);
    await assert.rejects(app.execute({ ticket: "open", kind: "choose-file", types: [], limit: 10, slice: "document" }, new Map()), /Unsupported selected document scheme/);
  });
});

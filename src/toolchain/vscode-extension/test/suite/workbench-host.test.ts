import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as sinon from "sinon";
import { ColorThemeKind, commands, env, EventEmitter, Uri, window, workspace, type ExtensionContext, type WebviewPanel, type WorkspaceConfiguration } from "vscode";
import { Ask } from "../../client/src/osprey-ui/asks";
import * as host from "../../client/src/osprey-ui/panel";
import * as Runner from "../../client/src/osprey-ui/wasi-runner";
import { registerPackageWorkspace } from "../../client/src/package-workspace";
import { registerWorkbench } from "../../client/src/workbench";
import { shipwrightPlatform } from "../../client/src/client-config";
import { extensionRoot } from "./osprey-test-env";

const context = (): ExtensionContext => ({ extensionPath: extensionRoot, extensionUri: Uri.file(extensionRoot), subscriptions: [] } as unknown as ExtensionContext);
const openContext = host.nativeContext([]);
const application = (artifact: string): host.NativeApplication => ({ key: "fixture", artifact, context: openContext, execute: async () => ({ detail: "{}" }) });
const options = (command: string, prepare: host.NativePanelOptions["prepare"]): host.NativePanelOptions => ({ command, prepare, title: "Native host regression", view: command, loading: "Loading fixture" });

function ask(code: number, ticket: string, fields: readonly (readonly [number, number, string])[]): Ask {
  return new Ask(code, ticket, fields.map(([field, number, text]) => ({ field, number, text })));
}

suite("native startup appearance", () => {
  for (const [kind, dark, expected] of [[ColorThemeKind.Dark, true, "dark"], [ColorThemeKind.HighContrast, true, "dark"],
    [ColorThemeKind.Light, false, "light"], [ColorThemeKind.HighContrastLight, false, "light"]] as const) {
    test(`editor theme kind ${kind} supplies ${expected} application context`, () => {
      sinon.stub(window, "activeColorTheme").get(() => ({ kind }));
      try {
        const context = host.nativeContext([]);
        assert.strictEqual(context.dark, dark);
        assert.strictEqual(typeof context.now, "number");
      } finally { sinon.restore(); }
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
  return { panel, ready: () => messages.fire({ type: "ready" }), post: (message: unknown) => messages.fire(message),
    dispose: () => { closed.dispose(); messages.dispose(); } };
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
      sinon.stub(Runner, "startNativeRuntime").callsFake((_module, signal) => {
        resolve(signal);
        return {
          open: async () => new Promise((_resolve, reject) => signal?.addEventListener("abort", () => reject(new Error("cancelled")), { once: true })),
          event: async () => ({ status: 0, revision: 2, runs: { shapes: new ArrayBuffer(0), views: new ArrayBuffer(0), text: new ArrayBuffer(0), asks: new ArrayBuffer(0) } }),
          hold: () => undefined,
          dispose: () => undefined,
        };
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
      assert.strictEqual(signal.aborted, true, "closing the panel must stop its instance");
    } finally { registration.dispose(); fixture.dispose(); }
  });

  test("closing a panel aborts its pending WASI command (extra: disposing the registration stops its open panel too)", async () => {
    const fixture = fixturePanel();
    const started = new Promise<AbortSignal | undefined>(resolve => {
      sinon.stub(Runner, "startNativeRuntime").callsFake((_module, signal) => {
        resolve(signal);
        return {
          open: async () => new Promise((_resolve, reject) => signal?.addEventListener("abort", () => reject(new Error("cancelled")), { once: true })),
          event: async () => ({ status: 0, revision: 2, runs: { shapes: new ArrayBuffer(0), views: new ArrayBuffer(0), text: new ArrayBuffer(0), asks: new ArrayBuffer(0) } }),
          hold: () => undefined,
          dispose: () => undefined,
        };
      });
    });
    const command = "osprey.test.native.registration";
    const registration = host.registerNativePanel(context(), options(command, async () => application(artifact)));
    try {
      await commands.executeCommand(command);
      fixture.ready();
      const signal = await started;
      assert.ok(signal, "each panel must supply its own cancellation signal");
      registration.dispose();
      assert.strictEqual(signal.aborted, true, "disposing the registration must abort the panel it opened");
    } finally { registration.dispose(); fixture.dispose(); }
  });

  test("a webview link opens only as plain HTTPS, and malformed messages are ignored", async () => {
    const fixture = fixturePanel();
    const forwarded: unknown[] = [];
    sinon.stub(Runner, "startNativeRuntime").returns({ open: async () => new Promise(() => undefined), event: async () => { forwarded.push("event"); return new Promise(() => undefined); },
      hold: () => undefined, dispose: () => undefined } as never);
    const opened = sinon.stub(env, "openExternal").resolves(true);
    const errors = sinon.stub(window, "showErrorMessage").resolves(undefined);
    const command = "osprey.test.native.external";
    const registration = host.registerNativePanel(context(), options(command, async () => application(artifact)));
    try {
      await commands.executeCommand(command);
      for (const ignored of [null, "external", ["external"], { type: "external" }, { type: "external", url: 7 }]) fixture.post(ignored);
      fixture.post({ type: "external", url: "https://osprey.example/docs/flavors" });
      for (const url of ["http://osprey.example", "https://user:secret@osprey.example", "https://:secret@osprey.example", "javascript:alert(1)", "not a url"]) {
        fixture.post({ type: "external", url });
      }
      assert.deepStrictEqual(opened.getCalls().map((call) => call.args[0].toString()), ["https://osprey.example/docs/flavors"], "only the HTTPS link opens");
      const refusals = errors.getCalls().map((call) => String(call.args[0]));
      assert.deepStrictEqual(refusals.slice(0, 4), Array(4).fill("Native host regression: Refused a non-HTTPS external link"));
      assert.match(refusals[4] ?? "", /^Native host regression: .*Invalid URL/, "an unparsable link names the parse failure");
      assert.strictEqual(refusals.length, 5);
      assert.deepStrictEqual(forwarded, [], "no link or malformed message reaches the application");
    } finally { registration.dispose(); fixture.dispose(); }
  });

  test("failed stylesheet construction closes its panel and reports the actual error", async () => {
    const panels = observePanels();
    const errors = sinon.stub(window, "showErrorMessage");
    const command = "osprey.test.native.failed";
    const registration = host.registerNativePanel(context(), options(command, async () => ({ ...application(artifact), assets: root, stylesheet: "missing.css" })));
    try {
      await commands.executeCommand(command);
      assert.strictEqual(panels.size, 1);
      for (const disposed of panels.values()) assert.ok(disposed.calledOnce, "failed construction must dispose its orphan panel");
      assert.ok(String(errors.firstCall.args[0]).includes("missing.css"), String(errors.firstCall.args[0]));
    } finally { registration.dispose(); for (const panel of panels.keys()) panel.dispose(); }
  });

  test("failed stylesheet construction closes its panel and reports the actual error (extra: a stylesheet outside the asset root is refused too)", async () => {
    const panels = observePanels();
    const errors = sinon.stub(window, "showErrorMessage");
    const command = "osprey.test.native.escaped";
    fs.mkdirSync(path.join(root, "assets"), { recursive: true });
    const registration = host.registerNativePanel(context(), options(command, async () => ({ ...application(artifact), assets: path.join(root, "assets"), stylesheet: "../outside.css" })));
    try {
      await commands.executeCommand(command);
      assert.strictEqual(panels.size, 1);
      for (const disposed of panels.values()) assert.ok(disposed.calledOnce, "an escaped stylesheet must not leave an orphan panel");
      assert.ok(String(errors.firstCall.args[0]).length > 0, "the refusal names the stylesheet");
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
    // Over asks the chosen file crosses as its bare path; the workbench reads its text through its
    // own store read, and an empty path is the explicit cancellation.
    const file = path.join(root, "proof.ospml");
    fs.writeFileSync(file, "p ∧ q");
    const open = sinon.stub(window, "showOpenDialog").onFirstCall().resolves([Uri.file(file)]).onSecondCall().resolves(undefined);
    const app = await preparedWorkbench(root);
    const request = ask(13, "open", [[1, 0, "ospml"], [2, 7, ""], [3, 0, "document"]]);
    assert.deepStrictEqual(await app.execute(request, new Map()), { detail: file });
    assert.deepStrictEqual(open.firstCall.args[0], { canSelectFiles: true, canSelectFolders: false, canSelectMany: false, filters: { Documents: ["ospml"] } });
    assert.deepStrictEqual(await app.execute(request, new Map()), { detail: "" });
  });

  test("Open uses a single-file picker and returns selected text or explicit cancellation (extra: a multi-type filter keeps every extension)", async () => {
    const file = path.join(root, "notes.ospml");
    fs.writeFileSync(file, "x");
    const open = sinon.stub(window, "showOpenDialog").resolves([Uri.file(file)]);
    const app = await preparedWorkbench(root);
    await app.execute(ask(13, "open", [[1, 0, "ospml"], [1, 0, "osp"], [2, 7, ""], [3, 0, "document"]]), new Map());
    assert.deepStrictEqual(open.firstCall.args[0]?.filters, { Documents: ["ospml", "osp"] });
  });

  test("Save exports the current Osprey slice through the native save dialog", async () => {
    const file = path.join(root, "export.html");
    const save = sinon.stub(window, "showSaveDialog").resolves(Uri.file(file));
    const app = await preparedWorkbench(root);
    const result = await app.execute(ask(15, "save", [[1, 0, "proof.html"], [2, 0, "export"]]), new Map([["export", "<math>∧</math>"]]));
    assert.deepStrictEqual(result, { detail: file });
    assert.strictEqual(fs.readFileSync(file, "utf8"), "<math>∧</math>");
    assert.strictEqual(save.firstCall.args[0]?.defaultUri?.fsPath, path.join(os.homedir(), "proof.html"));
  });

  test("non-file selections fail explicitly before accessing a remote provider", async () => {
    sinon.stub(window, "showOpenDialog").resolves([Uri.parse("vscode-remote://ssh/secret")]);
    const app = await preparedWorkbench(root);
    await assert.rejects(app.execute(ask(13, "open", [[1, 0, "ospml"], [2, 10, ""], [3, 0, "document"]]), new Map()),
      /Unsupported selected document scheme/);
  });
});

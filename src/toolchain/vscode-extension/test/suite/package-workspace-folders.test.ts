// Runs in the two-project fixture workspace (.vscode-test.js): which project a package command
// manages, and which registry setting it accepts.
import * as assert from "assert";
import * as http from "http";
import * as sinon from "sinon";
import { commands, ExtensionContext, Uri, WebviewPanel, window, workspace } from "vscode";
import { folderFor, hostBridge } from "../../client/src/package-workspace";
import { activateWithServer } from "./profiler-kit";
import { withRegistrySetting } from "./real-registry-kit";

const roots = ["one", "two"].map((name, index) => ({ name, index, uri: Uri.file(`/work/${name}`) }));

suite("package workspace project safety", () => {
  teardown(() => sinon.restore());

  test("the capability host reads live trust after the panel opens", () => {
    let trusted = true;
    sinon.stub(workspace, "isTrusted").get(() => trusted);
    const host = hostBridge({ extensionPath: "/extension" } as ExtensionContext);
    assert.strictEqual(host.trusted, true);
    trusted = false;
    assert.strictEqual(host.trusted, false);
  });

  test("multi-root command palette selection requires an explicit project", async () => {
    sinon.stub(workspace, "workspaceFolders").get(() => roots);
    const item = { label: "two", uri: roots[1].uri };
    const picker = sinon.stub(window, "showQuickPick").resolves(item);
    const selected = await folderFor();
    assert.strictEqual(picker.callCount, 1);
    assert.strictEqual(selected?.toString(), roots[1].uri.toString());
  });

  test("cancelling multi-root selection never falls back to the first project", async () => {
    sinon.stub(workspace, "workspaceFolders").get(() => roots);
    sinon.stub(window, "showQuickPick").resolves(undefined);
    assert.strictEqual(await folderFor(), undefined);
  });

  test("an Explorer resource selects its own project without a picker", async () => {
    sinon.stub(workspace, "workspaceFolders").get(() => roots);
    sinon.stub(workspace, "getWorkspaceFolder").returns(roots[1]);
    const picker = sinon.stub(window, "showQuickPick");
    assert.strictEqual((await folderFor(Uri.file("/work/two/osprey.toml")))?.toString(), roots[1].uri.toString());
    assert.strictEqual(picker.callCount, 0);
  });
});

// Every editor surface reads `osprey.packages.registry` through one validator, so a setting the
// package commands refuse is refused by the package manager too, and a cleared setting means the default.
const REFUSED_REGISTRIES = ["http://packages.example", "https://user:secret@example.com", "https://example.com/path",
  "https://example.com/?token=x", "https://example.com/#top", "ftp://127.0.0.1", "http://192.168.1.10:8787"];

/** A loopback registry that only answers its health check, on every loopback address. */
async function withHealthyLoopback(run: (port: number) => Promise<void>): Promise<void> {
  const server = http.createServer((request, response) => { response.statusCode = request.url === "/v1/health" ? 200 : 404; response.end(); });
  await new Promise<void>((resolve) => server.listen(0, "::", resolve));
  try { await run((server.address() as { port: number }).port); }
  finally { server.closeAllConnections(); await new Promise<void>((resolve) => server.close(() => resolve())); }
}

/** `prompts` holds the registry prompts only; any `osprey.*` change also announces a restart. */
interface Opened { readonly panel?: WebviewPanel; readonly errors: string[]; readonly prompts: string[] }

/** Opens the package manager for `folder` (or the only project) with `osprey.packages.registry` set to `origin`. */
async function openWithRegistry(origin: string, folder?: Uri): Promise<Opened> {
  const panels = sinon.spy(window, "createWebviewPanel");
  const errors = sinon.stub(window, "showErrorMessage").resolves(undefined);
  const prompts = sinon.stub(window, "showInformationMessage").resolves(undefined);
  try {
    await withRegistrySetting(origin, async () => { await commands.executeCommand("osprey.packages.open", folder); });
    const panel = panels.lastCall?.returnValue as WebviewPanel | undefined;
    return { panel, errors: errors.getCalls().map((call) => String(call.args[0])), prompts: prompts.getCalls().map((call) => String(call.args[0])).filter((prompt) => prompt.includes("registry")) };
  } finally {
    panels.restore(); errors.restore(); prompts.restore();
  }
}

function assertOpened(opened: Opened, origin: string): void {
  assert.deepStrictEqual([opened.errors, opened.prompts], [[], []], `${origin} is a usable registry`);
  assert.ok(opened.panel, `${origin} opens the package manager`);
  assert.strictEqual(opened.panel.title, "Osprey Packages");
  opened.panel.dispose();
}

suite("package registry setting", () => {
  suiteSetup(activateWithServer);
  teardown(() => sinon.restore());

  test("the package manager refuses every registry the package commands refuse, and opens every one they accept", async () => {
    const folder = workspace.workspaceFolders?.[0]?.uri;
    assert.ok(folder, "runner supplies a real project");
    for (const origin of REFUSED_REGISTRIES) {
      const opened = await openWithRegistry(origin, folder);
      assert.strictEqual(opened.panel, undefined, `${origin} opens no package manager`);
      assert.deepStrictEqual(opened.errors, ["Osprey Packages: Package registry must be an HTTPS origin (HTTP is allowed only on loopback)."], origin);
    }
    await withHealthyLoopback(async (port) => {
      for (const origin of [`http://127.0.0.1:${port}`, `http://localhost:${port}`, `http://[::1]:${port}`, "https://localhost:1"]) {
        assertOpened(await openWithRegistry(origin, folder), origin);
      }
    });
    const cleared = await openWithRegistry("", folder);
    assert.deepStrictEqual([cleared.panel, cleared.errors], [undefined, []], "a cleared setting is the default registry, not an error");
    assert.match(cleared.prompts.join("\n"), /registry at http:\/\/127\.0\.0\.1:8787 is not running/, "the default registry is offered to start");
  });

  test("a single-project window opens its only project without asking", async () => {
    const folder = workspace.workspaceFolders?.[0];
    assert.ok(folder);
    sinon.stub(workspace, "workspaceFolders").get(() => [folder]);
    const picker = sinon.stub(window, "showQuickPick");
    assert.strictEqual((await folderFor())?.toString(), folder.uri.toString(), "one project needs no picker");
    await withHealthyLoopback(async (port) => assertOpened(await openWithRegistry(`http://127.0.0.1:${port}`), "the only project"));
    assert.strictEqual(picker.callCount, 0, "neither the resolver nor the command asks");
  });

  test("a window with no project opens nothing and says why", async () => {
    sinon.stub(workspace, "workspaceFolders").get(() => undefined);
    const picker = sinon.stub(window, "showQuickPick");
    assert.strictEqual(await folderFor(), undefined, "no project resolves");
    assert.deepStrictEqual(hostBridge({ extensionPath: "/extension" } as ExtensionContext).workspaceFolders, [], "the capability host sees no folders");
    const opened = await openWithRegistry("https://localhost:1");
    assert.deepStrictEqual([opened.panel, opened.prompts], [undefined, []], "nothing opens or is offered");
    assert.deepStrictEqual(opened.errors, ["Open a workspace folder before managing Osprey packages."]);
    assert.strictEqual(picker.callCount, 0, "there is nothing to choose from");
  });
});

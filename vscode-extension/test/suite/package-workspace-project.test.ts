import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import type { Browser, Frame } from "playwright-core";
import * as http from "http";
import * as sinon from "sinon";
import { commands, ConfigurationTarget, ExtensionContext, Uri, WebviewPanel, window, workspace, WorkspaceEdit, Range, Position, extensions } from "vscode";
import { activateWithServer, connectEditorBrowser, editorWebviewFrame, waitFor, EXTENSION_ID } from "./profiler-kit";
import { installRegistry, servedPackage, sdkHostRequests, sdkRequest } from "./package-harness";
import { createRequestExecutor } from "../../client/src/osprey-ui/capabilities";
import { record, type NativeRequest } from "../../client/src/osprey-ui/protocol";
import { folderFor, hostBridge } from "../../client/src/package-workspace";

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

function renderedHtml(messages: sinon.SinonSpy): string {
  return messages.getCalls().map((call) => call.args[0] as { type?: string; html?: string })
    .filter((message) => message.type === "render").at(-1)?.html ?? "";
}

async function observeWorkspace(command: string, folder: Uri, panels: sinon.SinonSpy): Promise<WebviewPanel> {
  await commands.executeCommand(command, folder);
  const panel = panels.lastCall.returnValue as WebviewPanel;
  assert.ok(panel, "activated command creates a real webview panel");
  const messages = sinon.spy(panel.webview, "postMessage");
  await waitFor(() => renderedHtml(messages), (html) => html.includes("Installed counts unavailable") && html.includes("No lockfile is available."),
    8000, "actual WASM reports missing lock without inventing installed counts");
  assert.ok(renderedHtml(messages).includes(folder.path.split("/").pop() ?? ""), "render identifies selected project");
  return panel;
}

async function catalogFixture(listing: unknown, sources: ReadonlyMap<string, unknown>, catalogStatus: () => number) {
  let reads = 0;
  const server = http.createServer((request, response) => {
    // The registry's liveness probe, which opening the package manager checks
    // first ([PACKAGE-LOCAL-REGISTRY]); it is not a catalog read.
    if (request.url === "/v1/health") {
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(JSON.stringify({ status: "ok" }));
      return;
    }
    reads++;
    const body = request.url === "/v1/packages" ? listing : sources.get(request.url ?? "");
    response.writeHead(body === undefined ? 404 : request.url === "/v1/packages" ? catalogStatus() : 200, { "Content-Type": "application/json" });
    response.end(JSON.stringify(body ?? { error: "not-found" }));
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address !== "string");
  return { server, reads: () => reads, origin: `http://127.0.0.1:${address.port}` };
}

async function withCatalog(run: (reads: () => number) => Promise<void>, listing: unknown = { packages: [] }, sources: ReadonlyMap<string, unknown> = new Map(), catalogStatus: () => number = () => 200): Promise<void> {
  const fixture = await catalogFixture(listing, sources, catalogStatus);
  const config = workspace.getConfiguration("osprey");
  const before = config.inspect<string>("packages.registry")?.workspaceValue;
  try {
    await config.update("packages.registry", fixture.origin, ConfigurationTarget.Workspace);
    await run(fixture.reads);
  } finally {
    await config.update("packages.registry", before, ConfigurationTarget.Workspace);
    await new Promise<void>((resolve) => fixture.server.close(() => resolve()));
  }
}

async function verifyPanelLifecycle(reads: () => number, panels: sinon.SinonSpy): Promise<void> {
  const folders = workspace.workspaceFolders ?? [];
  assert.strictEqual(folders.length, 2, "runner supplies two isolated real projects");
  const first = await observeWorkspace("osprey.packages.add", folders[0].uri, panels);
  await waitFor(reads, (count) => count > 0, 8000, "real WASM capability fetches catalog");
  await commands.executeCommand("osprey.packages.remove", folders[0].uri);
  assert.strictEqual(panels.callCount, 1, "same project reuses its live panel");
  let disposed = false;
  first.onDidDispose(() => { disposed = true; });
  const second = await observeWorkspace("osprey.packages.open", folders[1].uri, panels);
  assert.ok(disposed, "switching project disposes the old session's panel");
  assert.strictEqual(panels.callCount, 2);
  second.dispose();
}

suite("package workspace real host lifecycle", () => {
  teardown(() => sinon.restore());
  test("real WASM loads a project, reuses its panel and switches projects", async () => {
    await activateWithServer();
    const panels = sinon.spy(window, "createWebviewPanel");
    try { await withCatalog((reads) => verifyPanelLifecycle(reads, panels)); }
    finally { for (const call of panels.getCalls()) (call.returnValue as WebviewPanel).dispose(); }
  });
});



const browserCatalog = { packages: [{ key: "@demo/greeting", releases: [
  { epoch: 1, ordinal: 1, digest: `sha256:${"a".repeat(64)}` },
] }] };

async function withPackageBrowser(run: (frame: Frame, folder: Uri) => Promise<void>, listing: unknown = browserCatalog, sources: ReadonlyMap<string, unknown> = new Map(), catalogStatus: () => number = () => 200): Promise<void> {
  await activateWithServer();
  const folder = workspace.workspaceFolders?.[0];
  assert.ok(folder);
  const panels = sinon.spy(window, "createWebviewPanel");
  const restoreProject = projectSnapshot(folder.uri);
  let browser: Browser | undefined;
  try {
    await withCatalog(async () => {
      browser = await connectEditorBrowser();
      await observeWorkspace("osprey.packages.open", folder.uri, panels);
      await runBrowserFlow(await editorWebviewFrame(browser, '[role="tab"][data-value="browse"]'), folder.uri, run);
    }, listing, sources, catalogStatus);
  } finally {
    await closeBrowserPanels(browser, panels);
    restoreProject();
  }
}

function projectSnapshot(folder: Uri): () => void {
  assert.ok(path.basename(path.dirname(folder.fsPath)).startsWith("osp-package-workspace-"), "only restore runner-owned fixtures");
  const manifest = path.join(folder.fsPath, "osprey.toml");
  const original = fs.readFileSync(manifest, "utf8");
  assert.ok(!fs.existsSync(path.join(folder.fsPath, "osprey.lock")), "each flow starts without a lock");
  return () => {
    fs.writeFileSync(manifest, original);
    for (const name of ["osprey.lock", ".osprey"]) fs.rmSync(path.join(folder.fsPath, name), { recursive: true, force: true });
  };
}

async function closeBrowserPanels(browser: Browser | undefined, panels: sinon.SinonSpy): Promise<void> {
  await browser?.close();
  for (const call of panels.getCalls()) (call.returnValue as WebviewPanel).dispose();
}

async function runBrowserFlow(frame: Frame, folder: Uri, run: (frame: Frame, folder: Uri) => Promise<void>): Promise<void> {
  try { await run(frame, folder); }
  catch (error) {
    console.error("Package UI at failure:", await frame.locator("body").innerText());
    throw error;
  }
}

async function searchCatalog(frame: Frame): Promise<void> {
  const browse = frame.getByRole("tab", { name: "Browse registry" });
  await browse.click();
  await pollUntilResult(() => browse.getAttribute("aria-selected"), (selected) => selected === "true");
  const search = frame.getByPlaceholder("Search @organization/package");
  await search.fill("not-a-real-package");
  await frame.getByText("No registry packages", { exact: true }).waitFor();
  assert.strictEqual(await frame.getByRole("option").count(), 0, "search removes unrelated rows");
  await search.fill("greeting");
  await frame.getByRole("option").filter({ hasText: "@demo/greeting" }).click();
  await frame.getByText("Audit status unverified:", { exact: false }).waitFor();
}

async function cancelUnverifiedAdd(frame: Frame, folder: Uri): Promise<void> {
  const manifest = path.join(folder.fsPath, "osprey.toml");
  const before = fs.readFileSync(manifest, "utf8");
  await frame.locator(".osp-context-body").click({ button: "right" });
  await frame.getByRole("menu").getByRole("button", { name: "Add to project", exact: true }).click();
  await frame.getByRole("alert").filter({ hasText: "Audit status unverified" }).waitFor();
  await frame.getByRole("button", { name: "Cancel", exact: true }).click();
  await frame.getByRole("alert").waitFor({ state: "detached", timeout: 4000 });
  assert.strictEqual(await frame.getByRole("alert").count(), 0, "Cancel closes the confirmation");
  assert.strictEqual(fs.readFileSync(manifest, "utf8"), before, "Cancel preserves the real manifest");
  assert.strictEqual(fs.existsSync(path.join(folder.fsPath, "osprey.lock")), false, "Cancel creates no lock");
}

suite("package workspace browser interaction", () => {
  teardown(() => sinon.restore());
  test("browse, search and cancel Add through the real Osprey webview", async () => {
    await withPackageBrowser(async (frame, folder) => {
      await searchCatalog(frame);
      await cancelUnverifiedAdd(frame, folder);
      await frame.locator("body").screenshot({ path: path.resolve(__dirname,
        "../../../../target/registry-ownership-check/workspace-package-details.png") });
    });
  });
});


async function installFromBrowser(frame: Frame): Promise<void> {
  await frame.getByRole("tab", { name: "Browse registry" }).click();
  await frame.getByRole("option").filter({ hasText: "@demo/greeting" }).click();
  await frame.getByRole("button", { name: "Add to project", exact: true }).click();
  await frame.getByRole("button", { name: "Confirm add @demo/greeting", exact: true }).click();
  await frame.getByText("2 installed · 1 direct · 0 missing · 0 findings", { exact: true }).waitFor({ timeout: 4000 });
  await frame.getByRole("tab", { name: "Installed", exact: true }).click();
  await frame.getByRole("option").filter({ hasText: "@demo/greeting" }).click();
  await frame.getByText("Dependencies", { exact: true }).waitFor();
}

async function inspectDependencyGraph(frame: Frame): Promise<void> {
  const dependency = frame.locator("section").filter({ has: frame.getByText("Dependencies", { exact: true }) });
  await dependency.getByText("@demo/maths", { exact: true }).waitFor();
  await frame.getByRole("option").filter({ hasText: "@demo/maths" }).click();
  const dependents = frame.locator("section").filter({ has: frame.getByText("Required by", { exact: true }) });
  await dependents.getByText("@demo/greeting", { exact: true }).waitFor();
  assert.strictEqual(await frame.getByRole("button", { name: "Remove dependency", exact: true }).isEnabled(), false,
    "a transitive package cannot be removed as a direct dependency");
  await frame.getByRole("option").filter({ hasText: "@demo/greeting" }).click();
  await dependency.getByText("@demo/maths", { exact: true }).waitFor();
  await frame.locator("body").screenshot({ path: path.resolve(__dirname,
    "../../../../target/registry-ownership-check/workspace-installed-browser.png") });
}

async function tamperInstalledSource(frame: Frame, folder: Uri, pkg: ReturnType<typeof servedPackage>): Promise<void> {
  const source = path.join(folder.fsPath, ".osprey/packages", pkg.key, pkg.digest.slice(7), "src/main.osp");
  fs.writeFileSync(source, "fn value() = 0\n");
  await frame.getByRole("button", { name: "Refresh", exact: true }).click();
  await frame.getByText("verify.digest", { exact: true }).first().waitFor({ timeout: 4000 });
  assert.strictEqual(await frame.getByRole("button", { name: "Add to project", exact: true }).isEnabled(), false,
    "a package with integrity findings cannot be added");
}

async function removeFromBrowser(frame: Frame, folder: Uri): Promise<void> {
  await frame.locator(".osp-context-body").click({ button: "right" });
  await frame.getByRole("menu").getByRole("button", { name: "Remove dependency", exact: true }).click();
  await frame.getByRole("button", { name: "Confirm remove @demo/greeting", exact: true }).click();
  await frame.getByText("0 installed · 0 direct · 0 missing · 0 findings", { exact: true }).waitFor({ timeout: 4000 });
  assert.ok(!fs.readFileSync(path.join(folder.fsPath, "osprey.toml"), "utf8").includes("@demo/greeting"));
}

suite("package workspace browser installation", () => {
  teardown(() => sinon.restore());
  test("real Add verifies two packages and Remove resolves their dependency graph", async () => {
    const registry = installRegistry();
    await withPackageBrowser(async (frame, folder) => {
      assert.ok(folder.fsPath.includes(" "), "real package operations run from a workspace path containing spaces");
      await installFromBrowser(frame);
      for (const pkg of registry.packages) {
        const stored = path.join(folder.fsPath, ".osprey/packages", pkg.key, pkg.digest.slice(7));
        for (const file of pkg.files) assert.strictEqual(fs.readFileSync(path.join(stored, file.path), "utf8"), file.text);
      }
      await inspectDependencyGraph(frame);
      await tamperInstalledSource(frame, folder, registry.packages[0]);
      await removeFromBrowser(frame, folder);
    }, registry.listing, registry.sources);
  });
});

async function verifyRegistryRecovery(frame: Frame, setStatus: (status: number) => void): Promise<void> {
  await searchCatalog(frame);
  setStatus(503);
  await frame.getByRole("button", { name: "Refresh", exact: true }).click();
  await frame.getByText("Package data unavailable", { exact: true }).waitFor();
  await frame.getByText("Registry returned HTTP 503", { exact: true }).waitFor();
  assert.strictEqual(await frame.getByRole("option").count(), 0, "failure hides package rows");
  assert.strictEqual(await frame.getByRole("button", { name: "Add to project", exact: true }).count(), 0);
  setStatus(200);
  await frame.getByRole("button", { name: "Refresh", exact: true }).click();
  await frame.getByRole("option").filter({ hasText: "@demo/greeting-recovered" }).waitFor();
  assert.strictEqual(await frame.getByText("Package data unavailable", { exact: true }).count(), 0);
}

async function withDirtyManifest(folder: Uri, run: () => Promise<void>): Promise<void> {
  const document = await workspace.openTextDocument(Uri.joinPath(folder, "osprey.toml"));
  const original = document.getText();
  const edit = new WorkspaceEdit();
  edit.insert(document.uri, document.positionAt(original.length), "\n# unsaved editor edit\n");
  assert.ok(await workspace.applyEdit(edit));
  assert.ok(document.isDirty, "the real editor buffer is dirty");
  try { await run(); }
  finally {
    const restore = new WorkspaceEdit();
    restore.replace(document.uri, new Range(new Position(0, 0), document.positionAt(document.getText().length)), original);
    assert.ok(await workspace.applyEdit(restore));
    assert.ok(await document.save());
  }
}

async function verifyDirtyAdd(frame: Frame, folder: Uri): Promise<void> {
  await searchCatalog(frame);
  const manifest = fs.readFileSync(path.join(folder.fsPath, "osprey.toml"), "utf8");
  await withDirtyManifest(folder, async () => {
    await frame.getByRole("button", { name: "Add to project", exact: true }).click();
    await frame.getByRole("button", { name: "Confirm add @demo/greeting", exact: true }).click();
    await frame.getByText("Save or revert unsaved osprey.toml and osprey.lock changes before running package commands", { exact: true }).waitFor();
    assert.strictEqual(fs.readFileSync(path.join(folder.fsPath, "osprey.toml"), "utf8"), manifest);
    assert.strictEqual(fs.existsSync(path.join(folder.fsPath, "osprey.lock")), false);
    assert.strictEqual(fs.existsSync(path.join(folder.fsPath, ".osprey")), false);
  });
}

async function verifyOrgScopedRows(frame: Frame): Promise<void> {
  await frame.getByRole("tab", { name: "Browse registry" }).click();
  for (const key of ["@my-org/helpers", "@other-org/helpers"]) {
    const row = frame.getByRole("option").filter({ hasText: key });
    await row.waitFor();
    assert.strictEqual(await row.getByText("No findings recorded; audit status unverified", { exact: true }).count(), 1,
      `${key} carries its own security warning`);
    await row.click();
    await frame.locator(".osp-context-body").getByText(key, { exact: true }).waitFor();
  }
  assert.strictEqual(await frame.getByRole("option").count(), 2, "orgs may reuse leaf names");
}

async function verifyUnscopedRefusal(frame: Frame, invalidate: () => void): Promise<void> {
  await searchCatalog(frame);
  invalidate();
  await frame.getByRole("button", { name: "Refresh", exact: true }).click();
  await frame.getByText("HelperFunctions is not a canonical package key", { exact: true }).waitFor();
  assert.strictEqual(await frame.getByRole("option").count(), 0, "unscoped identities are never install choices");
  assert.strictEqual(await frame.getByRole("button", { name: "Add to project", exact: true }).count(), 0);
}

suite("package workspace browser safety", () => {
  teardown(() => sinon.restore());
  test("typing while a tab render is pending preserves the actual search event", async () => {
    await withPackageBrowser(async (frame) => {
      await frame.evaluate(() => {
        const tab = document.querySelector<HTMLButtonElement>('[role="tab"][data-value="browse"]');
        const search = document.querySelector<HTMLInputElement>('input[placeholder="Search @organization/package"]');
        if (!tab || !search) throw new Error("real package controls are absent");
        tab.click();
        search.focus();
        search.value = "not-a-real-package";
        search.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await frame.getByText("No registry packages", { exact: true }).waitFor();
      assert.strictEqual(await frame.getByRole("option").count(), 0);
    });
  });
  test("duplicate leaf names remain distinct and warned under different orgs", async () => {
    const listing = { packages: ["@my-org/helpers", "@other-org/helpers"].map((key) => ({
      ...structuredClone(browserCatalog.packages[0]), key,
    })) };
    await withPackageBrowser(verifyOrgScopedRows, listing);
  });
  test("an unscoped catalog identity hides package choices with a visible reason", async () => {
    const listing = structuredClone(browserCatalog);
    await withPackageBrowser((frame) => verifyUnscopedRefusal(frame, () => {
      listing.packages[0].key = "HelperFunctions";
    }), listing);
  });
  test("a real catalog error hides rows and a successful refresh recovers", async () => {
    let status = 200;
    const listing = structuredClone(browserCatalog);
    await withPackageBrowser((frame) => verifyRegistryRecovery(frame, (next) => {
      status = next;
      if (next === 200) listing.packages[0].key = "@demo/greeting-recovered";
    }), listing, new Map(), () => status);
  });
  test("a real unsaved manifest blocks confirmed Add without disk mutations", async () => {
    await withPackageBrowser(verifyDirtyAdd);
  });
});

interface SdkScenario {
  readonly frame: Frame;
  readonly folder: Uri;
  readonly document: Uri;
  readonly requests: readonly NativeRequest[];
  readonly execute: (request: NativeRequest) => Promise<unknown>;
}

async function sdkScenario(frame: Frame, folder: Uri, document: Uri): Promise<SdkScenario> {
  fs.writeFileSync(document.fsPath, "alpha\nsecond\nomega\n");
  const extension = extensions.getExtension(EXTENSION_ID);
  const peer = workspace.workspaceFolders?.[1];
  assert.ok(extension && peer, "real activated extension and second workspace exist");
  const requests = await sdkHostRequests([document.toString(), folder.toString(), peer.uri.toString(),
    Uri.joinPath(folder, "missing-sdk-document.txt").toString()]);
  return { frame, folder, document, requests, execute: createRequestExecutor(hostBridge(extension)) };
}

async function withSdkScenario(run: (scenario: SdkScenario) => Promise<void>): Promise<void> {
  let document: Uri | undefined;
  try {
    await withPackageBrowser(async (frame, folder) => {
      document = Uri.joinPath(folder, "sdk-inspection.txt");
      await run(await sdkScenario(frame, folder, document));
    });
  } finally {
    await commands.executeCommand("workbench.action.closeAllEditors");
    if (document) fs.rmSync(document.fsPath, { force: true });
  }
}

async function withScopedRegistries(run: () => Promise<void>): Promise<void> {
  const scopes = (workspace.workspaceFolders ?? []).map((folder) => workspace.getConfiguration("osprey.packages", folder.uri));
  const previous = scopes.map((scope) => scope.inspect<string>("registry")?.workspaceFolderValue);
  try {
    for (const [index, scope] of scopes.entries()) await scope.update("registry", `https://sdk-${index}.example`, ConfigurationTarget.WorkspaceFolder);
    await run();
  } finally {
    for (const [index, scope] of scopes.entries()) await scope.update("registry", previous[index], ConfigurationTarget.WorkspaceFolder);
  }
}

async function verifySdkDocument(scenario: SdkScenario): Promise<void> {
  const { document, requests, execute } = scenario;
  assert.deepStrictEqual(await execute(sdkRequest(requests, "sdk/open")), { opened: true, uri: document.toString() });
  assert.strictEqual(window.activeTextEditor?.document.uri.toString(), document.toString());
  assert.strictEqual(window.activeTextEditor?.selection.active.line, 1);
  assert.strictEqual(window.activeTextEditor?.selection.active.character, 2);
  const snapshot = await execute(sdkRequest(requests, "sdk/snapshot"));
  assert.ok(record(snapshot));
  assert.strictEqual(snapshot.activeDocument, document.toString());
  assert.strictEqual(snapshot.trusted, workspace.isTrusted);
}

async function verifySdkConfiguration(scenario: SdkScenario): Promise<void> {
  for (const [ticket, expected] of [["sdk/config-one", "https://sdk-0.example"], ["sdk/config-two", "https://sdk-1.example"]]) {
    const result = await scenario.execute(sdkRequest(scenario.requests, ticket));
    assert.ok(record(result) && record(result.values));
    assert.strictEqual(result.section, "osprey.packages");
    assert.strictEqual(result.values.registry, expected, "configuration resolves against the request's real resource");
  }
}

async function verifySdkReveal(scenario: SdkScenario): Promise<void> {
  const revealing = scenario.execute(sdkRequest(scenario.requests, "sdk/reveal"));
  const page = scenario.frame.page();
  const picker = page.getByPlaceholder("Package changes apply only to this project");
  try {
    await picker.fill("first");
    await page.getByRole("option").filter({ hasText: "first" }).first().click();
    assert.deepStrictEqual(await revealing, { revealed: true });
    await scenario.frame.getByRole("button", { name: "Refresh", exact: true }).waitFor();
  } finally { if (await picker.isVisible()) await picker.press("Escape"); }
}

async function verifySdkRefusals(scenario: SdkScenario): Promise<void> {
  for (const [ticket, message] of [
    ["sdk/bad-view", /Unknown Osprey view: osprey.unknown/], ["sdk/missing", /missing-sdk-document.txt/],
    ["sdk/no-doc-args", /document.open is missing its arguments/], ["sdk/remote-doc", /document.open requires a file resource/],
    ["sdk/no-config-args", /configuration.read is missing its arguments/], ["sdk/no-reveal-args", /webview.reveal is missing its arguments/],
    ["sdk/bad-namespace", /Unknown host namespace: other/], ["sdk/unknown-operation", /Unsupported VS Code capability: impossible.operation/],
  ] as const) await assert.rejects(() => scenario.execute(sdkRequest(scenario.requests, ticket)), message);
  assert.strictEqual(fs.existsSync(Uri.joinPath(scenario.folder, "missing-sdk-document.txt").fsPath), false);
}

suite("package workspace SDK host E2E", () => {
  teardown(() => sinon.restore());
  test("real compiled SDK requests open documents, scope configuration and reveal the view", async () => {
    await withSdkScenario((scenario) => withScopedRegistries(async () => {
      await verifySdkDocument(scenario);
      await verifySdkConfiguration(scenario);
      await verifySdkReveal(scenario);
    }));
  });
  test("real compiled SDK requests refuse invalid host operations without side effects", async () => {
    await withSdkScenario(verifySdkRefusals);
  });
});

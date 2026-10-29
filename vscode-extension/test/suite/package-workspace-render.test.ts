import * as assert from "assert";
import { randomBytes } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { Uri, ViewColumn, window, type WebviewPanel } from "vscode";
import type { Browser, Frame } from "playwright-core";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { SDK_COMPONENT_SOURCE } from "./sdk-renderer-source";
import { renderWorkspace } from "../../client/src/osprey-ui/render";
import { WEBVIEW_SCRIPT } from "../../client/src/osprey-ui/webview-client";
import { workspaceHtml } from "../../client/src/osprey-ui/webview-html";
import { createWorkspaceSession } from "../../client/src/osprey-ui/session";
import { record, type Regions } from "../../client/src/osprey-ui/protocol";
import { BUILD_MS, connectEditorBrowser, editorWebviewFrame, waitFor } from "./profiler-kit";
import { compileSdkWasm, compileSdkWasmJson, runSdkWasmJson, extensionRoot } from "./osprey-test-env";

function regions(root: unknown, packages?: unknown): Regions {
  const map = new Map<string, { revision: number; node: unknown }>();
  map.set("root", { revision: 3, node: root });
  if (packages !== undefined) map.set("packages", { revision: 1, node: packages });
  return map;
}

const screen = {
  kind: "screen", title: "Packages", subtitle: "Your project", toolbar: [],
  children: [{ kind: "slot", key: "packages" }],
};

const rows = {
  kind: "section", title: "Packages", footer: "",
  children: [
    { kind: "field", label: "Search", value: "", prompt: "<script>alert(1)</script>", secure: false, multiline: false, commit: "edit", bind: "Search", event: 0 },
    { kind: "rows", selected: "a", rows: [{ key: "a", symbol: "shippingbox", group: "Installed", child: { kind: "text", text: "<img src=x onerror=alert(1)>", role: "body", tone: "primary" } }], event: 1 },
  ],
};

suite("package workspace renderer", () => {
  test("native split children display both keyed package panes", () => {
    const map = new Map(regions({ kind: "split", children: [{ kind: "slot", key: "packages" }, { kind: "slot", key: "details" }] }, rows));
    map.set("details", { revision: 1, node: { kind: "text", text: "Package details", role: "body", tone: "primary" } });
    const html = renderWorkspace({ regions: map });
    assert.ok(html.includes("Search"), html);
    assert.ok(html.includes("Package details"), html);
    assert.strictEqual((html.match(/class="osp-pane"/g) ?? []).length, 2);
  });

  test("untrusted text and placeholders are escaped while slots render their region", () => {
    const html = renderWorkspace({ regions: regions(screen, rows), assetUri: "vscode-resource://icon.png" });
    assert.ok(!html.includes("<img src=x"), html);
    assert.ok(!html.includes("<script>alert"), html);
    assert.ok(html.includes("&lt;img src=x onerror=alert(1)&gt;"), html);
    assert.ok(html.includes("&lt;script&gt;alert(1)&lt;/script&gt;"), html);
    assert.ok(html.includes('data-region-key="packages"'), html);
    assert.ok(html.includes('data-region-revision="1"'), html);
  });

  test("control provenance names the region, revision and ordinal the framework issued", () => {
    const html = renderWorkspace({ regions: regions(screen, rows) });
    assert.ok(html.includes('data-event-region="packages" data-event-revision="1" data-event-ordinal="0"'), html);
    assert.ok(html.includes('data-event-region="packages" data-event-revision="1" data-event-ordinal="1"'), html);
    assert.ok(html.includes('data-focus="packages:0"'), html);
  });

  test("the application palette becomes sanitized light-dark custom properties", () => {
    const html = renderWorkspace({
      regions: regions({ kind: "text", text: "x", role: "body", tone: "primary" }),
      theme: { tokens: [{ name: "accent", light: "#006B86", dark: "#77D7F4" }, { name: "bad;}", light: "red", dark: "#fff" }] },
    });
    assert.ok(html.includes("--osp-brand-accent: light-dark(#006B86, #77D7F4);"), html);
    assert.ok(!html.includes("bad;}"), html);
  });

  test("an unknown native component is a protocol defect, not a silent blank", () => {
    assert.throws(() => renderWorkspace({ regions: regions({ kind: "mystery" }) }), /Unknown native component/);
  });

  test("malformed or excessive trees fail explicitly instead of rendering a partial screen", () => {
    for (const node of [null, [], {}, { kind: 1 }]) {
      assert.throws(() => renderWorkspace({ regions: regions(node) }), /not an object with a kind/);
    }
    let deep: unknown = { kind: "text", text: "leaf" };
    for (let depth = 0; depth < 66; depth++) deep = { kind: "stack", children: [deep] };
    assert.throws(() => renderWorkspace({ regions: regions(deep) }), /too deep/);
    assert.throws(() => renderWorkspace({ regions: regions(screen, rows), maxNodes: 2 }), /too large/);
  });

  test("invalid control addresses stay inert and malformed list entries cannot create actions", () => {
    for (const event of [undefined, "1", -1, 0.5, Number.MAX_SAFE_INTEGER + 1]) {
      const html = renderWorkspace({ regions: regions({ kind: "field", event }) });
      assert.ok(!html.includes("data-event-region="), html);
    }
    const html = renderWorkspace({ regions: regions({ kind: "stack", children: [
      { kind: "rows", rows: [null] }, { kind: "picker", choices: [null] }, { kind: "tabs", tabs: [null] },
      { kind: "stack", children: "invalid" }, { kind: "alert", visible: false },
      { kind: "text", role: "invalid", tone: "invalid" },
    ] }) });
    assert.ok(!html.includes("<option") && !html.includes("osp-row-button"), html);
  });

  test("a context menu keeps its secondary actions out of the body until opened", () => {
    const menu = { kind: "context-menu", body: { kind: "text", text: "Package", role: "body", tone: "primary" }, items: [{ kind: "button", label: "Add", symbol: "plus", role: "plain", enabled: true, event: 2 }] };
    const html = renderWorkspace({ regions: regions(menu) });
    assert.ok(html.includes('data-context-host'), html);
    assert.ok(html.includes('role="menu" hidden'), html);
    assert.ok(html.includes('data-event-ordinal="2"'), html);
  });

  test("symbols name official Codicons and never emit hand-drawn artwork", () => {
    const html = renderWorkspace({ regions: regions({ kind: "stack", axis: "vertical", children: [
      { kind: "badge", label: "Installed", symbol: "shippingbox", tone: "good" },
      { kind: "icon", symbol: "not-a-symbol" },
    ] }) });
    assert.ok(html.includes('codicon codicon-package'), html);
    assert.ok(html.includes('codicon codicon-info'), html);
    assert.ok(!html.includes("osp-symbol"), html);
    assert.ok(!html.includes("not-a-symbol"), html);
  });

  test("only HTTPS links are active and the rows label is application text", () => {
    const html = renderWorkspace({ regions: regions({ kind: "stack", axis: "vertical", children: [
      { kind: "link", label: "Guide", url: "https://example.test/guide" },
      { kind: "link", label: "Local", url: "http://127.0.0.1/" },
      { kind: "rows", label: "Findings", selected: "", rows: [] },
    ] }) });
    assert.ok(html.includes('href="https://example.test/guide"'), html);
    assert.ok(html.includes("osp-link-disabled"), html);
    assert.ok(html.includes('aria-label="Findings"'), html);
    assert.ok(!html.includes("Package rows"), html);
  });

  test("the document keeps a nonce CSP with no remote connection and a local Codicon font", () => {
    const html = workspaceHtml({ nonce: "abc123", assetUri: "", cspSource: "vscode-resource:" });
    assert.ok(html.includes("script-src 'nonce-abc123'"), html);
    assert.ok(html.includes("connect-src 'none'"), html);
    assert.ok(html.includes("font-src data:"), html);
    assert.ok(html.includes('<script nonce="abc123">'), html);
    assert.ok(!html.includes("default-src *"), html);
  });

  test("the webview delegates real controls, flushes pending edits and restores scroll", () => {
    assert.ok(WEBVIEW_SCRIPT.includes('closest(event.target, "[data-event-region]")'), WEBVIEW_SCRIPT.slice(0, 400));
    assert.ok(WEBVIEW_SCRIPT.includes("flushFields"), WEBVIEW_SCRIPT);
    assert.ok(WEBVIEW_SCRIPT.includes("captureScroll") && WEBVIEW_SCRIPT.includes("restoreScroll"), WEBVIEW_SCRIPT);
    assert.ok(WEBVIEW_SCRIPT.includes("restoreScroll(scroll)"), WEBVIEW_SCRIPT);
    assert.doesNotThrow(() => new Function(WEBVIEW_SCRIPT));
  });

  test("an aside width is clamped to its protocol limits before it reaches the layout", () => {
    const html = renderWorkspace({ regions: regions({ kind: "aside", side: "trailing\"><img src=x>", width: 99999, least: 280, most: 640, event: 3,
      pane: { kind: "text", text: "Pane", role: "body", tone: "primary" },
      aside: { kind: "text", text: "Aside", role: "body", tone: "primary" },
    }) });
    assert.ok(html.includes("--osp-aside-width:640px"), html);
    assert.ok(!html.includes("99999"), html);
    assert.ok(!html.includes("<img src=x>"), html);
    assert.ok(html.includes('data-side="trailing&quot;&gt;&lt;img src=x&gt;"'), html);
    for (const width of [undefined, "bad", 0.5]) {
      const fallback = renderWorkspace({ regions: regions({ kind: "aside", width,
        pane: { kind: "text" }, aside: { kind: "text" }, least: 0, most: 0 }) });
      assert.ok(fallback.includes('data-min="240" data-max="720"'), fallback);
      assert.ok(fallback.includes("--osp-aside-width:240px"), fallback);
    }
  });
});



type SdkMode = "expanded" | "collapsed" | "variants" | "normalized" | "board";
const boardNode = { kind: "board", slice: "sdk-board", count: 0, lasso: false, event: 0, lassoEvent: 1 };
const boardSlices = new Map([["sdk-board", JSON.stringify({ version: 1, strokes: [], notes: "", size: { width: 1536, height: 1024 }, view: { x: 0, y: 0, scale: 100 } })]]);

async function compiledSdkTree(mode: SdkMode): Promise<unknown> {
  return compileSdkWasmJson(SDK_COMPONENT_SOURCE, mode, BUILD_MS);
}

const sdkAsset = (panel: WebviewPanel): string => panel.webview.asWebviewUri(Uri.file(path.join(extensionRoot, "icon.png"))).toString();

function sdkDocument(panel: WebviewPanel): void {
  panel.webview.html = workspaceHtml({ nonce: randomBytes(16).toString("hex"), assetUri: sdkAsset(panel), cspSource: panel.webview.cspSource });
}

function showSdkTree(panel: WebviewPanel, node: unknown, events: unknown[]): void {
  const assetUri = sdkAsset(panel);
  const html = renderWorkspace({ regions: regions(node), assetUri, slices: boardSlices });
  panel.webview.onDidReceiveMessage((raw: unknown) => {
    if (!record(raw)) return;
    if (raw.type === "ready") void panel.webview.postMessage({ type: "render", html, diagnostics: [] });
    if (raw.type === "event") events.push(raw);
  });
  sdkDocument(panel);
}

async function showNormalizedSdk(panel: WebviewPanel, events: unknown[]): Promise<void> {
  const module = await compileSdkWasm(SDK_COMPONENT_SOURCE, BUILD_MS);
  const session = createWorkspaceSession({
    run: async (frame) => ({ exitCode: 0, stdout: JSON.stringify(await runSdkWasmJson(module, frame)) }),
    render: (regions, _diagnostics, theme) => renderWorkspace({ regions, theme, assetUri: sdkAsset(panel) }),
    send: (message) => { events.push(message); void panel.webview.postMessage(message); },
    execute: async () => assert.fail("the normalizing SDK fixture requests no capability"),
    now: () => 1, report: (error) => { events.push({ type: "error", message: error.message }); },
  });
  panel.onDidDispose(() => session.dispose());
  panel.webview.onDidReceiveMessage((raw: unknown) => {
    if (record(raw) && raw.type === "ready") { void session.start({ platform: "vscode" }); return; }
    events.push(raw); session.message(raw);
  });
  sdkDocument(panel);
}

async function assertSdkFields(frame: Frame): Promise<void> {
  assert.strictEqual(await frame.getByRole("switch", { name: "SDK switch" }).getAttribute("aria-checked"), "true");
  assert.strictEqual(await frame.getByRole("combobox", { name: "SDK choice", exact: true }).inputValue(), "a");
  assert.strictEqual(await frame.getByRole("textbox", { name: "SDK notes", exact: true }).inputValue(), "one\ntwo");
  assert.strictEqual(await frame.getByLabel("SDK secret", { exact: true }).getAttribute("type"), "password");
  assert.strictEqual(await frame.getByRole("button", { name: "SDK disabled" }).isDisabled(), true);
}

async function verifySdkEditor(frame: Frame, events: unknown[], panel: WebviewPanel, node: unknown): Promise<void> {
  const field = frame.getByRole("textbox", { name: "SDK argument", exact: true });
  await field.focus();
  await field.evaluate((element) => (element as HTMLTextAreaElement).setSelectionRange(3, 3));
  await waitFor(() => events.length, (count) => count === 1, 3000, "Unicode caret reaches Osprey as a byte offset");
  assert.deepStrictEqual(events[0], { type: "event", region: "root", revision: 3, ordinal: 9, value: 5 });
  await field.fill("p ∧ q\nFrom hq.");
  await waitFor(() => events.length, (count) => count === 2, 3000, "editor text reaches its separate binding");
  assert.ok(record(events[1]) && events[1].ordinal === 8 && events[1].value === "p ∧ q\nFrom hq.");
  const next = new Map(regions(node));
  next.set("root", { revision: 4, node });
  await panel.webview.postMessage({ type: "render", html: renderWorkspace({ regions: next }), diagnostics: [] });
  await frame.locator('textarea[data-native-editor][data-event-revision="4"]').waitFor();
  assert.strictEqual(await field.inputValue(), "p ∧ q\nFrom hq.");
  assert.strictEqual(await frame.locator(".osp-editor-mirror").textContent(), "p ∧ q\nFrom hq.");
}

async function assertSdkComponents(frame: Frame): Promise<void> {
  await assertSdkFields(frame);
  assert.strictEqual(await frame.locator('math[aria-label="x squared over square root of y"] mfrac msup').count(), 1);
  assert.strictEqual(await frame.locator('math mfrac msqrt').count(), 1);
  assert.strictEqual(await frame.getByRole("heading", { name: "SDK vocabulary", exact: true }).count(), 1);
  assert.strictEqual(await frame.getByRole("img", { name: "SDK health" }).locator("span").count(), 3);
  assert.strictEqual(await frame.locator(".osp-spacer").count(), 1);
  assert.strictEqual(await frame.locator("hr.osp-divider").count(), 1);
  assert.strictEqual(await frame.getByRole("heading", { name: "SDK empty" }).count(), 1);
  assert.strictEqual(await frame.getByRole("progressbar", { name: "SDK progress" }).textContent(), "SDK progressCompiling");
  assert.strictEqual(await frame.locator(".osp-swatch").getAttribute("style"), "--osp-swatch-light:#112233;--osp-swatch-dark:#ddeeff");
  assert.strictEqual(await frame.getByRole("img", { name: "Osprey", exact: true }).count(), 2);
  assert.strictEqual(await frame.getByText("SDK evidence", { exact: true }).count(), 1);
  assert.strictEqual(await frame.getByText("SDK label", { exact: true }).count(), 1);
  assert.strictEqual(await frame.getByText("SDK footer", { exact: true }).count(), 1);
  assert.strictEqual(await frame.getByRole("button", { name: "Nested action", includeHidden: true }).isVisible(), true,
    "an expanded SDK disclosure shows its child controls");
}

async function assertSdkEvents(frame: Frame, events: unknown[], expanded = true): Promise<void> {
  await frame.getByRole("switch", { name: "SDK switch" }).click();
  await frame.getByRole("combobox", { name: "SDK choice", exact: true }).selectOption("b");
  await frame.getByRole("button", { name: "SDK details" }).click();
  await assertHostEvents(events, [false, "b", !expanded]);
}

async function assertHostEvents(events: unknown[], values: readonly unknown[]): Promise<void> {
  await waitFor(() => events.length, (count) => count === values.length, 3000, "real SDK controls reach the VS Code host");
  assert.deepStrictEqual(events, values.map((value, ordinal) => ({ type: "event", region: "root", revision: 3, ordinal, value })));
}

async function verifySdkFrame(frame: Frame, events: unknown[]): Promise<void> {
  try {
    await assertSdkComponents(frame);
    await assertSdkEvents(frame, events);
  } catch (error) {
    const evidence = path.resolve(extensionRoot, "../target/sdk-components-e2e");
    fs.mkdirSync(evidence, { recursive: true });
    fs.writeFileSync(path.join(evidence, "browser.json"),
      JSON.stringify({ html: await frame.locator("#osprey-root").innerHTML(), aria: await frame.locator("#osprey-root").ariaSnapshot() }, null, 2));
    throw error;
  }
}

async function closeSdkView(panel: WebviewPanel | undefined, browser: Browser, events: unknown[], mode: SdkMode): Promise<void> {
  panel?.dispose();
  await browser.close();
  const evidence = path.resolve(extensionRoot, "../target/sdk-components-e2e");
  fs.mkdirSync(evidence, { recursive: true });
  fs.writeFileSync(path.join(evidence, `${mode}-receipt.json`),
    JSON.stringify({ profile: process.env.OSPREY_TEST_USER_DATA, events, note: "Execution receipt; consult the test harness for pass/fail." }, null, 2));
}

type SdkBrowserCheck = (frame: Frame, events: unknown[], panel: WebviewPanel, node: unknown) => Promise<void>;

async function withSdkView(mode: SdkMode, run: SdkBrowserCheck): Promise<void> {
  const node = mode === "normalized" ? undefined : mode === "board" ? boardNode : await compiledSdkTree(mode);
  const browser = await connectEditorBrowser();
  const events: unknown[] = [];
  let panel: WebviewPanel | undefined;
  try {
    panel = window.createWebviewPanel("osprey.sdk.probe", "SDK vocabulary", ViewColumn.One,
      { enableScripts: true, localResourceRoots: [Uri.file(extensionRoot)] });
    if (mode === "normalized") await showNormalizedSdk(panel, events);
    else showSdkTree(panel, node, events);
    const frame = await editorWebviewFrame(browser, mode === "normalized" ? 'input[placeholder="Normalized"]' : mode === "board" ? "canvas[data-board-slice]" : '.osp-progress[aria-label="SDK progress"]');
    await run(frame, events, panel, node);
  } finally {
    await closeSdkView(panel, browser, events, mode);
  }
}

async function verifyBoardPointer(frame: Frame, events: unknown[], panel: WebviewPanel): Promise<void> {
  const canvas = frame.locator("canvas[data-board-slice]");
  const box = await canvas.boundingBox();
  assert.ok(box);
  const mouse = frame.page().mouse;
  await mouse.move(box.x + 120, box.y + 140);
  await mouse.down();
  await panel.webview.postMessage({ type: "render", html: renderWorkspace({ regions: new Map([["root", { revision: 4, node: boardNode }]]), slices: boardSlices }), diagnostics: [] });
  await mouse.move(box.x + 360, box.y + 220, { steps: 12 });
  await mouse.up();
  await waitFor(() => events.length, (count) => count >= 1, 3000, "one finished stroke reaches the host");
  assert.strictEqual(events.length, 1, "pointerup must not also dispatch a null canvas click");
  const stroke = events[0];
  assert.ok(record(stroke) && record(stroke.value));
  assert.strictEqual(stroke.revision, 3, "a pending stroke retains its original provenance");
  assert.ok(typeof stroke.edit === "number" && Array.isArray(stroke.value.points) && stroke.value.points.length > 2);
  await verifyOrdinaryBoardStroke(frame, events, box);
}

async function verifyOrdinaryBoardStroke(frame: Frame, events: unknown[], box: { x: number; y: number }): Promise<void> {
  const mouse = frame.page().mouse;
  await mouse.move(box.x + 160, box.y + 180);
  await mouse.down();
  await mouse.move(box.x + 320, box.y + 240, { steps: 5 });
  await mouse.up();
  await frame.evaluate(() => new Promise<void>((resolve) => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  assert.strictEqual(events.length, 2, "ordinary pointerup must not dispatch a second null canvas action");
}

suite("native board browser", () => {
  test("pointer drawing survives a background render and dispatches one complete stroke", async () => {
    await withSdkView("board", verifyBoardPointer);
  });
});

async function assertSdkVariants(frame: Frame, events: unknown[]): Promise<void> {
  const aside = frame.locator(".osp-aside");
  assert.strictEqual(await aside.getAttribute("class"), "osp-aside osp-side-leading");
  assert.strictEqual(await aside.getAttribute("style"), "--osp-aside-width:640px");
  assert.deepStrictEqual(await aside.locator(":scope > div").evaluateAll((nodes) => nodes.map((node) => node.className)),
    ["osp-aside-column", "osp-splitter", "osp-aside-pane"]);
  assert.strictEqual(await frame.locator(".osp-row .codicon, .osp-row img, .osp-row script").count(), 0);
  assert.strictEqual(await frame.locator(".osp-swatch").getAttribute("style"), "--osp-swatch-light:transparent;--osp-swatch-dark:transparent");
  assert.strictEqual(await frame.locator(".osp-section-title, .osp-section-footer").count(), 0);
  assert.strictEqual(await frame.locator('[data-region-slot="root"]').count(), 1);
  await frame.getByRole("switch", { name: "SDK off" }).click();
  await frame.getByRole("option", { name: "<untrusted>", exact: true }).click();
  await frame.locator(".osp-splitter").press("ArrowLeft");
  assert.strictEqual(await aside.evaluate((node) => (node as HTMLElement).style.getPropertyValue("--osp-aside-width")), "620px");
  await assertHostEvents(events, [true, "plain", 620]);
}

async function verifyWideSdkVariants(frame: Frame, events: unknown[]): Promise<void> {
  const page = frame.page();
  const size = await page.evaluate(() => ({ width: innerWidth, height: innerHeight }));
  try {
    await page.setViewportSize({ width: 1800, height: 1000 });
    await assertSdkVariants(frame, events);
  } finally {
    await page.setViewportSize(size);
  }
}

async function assertInFlightEdit(frame: Frame, events: unknown[], panel: WebviewPanel, node: unknown): Promise<void> {
  await frame.getByRole("textbox", { name: "SDK draft", exact: true }).fill("draft");
  await waitFor(() => events.length, (count) => count === 1, 3000, "the original edit reaches the host");
  const next = new Map(regions(node));
  next.set("root", { revision: 4, node });
  await panel.webview.postMessage({ type: "render", html: renderWorkspace({ regions: next }), diagnostics: [] });
  await frame.locator('input[placeholder="Draft"][data-event-revision="4"]').waitFor();
  assert.deepStrictEqual(events.map((event) => record(event) ? [event.revision, event.value] : event), [[3, "draft"]]);
  assert.strictEqual(await frame.getByRole("textbox", { name: "SDK draft", exact: true }).inputValue(), "draft");
  await assertNewerDraft(frame, events, panel, node);
}

async function assertNewerDraft(frame: Frame, events: unknown[], panel: WebviewPanel, node: unknown): Promise<void> {
  const next = new Map(regions(node));
  await frame.getByRole("textbox", { name: "SDK draft", exact: true }).fill("newer");
  assert.ok(record(events[0]));
  await panel.webview.postMessage({ type: "edit-result", edit: events[0].edit, accepted: true });
  next.set("root", { revision: 5, node });
  await panel.webview.postMessage({ type: "render", html: renderWorkspace({ regions: next }), diagnostics: [] });
  await waitFor(() => events.length, (count) => count === 2, 3000, "an older receipt cannot discard a newer edit");
  assert.deepStrictEqual(events.map((event) => record(event) ? [event.revision, event.value] : event), [[3, "draft"], [4, "newer"]]);
  assert.strictEqual(await frame.getByRole("textbox", { name: "SDK draft", exact: true }).inputValue(), "newer");
}

async function verifyNormalizedSdk(frame: Frame, events: unknown[]): Promise<void> {
  const field = frame.getByRole("textbox", { name: "SDK normalized", exact: true });
  await field.fill("draft");
  await pollUntilResult({ fn: () => field.inputValue(), until: (value) => value === "CANONICAL", timeoutMs: 3000,
    description: "the actual Osprey update normalizes the text" });
  await frame.waitForTimeout(400);
  const messages = events.filter(record);
  assert.strictEqual(messages.filter((message) => message.type === "event").length, 1, "a normalized edit is delivered once");
  assert.strictEqual(messages.filter((message) => message.type === "edit-result" && message.accepted === true).length, 1);
  assert.strictEqual(messages.filter((message) => message.type === "error" || message.type === "fatal").length, 0);
  assert.strictEqual(await field.inputValue(), "CANONICAL");
}

async function verifyRemovedSdkField(frame: Frame, events: unknown[], panel: WebviewPanel): Promise<void> {
  await frame.evaluate(() => {
    const field = document.querySelector<HTMLInputElement>('input[placeholder="Normalized"]');
    const hide = Array.from(document.querySelectorAll("button")).find((button) => button.textContent?.includes("Hide field"));
    if (!field || !hide) throw new Error("native SDK field and removal action must exist");
    hide.click(); field.value = "draft"; field.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await frame.getByText("SDK removed", { exact: true }).waitFor();
  await waitFor(() => events.filter(record).filter((message) => message.type === "edit-result" && message.accepted === false).length,
    (count) => count === 1, 3000, "the Osprey framework refuses the removed binding");
  const latest = events.filter(record).filter((message) => message.type === "render").at(-1);
  assert.ok(latest);
  await panel.webview.postMessage({ ...latest, diagnostics: ["Registry returned HTTP503"] });
  await pollUntilResult({ fn: () => frame.locator("#osprey-diagnostics").textContent(), until: (text) => Boolean(text?.includes("HTTP503")),
    timeoutMs: 3000, description: "later host failures remain visible alongside the edit refusal" });
  assert.ok(await frame.locator("#osprey-diagnostics").isVisible(), "a removed edit reports an explicit error");
  assert.ok((await frame.locator("#osprey-diagnostics").textContent() ?? "").length > "Registry returned HTTP503".length);
}

async function assertSubmitOnce(frame: Frame, events: unknown[]): Promise<void> {
  const field = frame.getByLabel("SDK secret", { exact: true });
  const barrier = frame.getByRole("button", { name: "SDK details" });
  await field.fill("submitted");
  await field.press("Enter");
  await barrier.click();
  await waitFor(() => events.filter(record).filter((event) => event.value === false).length,
    (count) => count === 1, 3000, "the following button event confirms delivery of prior field events");
  assert.deepStrictEqual(events.filter(record).map((event) => event.value), ["submitted", false]);
  await field.fill("edited");
  await barrier.click();
  await waitFor(() => events.filter(record).filter((event) => event.value === false).length,
    (count) => count === 2, 3000, "a later edit still commits on blur");
  assert.deepStrictEqual(events.filter(record).map((event) => event.value), ["submitted", false, "edited", false]);
  await field.press("Enter");
  await field.press("Enter");
  await barrier.click();
  await waitFor(() => events.length, (count) => count === 7, 3000, "intentional repeated Enter submits still execute");
  assert.deepStrictEqual(events.filter(record).map((event) => event.value), ["submitted", false, "edited", false, "edited", "edited", false]);
}

suite("compiled SDK renderer E2E", () => {
  test("a marked Osprey editor retains Unicode caret provenance and pending working", async () => {
    await withSdkView("expanded", verifySdkEditor);
  });
  test("submitting a field with Enter commits once when it subsequently blurs", async () => {
    await withSdkView("expanded", assertSubmitOnce);
  });
  test("an edit whose field disappears is reported, never silently dropped", async () => {
    await withSdkView("normalized", verifyRemovedSdkField);
  });
  test("a real Osprey SDK handler can normalize an edit without a replay loop", async () => {
    await withSdkView("normalized", verifyNormalizedSdk);
  });
  test("an in-flight SDK edit survives a render until its host receipt", async () => {
    await withSdkView("expanded", assertInFlightEdit);
  });
  test("native SDK WASM components draw and deliver real webview events", async () => {
    await withSdkView("expanded", verifySdkFrame);
  });
  test("native SDK leading layout sanitizes values and delivers keyboard resizing", async () => {
    await withSdkView("variants", verifyWideSdkVariants);
  });
  test("a collapsed native SDK disclosure hides its child controls", async () => {
    await withSdkView("collapsed", async (frame, events) => {
      assert.strictEqual(await frame.getByRole("button", { name: "Nested action", includeHidden: true }).isVisible(), false,
        "a collapsed SDK disclosure must hide its child controls");
      assert.strictEqual(await frame.getByRole("button", { name: "Nested action" }).count(), 0,
        "collapsed child controls must be absent from the accessibility tree");
      await assertSdkEvents(frame, events, false);
    });
  });
});

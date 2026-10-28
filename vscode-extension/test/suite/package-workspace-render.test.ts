import * as assert from "assert";
import { randomBytes } from "node:crypto";
import * as fs from "node:fs";
import * as path from "node:path";
import { Uri, ViewColumn, window, type WebviewPanel } from "vscode";
import type { Browser, Frame } from "playwright-core";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { RENDERED_KINDS, renderWorkspace } from "../../client/src/osprey-ui/render";
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
    { kind: "rows", selected: "a", rows: [{ key: "a", symbol: "package", group: "Installed", child: { kind: "text", text: "<img src=x onerror=alert(1)>", role: "body", tone: "primary" } }], event: 1 },
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

  test("every framework node kind is drawn here except scenes, which the webview refuses", () => {
    // Every `Node` constructor in packages/native/src/node.ospml, as the encoder names it ([NATIVE-HOSTS-COVERAGE]),
    // so a component added to the framework fails here until the webview draws it or refuses it by name.
    const source = fs.readFileSync(path.join(extensionRoot, "..", "packages", "native", "src", "node.ospml"), "utf8").replace(/\r/g, "");
    const block = source.slice(source.indexOf("type Node A ="));
    const body = block.slice(0, block.search(/\n\S/));
    const kinds = [...body.matchAll(/^ {4}([A-Z]\w*)$/gm)].map((match) => match[1].replace(/(?<!^)([A-Z])/g, "-$1").toLowerCase());
    assert.ok(kinds.includes("meter") && kinds.includes("context-menu"), kinds.join(","));
    assert.deepStrictEqual([...RENDERED_KINDS, "scene"].sort(), [...kinds].sort());
    assert.throws(() => renderWorkspace({ regions: regions({ kind: "scene" }) }), /Unknown native component: scene/);
  });

  test("a meter fills its share of the total, clamped, in its tone", () => {
    const html = renderWorkspace({ regions: regions({ kind: "stack", axis: "vertical", children: [
      { kind: "meter", label: "30 of 120 used", value: 30, total: 120, tone: "caution" },
      { kind: "meter", label: "<over>", value: 500, total: 120, tone: "nonsense" },
    ] }) });
    assert.ok(html.includes('class="osp-meter osp-tone-caution" role="meter" aria-valuemin="0" aria-valuemax="120" aria-valuenow="30" aria-label="30 of 120 used"'), html);
    assert.ok(html.includes('style="width:25.0%"'), html);
    assert.ok(html.includes('class="osp-meter osp-tone-accent" role="meter" aria-valuemin="0" aria-valuemax="120" aria-valuenow="120" aria-label="&lt;over&gt;"'), html);
    assert.ok(html.includes('style="width:100.0%"'), html);
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
    const menu = { kind: "context-menu", body: { kind: "text", text: "Package", role: "body", tone: "primary" }, items: [{ kind: "button", label: "Add", symbol: "add", role: "plain", enabled: true, event: 2 }] };
    const html = renderWorkspace({ regions: regions(menu) });
    assert.ok(html.includes('data-context-host'), html);
    assert.ok(html.includes('role="menu" hidden'), html);
    assert.ok(html.includes('data-event-ordinal="2"'), html);
  });

  test("symbols name official Codicons and never emit hand-drawn artwork", () => {
    const html = renderWorkspace({ regions: regions({ kind: "stack", axis: "vertical", children: [
      { kind: "badge", label: "Installed", symbol: "package", tone: "good" },
      { kind: "icon", symbol: "not-a-symbol" },
    ] }) });
    assert.ok(html.includes('codicon codicon-package'), html);
    assert.ok(html.includes('codicon codicon-info'), html);
    assert.ok(!html.includes("osp-symbol"), html);
    assert.ok(!html.includes("not-a-symbol"), html);
  });

  test("every framework symbol draws its own Codicon and a pair shows its label and value", () => {
    // Every `Symbols::name` in packages/native/src/symbol.ospml ([UI-COMPONENTS-SYMBOLS]).
    const names = ["add", "delete", "dismiss", "check", "warning", "info", "question", "search", "settings", "star", "play", "pause", "stop", "refresh", "folder", "document", "package", "inbox", "globe", "external", "lock", "shield", "branch", "tag", "clock", "person", "cursor", "chevron"];
    for (const name of names) {
      const html = renderWorkspace({ regions: regions({ kind: "icon", symbol: name }) });
      assert.ok(name === "info" || !html.includes("codicon-info\""), `${name} fell back to the info mark`);
    }
    const html = renderWorkspace({ regions: regions({ kind: "pair", label: "Digest", value: "<sha>" }) });
    assert.ok(html.includes('<span class="osp-pair-label">Digest</span><span class="osp-pair-value">&lt;sha&gt;</span>'), html);
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

const SDK_COMPONENT_SOURCE = String.raw`
namespace sdk_renderer_probe
import ui::Ui
import ui::Encode
import ui::Doc
import ui::Runtime
import ui::Keyed
import ui::Messages
import vscode::Webview

type Act = Changed | Gone

controls expanded = [
    Ui::toggle ("SDK switch", true, Changed),
    Ui::picker ("SDK choice", [Ui::choice ("a", "Alpha"), Ui::choice ("b", "Beta")], "a", Changed),
    Ui::disclosure ("SDK details", expanded, Changed, [Ui::button ("Nested action", ui::Add, Changed)]),
    ui::Field(label = "SDK notes", value = "one\ntwo", prompt = "Notes", secure = false, multiline = true, commit = ui::OnSubmit, bind = Changed),
    ui::Field(label = "SDK secret", value = "private", prompt = "Password", secure = true, multiline = false, commit = ui::OnSubmit, bind = Changed),
    ui::Button(label = "SDK disabled", symbol = ui::Lock, role = ui::Destructive, enabled = false, action = Changed),
    ui::Field(label = "SDK draft", value = "", prompt = "Draft", secure = false, multiline = false, commit = ui::OnEdit, bind = Changed),
]

proof () = [
    Ui::pair ("SDK pair", "37 lines"),
    Ui::label ("SDK label", ui::Info),
    ui::Spacer,
    ui::Divider,
    ui::Segments(tones = [ui::Good, ui::Caution, ui::Danger], label = "SDK health"),
    Ui::empty (ui::Inbox, "SDK empty", "No items exist"),
    ui::Progress(label = "SDK progress", detail = "Compiling"),
    ui::Swatch(name = "SDK palette", light = "#112233", dark = "#ddeeff"),
]

drawing expanded = Ui::screen ("SDK vocabulary", "Compiled from Osprey", [], [Ui::layer [Ui::section ("SDK proof", "SDK footer", listConcat (controls expanded) (proof ())) ]])

variants () = Ui::aside (Ui::column [
    Ui::section ("", "", []),
    Ui::toggle ("SDK off", false, Changed),
    ui::Swatch(name = "SDK invalid palette", light = "red", dark = "url(https://example.invalid)"),
    ui::Progress(label = "SDK progress", detail = "Safe values"),
    Ui::slot "root",
], ui::Leading, Ui::rows ([Ui::item ("plain", Ui::text "<untrusted>")], "plain", Changed), 9999, Changed)

tree mode = match mode
    "variants" => variants ()
    _ => drawing (mode == "expanded")

(** A real SDK application that canonicalizes input and can withdraw its field. *)
module Normalized
    slice () = perform Signal<string>.read ()
    drawing () =
        value = perform Signal<string>.read ()
        match value == "removed"
            true => Ui::text "SDK removed"
            false => Ui::column [Ui::field ("SDK normalized", value, "Normalized", Changed), Ui::button ("Hide field", ui::Dismiss, Gone)]
    regions (before, value, frames, _) =
        handle static Signal<string>
            read => value
        changed = signalChanges before value (\text => JsonString text) (signalDependencies slice)
        [Keyed::region (frames, "root", changed, drawing)]
    initial _ = Runtime::step ""
    update (_, current, message) = Messages::route (message, \_ => Runtime::step "removed", \(_, _) => Runtime::step "CANONICAL", \(_, _) => Runtime::step current, \(_, _) => Runtime::step current, \(_, _) => Runtime::step current, \_ => Runtime::step current, \_ => Runtime::step current, \(_, _) => Runtime::step current)
    save value = JsonString value
    restore doc = jsonText doc ?: ""
    palette = ui::Theme(tokens = [], spacing = [4], feedback = 160, settle = 220)
    (** Start the field application through the framework. *)
    export start context = Runtime::start (context, initial, regions, save, palette)
    (** Deliver a typed event through the framework. *)
    export dispatch (saved, event) = Runtime::dispatch (saved, event, update, regions, save, restore)

main () =
    source = input ()
    match Doc::has (Doc::parse source, "operation")
        true => print (Webview::respond (source, Normalized::start, Normalized::dispatch))
        false => print (Doc::write (listGet (Encode::walk ([], tree source)).nodes 0 ?: JsonNull))
`;

type SdkMode = "expanded" | "collapsed" | "variants" | "normalized";

async function compiledSdkTree(mode: SdkMode): Promise<unknown> {
  return compileSdkWasmJson(SDK_COMPONENT_SOURCE, mode, BUILD_MS);
}

const sdkAsset = (panel: WebviewPanel): string => panel.webview.asWebviewUri(Uri.file(path.join(extensionRoot, "icon.png"))).toString();

function sdkDocument(panel: WebviewPanel): void {
  panel.webview.html = workspaceHtml({ nonce: randomBytes(16).toString("hex"), assetUri: sdkAsset(panel), cspSource: panel.webview.cspSource });
}

function showSdkTree(panel: WebviewPanel, node: unknown, events: unknown[]): void {
  const assetUri = sdkAsset(panel);
  const html = renderWorkspace({ regions: regions(node), assetUri });
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

async function assertSdkComponents(frame: Frame): Promise<void> {
  await assertSdkFields(frame);
  assert.strictEqual(await frame.getByRole("heading", { name: "SDK vocabulary", exact: true }).count(), 1);
  assert.strictEqual(await frame.getByRole("img", { name: "SDK health" }).locator("span").count(), 3);
  assert.strictEqual(await frame.locator(".osp-spacer").count(), 1);
  assert.strictEqual(await frame.locator("hr.osp-divider").count(), 1);
  assert.strictEqual(await frame.getByRole("heading", { name: "SDK empty" }).count(), 1);
  assert.strictEqual(await frame.getByRole("progressbar", { name: "SDK progress" }).textContent(), "SDK progressCompiling");
  assert.strictEqual(await frame.locator(".osp-swatch").getAttribute("style"), "--osp-swatch-light:#112233;--osp-swatch-dark:#ddeeff");
  assert.strictEqual(await frame.getByRole("img", { name: "Osprey", exact: true }).count(), 1);
  assert.strictEqual(await frame.getByText("SDK pair", { exact: true }).count(), 1);
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
  const node = mode === "normalized" ? undefined : await compiledSdkTree(mode);
  const browser = await connectEditorBrowser();
  const events: unknown[] = [];
  let panel: WebviewPanel | undefined;
  try {
    panel = window.createWebviewPanel("osprey.sdk.probe", "SDK vocabulary", ViewColumn.One,
      { enableScripts: true, localResourceRoots: [Uri.file(extensionRoot)] });
    if (mode === "normalized") await showNormalizedSdk(panel, events);
    else showSdkTree(panel, node, events);
    const frame = await editorWebviewFrame(browser, mode === "normalized" ? 'input[placeholder="Normalized"]' : '.osp-progress[aria-label="SDK progress"]');
    await run(frame, events, panel, node);
  } finally {
    await closeSdkView(panel, browser, events, mode);
  }
}

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

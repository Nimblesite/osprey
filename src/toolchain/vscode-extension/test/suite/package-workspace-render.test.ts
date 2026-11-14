// [NATIVE-SHM-VIEW] [GPUI-HOST-RENDER] [UI-COMPONENTS]: the webview draws a checked frame as
// DOM - plain view rows through the row view, and the component tree the Node renderer told from
// told rows ([NATIVE-SHM-NODES]). Every kind the application can publish is drawn, every style
// value comes from the row or the component, and the document that loads the webview grants it
// only this extension's own script.
import * as assert from "assert";
import * as fs from "node:fs";
import * as path from "node:path";
import { RENDERED_KINDS, renderWorkspace, type RenderOptions } from "../../client/src/osprey-ui/render";
import { renderShown } from "../../client/src/osprey-ui/webview/render";
import { readFrame } from "../../client/src/osprey-ui/webview/rows";
import { workspaceHtml } from "../../client/src/osprey-ui/webview-html";
import { frame, type ControlSpec } from "./native-rows-kit";

// The compiled test lives at <ext>/out/test/suite, so the extension root is three levels up.
const extensionRoot = path.resolve(__dirname, "..", "..", "..");

function page(views: readonly ControlSpec[]): string {
  return renderShown(readFrame(frame({ views })));
}

function regions(root: unknown, packages?: unknown): RenderOptions["regions"] {
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
    { kind: "field", label: "Search", value: "", prompt: "<script>alert(1)</script>", secure: false, multiline: false, commit: "edit", event: 0 },
    { kind: "rows", selected: "a", rows: [{ key: "a", symbol: "package", group: "Installed", child: { kind: "text", text: "<img src=x onerror=alert(1)>", role: "body", tone: "primary" } }], event: 1 },
  ],
};

suite("native row rendering", () => {
  test("every published control kind is drawn, and a choice draws nothing", () => {
    const kinds = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    for (const kind of kinds) {
      const drawn = page([{ kind, action: 1, words: "words", hint: "search", width: 10, height: 10 }]);
      if (kind === 5) assert.ok(drawn.includes("flex-grow:1"), "a gap draws its flexible space");
      else if (kind === 10) assert.ok(!drawn.includes("hidden"), "a choice draws no element of its own");
      else if (kind === 13) assert.ok(drawn.includes('<canvas class="osp-canvas"'), `a canvas is drawn: ${drawn}`);
      else assert.ok(drawn.includes("osp-view-control"), `kind ${kind} is drawn: ${drawn}`);
    }
    assert.ok(!page([{ kind: 10, words: "hidden" }]).includes("hidden"), "a choice never leaks its words");
  });

  test("a nested page keeps drawing order, parents and fixed extents", () => {
    const drawn = page([
      { kind: 0, words: "Page", fill: 0x101010ff, padding: 20 },
      { kind: 2, parent: 0, words: "Title", size: 32, weight: 800 },
      { kind: 1, parent: 0, spacing: 10, grow: 1 },
      { kind: 3, parent: 2, action: 0, words: "One", width: 100, height: 40 },
      { kind: 3, parent: 2, action: 1, words: "Two" },
    ]);
    assert.ok(drawn.indexOf("Title") < drawn.indexOf("One"), drawn);
    assert.ok(drawn.indexOf("One") < drawn.indexOf("Two"), drawn);
    assert.strictEqual((drawn.match(/data-ordinal="[01]" data-words=""/g) ?? []).length, 2);
    assert.ok(drawn.includes("width:100px"), drawn);
    assert.ok(drawn.includes("height:40px"), drawn);
    assert.ok(drawn.includes("flex-grow:1"), drawn);
    assert.ok(drawn.includes("background:#101010ff"), drawn);
  });

  test("image rows use only host-approved asset URIs", () => {
    const controls: readonly ControlSpec[] = [{ kind: 12, words: "media/logo.png", width: 40 }];
    const missing = renderShown(readFrame(frame({ views: controls })));
    assert.ok(missing.includes("osp-view-image-missing"), missing);
    assert.ok(!missing.includes('src="media/logo.png"'), "an unresolved path is never a src");
    const approved = renderShown(readFrame(frame({ views: controls })), { assets: new Map([["media/logo.png", "vscode-resource://approved/logo.png"]]) });
    assert.ok(approved.includes('src="vscode-resource://approved/logo.png"'), approved);
    assert.ok(!approved.includes("osp-view-image-missing"), approved);
  });

  test("a canvas draws a scene surface with its control number and focus identity", () => {
    const drawn = renderShown(readFrame(frame({ views: [{ kind: 13, action: 3 }],
      scenes: [{ code: 20, index: 0, x: 400, y: 300, width: 1 }, { code: 0, x: 100, y: 100, width: 200, height: 100, colour: 0x336699ff }] })));
    assert.ok(drawn.includes('<canvas class="osp-canvas"'), drawn);
    assert.ok(drawn.includes('data-scene="0"'), drawn);
    assert.ok(drawn.includes('data-ordinal="3"'), drawn);
    assert.ok(drawn.includes("min-height:320px"), drawn);
    assert.ok(drawn.includes('role="img"'), drawn);
  });

  test("a sheet leads with a dismissal that delivers its control number", () => {
    const drawn = page([{ kind: 16, action: 8, words: "Details" }, { kind: 2, parent: 0, words: "Body" }]);
    assert.ok(drawn.includes('data-ordinal="8" data-words=""'), drawn);
    assert.ok(drawn.includes('aria-label="Dismiss"'), drawn);
    assert.ok(drawn.indexOf("Dismiss") < drawn.indexOf("Body"), drawn);
  });

  test("the webview document loads only this extension's script and escapes its values", () => {
    const html = workspaceHtml({ nonce: "n0nce", assetUri: "vscode-resource://icon.png",
      scriptUri: "vscode-resource://out/webview/webview/main.js?x=<script>", cspSource: "vscode-resource:",
      loadingText: "<script>alert(1)</script>" });
    assert.ok(html.includes("script-src vscode-resource: 'nonce-n0nce'"), html);
    assert.ok(html.includes('<script type="module" nonce="n0nce"'), html);
    assert.ok(html.includes("out/webview/webview/main.js?x=&lt;script&gt;"), html);
    assert.ok(!html.includes("<script>alert(1)</script>"), html);
    assert.ok(html.includes("&lt;script&gt;alert(1)&lt;/script&gt;"), html);
    assert.ok(html.includes("connect-src 'none'"), html);
    assert.ok(html.includes("default-src 'none'"), html);
    assert.ok(html.includes("font-src vscode-resource: data:"), "only approved local resources and embedded fonts are allowed");
    assert.ok(html.includes(".codicon-add:before"), "the document carries the official Codicon font this renderer draws");
  });

  test("a refused frame leaves the last good drawing untouched", () => {
    const good = readFrame(frame({ views: [{ kind: 0 }, { kind: 2, parent: 0, words: "Kept" }] }));
    const refused = (): string => renderShown(readFrame({ shapes: new ArrayBuffer(0), views: new ArrayBuffer(7), text: new ArrayBuffer(0) }));
    assert.ok(renderShown(good).includes("Kept"));
    assert.throws(refused, /whole number of rows/);
  });
});


suite("package workspace component renderer", () => {
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
    const packages = html.slice(html.indexOf('data-region-key="packages"'));
    assert.ok(packages.startsWith('data-region-key="packages" data-region-revision="1"'), html);
    assert.ok(packages.includes('data-event-ordinal="0"') && packages.includes('data-event-ordinal="1"'), html);
    assert.ok(html.includes('data-focus="packages:0"'), html);
    assert.ok(html.includes('data-focus="packages:1"'), html);
    assert.ok(html.includes('data-region-key="root" data-region-revision="3"'), html);
  });

  test("control provenance names the region, revision and ordinal the framework issued (extra: a later revision is named and focus stays per region)", () => {
    const map = regions(screen, rows) as Map<string, { revision: number; node: unknown }>;
    map.set("packages", { revision: 9, node: rows });
    const html = renderWorkspace({ regions: map });
    assert.ok(html.includes('data-region-key="packages" data-region-revision="9"'), html);
    assert.ok(!html.includes('data-region-revision="1"'), html);
    assert.strictEqual(html.match(/data-focus="packages:\d+"/g)?.length, 2, html);
    assert.ok(!html.includes('data-focus="root:'), "the slot's controls belong to the region that told them");
  });

  test("the framework brand renders the approved local logo", () => {
    // The brand is the screen header's lockup now that it is no longer a node of its own.
    const html = renderWorkspace({ regions: regions(screen, rows), assetUri: "vscode-resource://approved-logo.png" });
    assert.ok(html.includes('<div class="osp-brand-lockup"><img class="osp-brand" src="vscode-resource://approved-logo.png" alt="Osprey" />'), html);
    assert.ok(renderWorkspace({ regions: regions(screen, rows) }).includes('<span class="osp-brand osp-brand-word">Osprey</span>'));
    assert.throws(() => renderWorkspace({ regions: regions({ kind: "brand" }) }), /Unknown native component: brand/);
  });

  test("the framework brand renders the approved local logo (extra: a hostile logo URI is escaped)", () => {
    const html = renderWorkspace({ regions: regions(screen, rows), assetUri: 'x" onerror="alert(1)' });
    assert.ok(html.includes('src="x&quot; onerror=&quot;alert(1)"'), html);
    assert.ok(!html.includes('onerror="alert(1)"'), html);
    assert.strictEqual(html.match(/class="osp-brand"/g)?.length, 1, "one lockup per screen");
  });

  test("the document keeps a nonce CSP with no remote connection and a local Codicon font", () => {
    const html = workspaceHtml({ nonce: "abc123", assetUri: "", scriptUri: "vscode-resource://main.js", cspSource: "vscode-resource:" });
    assert.ok(html.includes("script-src vscode-resource: 'nonce-abc123'"), html);
    assert.ok(html.includes("connect-src 'none'"), html);
    assert.ok(html.includes("font-src vscode-resource: data:"), "only approved local resources and embedded fonts are allowed");
    assert.ok(!html.includes("font-src *") && !html.includes("font-src https:"));
    assert.ok(html.includes('nonce="abc123"'), html);
    assert.ok(!html.includes("default-src *") && html.includes("default-src 'none'"), html);
    assert.ok(html.includes(".codicon-add:before"), "the Codicon font is embedded, never fetched");
  });

  test("the document keeps a nonce CSP with no remote connection and a local Codicon font (extra: every source list names only local resources)", () => {
    const html = workspaceHtml({ nonce: "n", assetUri: "", scriptUri: "vscode-resource://main.js", cspSource: "vscode-resource:" });
    const policy = html.match(/Content-Security-Policy" content="([^"]*)"/)?.[1] ?? "";
    for (const directive of policy.split(";").map((item) => item.trim()).filter(Boolean)) {
      const sources = directive.split(/\s+/).slice(1);
      for (const source of sources) assert.ok(["'none'", "vscode-resource:", "data:", "blob:", "'unsafe-inline'", "'nonce-n'"].includes(source), `${directive}: ${source}`);
    }
    assert.ok(!/https?:/.test(policy), policy);
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

  test("every framework node kind is drawn here, scenes as the canvas the frame's scene rows name", () => {
    // Every `Node` constructor in src/osprey/packages/native/src/node.ospml, as the walk names it
    // ([NATIVE-HOSTS-COVERAGE]), so a component added to the framework fails here until the
    // webview draws it or refuses it by name.
    const source = fs.readFileSync(path.join(extensionRoot, "..", "..", "osprey", "packages", "native", "src", "node.ospml"), "utf8").replace(/\r/g, "");
    const block = source.slice(source.indexOf("type Node A ="));
    const body = block.slice(0, block.search(/\n\S/));
    const kinds = [...body.matchAll(/^ {4}([A-Z]\w*)$/gm)].map((match) =>
      match[1] === "Typeset" ? "math" : match[1].replace(/(?<!^)([A-Z])/g, "-$1").toLowerCase());
    assert.ok(kinds.includes("meter") && kinds.includes("context-menu") && kinds.includes("scene"), kinds.join(","));
    assert.deepStrictEqual([...RENDERED_KINDS].sort(), [...kinds].sort());
    const scene = renderWorkspace({ regions: regions({ kind: "scene", width: 400, height: 300, zoom: 0, event: 7 }) });
    assert.ok(scene.includes('<canvas class="osp-canvas"'), scene);
    assert.ok(scene.includes('data-ordinal="7"'), scene);
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
      assert.ok(!html.includes("data-event-ordinal="), html);
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
    assert.ok(html.includes("codicon codicon-package"), html);
    assert.ok(html.includes("codicon codicon-info"), html);
    assert.ok(!html.includes("not-a-symbol"), html);
  });

  test("every framework symbol draws its own Codicon and a pair shows its label and value", () => {
    // Every `Symbols::name` in src/osprey/packages/native/src/symbol.ospml ([UI-COMPONENTS-SYMBOLS]).
    const names = ["add", "delete", "dismiss", "check", "warning", "info", "question", "search", "settings", "star", "play", "pause", "stop", "refresh", "folder", "document", "package", "inbox", "globe", "external", "lock", "shield", "branch", "tag", "clock", "person", "cursor", "chevron", "send", "terminal", "edit", "panel", "sparkle"];
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

  test("the webview delegates real controls, flushes pending edits and restores scroll", () => {
    const source = fs.readFileSync(path.join(extensionRoot, "out", "webview", "webview", "client.js"), "utf8");
    assert.ok(source.includes('closest(event.target, "[data-event-ordinal], [data-ordinal]")'), source.slice(0, 400));
    assert.ok(source.includes("flushFields"), source);
    assert.ok(source.includes("captureScroll") && source.includes("restoreScroll"), source);
    assert.ok(source.includes("restoreScroll(scroll)"), source);
  });

  test("the webview delegates real controls, flushes pending edits and restores scroll (extra: the bundled client parses and names no remote origin)", () => {
    const source = fs.readFileSync(path.join(extensionRoot, "out", "webview", "webview", "client.js"), "utf8");
    assert.ok(!/fetch\(|XMLHttpRequest|WebSocket|https?:\/\//.test(source), "the webview client never reaches the network");
    assert.ok(source.includes("export"), "the client is a module the document's nonce script imports");
  });
});

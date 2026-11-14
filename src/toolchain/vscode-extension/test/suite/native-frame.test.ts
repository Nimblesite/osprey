// [NATIVE-SHM-VIEW] [NATIVE-SHM-SCENE] [NATIVE-SHM-ASK]: a frame's rows are checked like the
// Rust host's rows.rs and drawn with every style value the row carries. A bad row refuses the
// whole frame; nothing from the application is guessed.
import * as assert from "node:assert/strict";
import { readAsks } from "../../client/src/osprey-ui/asks";
import { keyName, scenePoint, sceneTransform } from "../../client/src/osprey-ui/webview/events";
import { sameTree, toldFrame } from "../../client/src/osprey-ui/nodes";
import { wordsOf } from "../../client/src/osprey-ui/webview/events";
import { renderShown } from "../../client/src/osprey-ui/webview/render";
import { renderWorkspace } from "../../client/src/osprey-ui/render";
import { FLAGS, readFrame, textAt, type Shown } from "../../client/src/osprey-ui/webview/rows";
import { frame, TextRun, viewRows, type ControlSpec } from "./native-rows-kit";

function shown(options: Parameters<typeof frame>[0]): Shown {
  return readFrame(frame(options));
}

function html(options: Parameters<typeof frame>[0]): string {
  return renderShown(shown(options));
}

suite("native shared rows", () => {
  test("a column applies the row's fill, ink, padding, spacing, radius and extent", () => { const page = html({ views: [{ kind: 0, fill: 0x112233ff, ink: 0xffffffff, padding: 12, spacing: 7,
      radius: 5, width: 430, grow: 1, border: 0x445566ff }, { kind: 2, parent: 0, words: "Page" }] });
    for (const declaration of ["background:#112233ff", "color:#ffffffff", "padding:12px", "gap:7px", "border-radius:5px", "width:430px", "flex-grow:1", "border:1px solid #445566ff", "display:flex", "flex-direction:column"]) {
      assert.ok(page.includes(declaration), `missing ${declaration}: ${page}`);
    }
    assert.ok(page.includes(">Page</span>"), page);
  });

  test("line, scroll, layer and gap draw with their own axis and depth", () => { const page = html({ views: [{ kind: 1, spacing: 4 }, { kind: 0, parent: 0, grow: 1 }] });
    assert.ok(page.includes("flex-direction:row"), page);
    assert.ok(page.includes("align-items:center"), page);
    const scroll = html({ views: [{ kind: 6 }] });
    assert.ok(scroll.includes("overflow-y:auto"), scroll);
    assert.ok(scroll.includes("min-height:0"), scroll);
    const layer = html({ views: [{ kind: 7 }, { kind: 2, parent: 0, words: "under" }, { kind: 2, parent: 0, words: "over" }] });
    assert.ok(layer.includes("position:relative"), layer);
    assert.strictEqual((layer.match(/position:absolute;inset:0/g) ?? []).length, 1, "only later layers sit over the first");
    assert.ok(layer.indexOf("under") < layer.indexOf("osp-view-layer-slot"), "the first child stays in the flow");
    assert.ok(html({ views: [{ kind: 5 }] }).includes("flex-grow:1"));
  });

  test("press, toggle, picker and sheet deliver the row's control number", () => { const page = html({ views: [ { kind: 3, action: 4, words: "Save", hint: "check", flags: FLAGS.chosen },
      { kind: 8, action: 5, words: "Night", flags: FLAGS.chosen }, { kind: 9, action: 6, words: "Size" }, { kind: 10, parent: 2, words: "Small", hint: "small", flags: FLAGS.chosen },
      { kind: 10, parent: 2, words: "Large", hint: "large" }, { kind: 16, action: 7, words: "Sheet" }, ] });
    assert.ok(page.includes('data-ordinal="4"'), page);
    assert.ok(page.includes('aria-checked="true"'), page);
    assert.ok(page.includes('data-words="false"'), "a chosen switch delivers false when tapped");
    assert.ok(page.includes('data-ordinal="6" data-words="small"'), page);
    assert.ok(page.includes('data-ordinal="6" data-words="large"'), page);
    assert.ok(page.includes('data-ordinal="7" data-words=""'), page);
    assert.ok(page.includes("osp-view-symbol"), page);
  });

  test("fields apply secret, on-commit and disabled flags from the row", () => { const page = html({ views: [ { kind: 4, action: 1, words: "visible", hint: "Type here" },
      { kind: 4, action: 2, words: "secret", flags: FLAGS.secret | FLAGS.onCommit }, { kind: 14, action: 3, words: "many", flags: FLAGS.disabled }, ] });
    assert.ok(page.includes('type="text"'), page);
    assert.ok(page.includes('value="visible"'), page);
    assert.ok(page.includes('placeholder="Type here"'), page);
    assert.ok(page.includes('type="password"'), page);
    assert.ok(page.includes('data-commit="enter"'), page);
    assert.ok(page.includes(" disabled readonly"), page);
    assert.ok(page.includes('data-commit="change"'), page);
  });

  test("label flags set fixed width, slant and centring; images and rules size from the row", () => { const page = html({ views: [
      { kind: 2, words: "Code", flags: FLAGS.fixedWidth | FLAGS.slanted | FLAGS.centred, size: 14 }, { kind: 11, size: 3, ink: 0x000000ff }, { kind: 12, words: "media/logo.png", width: 40, height: 20 },
    ] });
    assert.ok(page.includes("font-family:var(--vscode-editor-font-family,monospace)"), page);
    assert.ok(page.includes("font-style:italic"), page);
    assert.ok(page.includes("text-align:center"), page);
    assert.ok(page.includes("font-size:14px"), page);
    assert.ok(page.includes("height:3px"), page);
    assert.strictEqual(page.match(/width:100%/g)?.length, 1);
    assert.ok(page.includes("osp-view-image-missing"), "an unresolved image is a placeholder, never a raw path URL");
    assert.ok(page.includes("media/logo.png"), page);
  });

  test("choices draw nothing and unknown application text is escaped", () => {
    const page = html({ views: [{ kind: 10, words: "<script>alert(1)</script>" }, { kind: 2, words: "<img src=x onerror=alert(1)>", hint: "\" onmouseover=\"alert(2)" }] });
    assert.ok(!page.includes("<script>alert"), page);
    assert.ok(!page.includes("<img src=x"), page);
    assert.ok(page.includes("&lt;img src=x onerror=alert(1)&gt;"), page);
    assert.ok(!page.includes("onmouseover=\"alert"), page);
  });

  test("a canvas names its scene and control, and its scene is checked and kept", () => { const checked = shown({ views: [{ kind: 13, action: 9, width: 320, height: 200 }],
      scenes: [{ code: 20, index: 0, x: 800, y: 600, width: 0.5 }, { code: 1, x: 40, y: 50, width: 10, colour: 0xff0000ff, depth: 2 }],
    });
    const canvas = renderShown(checked);
    assert.ok(canvas.includes('data-ordinal="9"'), canvas);
    assert.ok(canvas.includes('data-scene="0"'), canvas);
    assert.strictEqual(checked.scenes.get(0)?.width, 800);
    assert.deepStrictEqual(checked.scenes.get(0)?.shapes, [{ shape: "disc", x: 40, y: 50, radius: 10, colour: 0xff0000ff, depth: 2 }]);
  });

  test("a bad view row refuses the whole frame", () => { const bad = (control: Parameters<typeof viewRows>[0][number], match: RegExp) =>
      assert.throws(() => shown({ views: [control] }), match);
    bad({ kind: 99 }, /unknown kind/);
    bad({ kind: 0, flags: 128 }, /unknown flags/);
    bad({ kind: 0, weight: 99 }, /font weight/);
    bad({ kind: 0, ink: -1 }, /colour/);
    bad({ kind: 0, size: Number.NaN }, /extent/);
    bad({ kind: 0, parent: 0 }, /earlier row/);
    assert.throws(() => shown({ views: [{ kind: 0 }, { kind: 0, parent: 5 }] }), /earlier row/);
  });

  test("told rows become the component tree a host draws ([NATIVE-SHM-NODES])", () => { const views: readonly ControlSpec[] = [
      // theme: record { tokens: list of {name, light, dark}, spacing: list, feedback, settle }
      { kind: 34, hint: "theme" }, { kind: 35, parent: 0, hint: "tokens" }, { kind: 34, parent: 1 }, { kind: 33, parent: 2, hint: "name", words: "accent" },
      { kind: 33, parent: 2, hint: "light", words: "#006B86" }, { kind: 33, parent: 2, hint: "dark", words: "#77D7F4" }, { kind: 35, parent: 0, hint: "spacing" },
      { kind: 33, parent: 6, weight: 1, ink: 8 }, { kind: 33, parent: 0, hint: "feedback", weight: 1, ink: 140 }, { kind: 33, parent: 0, hint: "settle", weight: 1, ink: 240 },
      // region: record { key: "root", node: button {kind,label,symbol,role,enabled,event} }
      { kind: 34, hint: "region" }, { kind: 33, parent: 10, hint: "key", words: "root" }, { kind: 34, parent: 10, hint: "node" }, { kind: 33, parent: 12, hint: "kind", words: "button" },
      { kind: 33, parent: 12, hint: "label", words: "Save" }, { kind: 33, parent: 12, hint: "symbol", words: "check" }, { kind: 33, parent: 12, hint: "role", words: "prominent" },
      { kind: 33, parent: 12, hint: "enabled", weight: 3, ink: 1 }, { kind: 33, parent: 12, hint: "event", weight: 1, ink: 3 }, ];
    const told = toldFrame(shown({ views }));
    assert.ok(told);
    // Told records are null-prototype objects; compared through their JSON shape.
    assert.deepStrictEqual(JSON.parse(JSON.stringify(told.theme)), { tokens: [{ name: "accent", light: "#006B86", dark: "#77D7F4" }], spacing: [8], feedback: 140, settle: 240 });
    assert.deepStrictEqual(JSON.parse(JSON.stringify(told.regions)), [{ key: "root", node: { kind: "button", label: "Save", symbol: "check", role: "prominent", enabled: true, event: 3 } }]);
    // A frame of plain rows tells nothing.
    assert.strictEqual(toldFrame(shown({ views: [{ kind: 0 }] })), undefined);
  });

  test("told reals carry their sign, words carry values, and a member with no name is ignored", () => { const views: readonly ControlSpec[] = [ { kind: 34, hint: "region" },
      { kind: 33, parent: 0, hint: "key", words: "root" }, { kind: 34, parent: 0, hint: "node" }, { kind: 33, parent: 2, hint: "kind", words: "scene" },
      { kind: 33, parent: 2, hint: "real", weight: 2, size: 12.5 }, { kind: 33, parent: 2, hint: "negative", weight: 4, size: 3.25 }, { kind: 33, parent: 2, words: "unnamed" }, ];
    const told = toldFrame(shown({ views }));
    assert.deepStrictEqual(JSON.parse(JSON.stringify(told?.regions[0]?.node)), { kind: "scene", real: 12.5, negative: -3.25 });
    assert.strictEqual(wordsOf("k"), "k");
    assert.strictEqual(wordsOf(true), "true");
    assert.strictEqual(wordsOf(12), "12");
    assert.strictEqual(wordsOf(null), "");
  });

  test("a told row of an unknown type or a told parent that is not earlier refuses the frame", () => { assert.throws(() => shown({ views: [{ kind: 33, weight: 5 }] }), /unknown type 5/);
    assert.throws(() => shown({ views: [{ kind: 34 }, { kind: 33, parent: 5, hint: "key" }] }), /earlier row/);
    // A negative whole number is a value, not a colour; a told row carries no flags or extents.
    const views = shown({ views: [{ kind: 34, hint: "theme" }, { kind: 33, parent: 0, hint: "feedback", weight: 1, ink: -9007199254740991 }] });
    assert.strictEqual(views.controls[1]?.value, Number.MIN_SAFE_INTEGER);
  });

  test("a scene marker names the record row of a scene component", () => { const checked = shown({ views: [{ kind: 34, hint: "region" }, { kind: 33, parent: 0, hint: "key", words: "root" },
        { kind: 34, parent: 0, hint: "node" }, { kind: 33, parent: 2, hint: "kind", words: "scene" }], scenes: [{ code: 20, index: 2, x: 640, y: 480, width: 1 }],
    });
    const told = toldFrame(checked);
    assert.ok(told);
    assert.strictEqual((told.regions[0]?.node as Record<string, unknown>).sceneRow, 2);
    assert.strictEqual(checked.scenes.get(2)?.width, 640);
    assert.throws(() => shown({ views: [{ kind: 34 }], scenes: [{ code: 20, index: 1 }] }), /names no canvas/);
  });

  test("trees compare structurally so an unchanged region keeps its revision", () => { const views: readonly ControlSpec[] = [ { kind: 34, hint: "region" },
      { kind: 33, parent: 0, hint: "key", words: "root" }, { kind: 34, parent: 0, hint: "node" }, { kind: 33, parent: 2, hint: "kind", words: "text" }, ];
    const first = toldFrame(shown({ views }))!;
    const again = toldFrame(shown({ views: [...views] }))!;
    assert.ok(sameTree(first.regions[0]?.node, again.regions[0]?.node));
    assert.ok(!sameTree(first.regions[0]?.node, toldFrame(shown({ views: [...views.slice(0, 3), { kind: 33, parent: 2, hint: "kind", words: "pair" }] }))!.regions[0]?.node));
  });

  test("a text range outside the run or not UTF-8 refuses the frame", () => { const run = new TextRun();
    const views = viewRows([{ kind: 2, words: "ok" }], run);
    const text = run.buffer();
    const outside = views.slice(0);
    new DataView(outside).setBigInt64(32, 100n, true);
    assert.throws(() => readFrame({ shapes: new ArrayBuffer(0), views: outside, text }), /text range outside/);
    const broken = Uint8Array.from([0xc3, 0x28]).buffer;
    assert.throws(() => textAt(new Uint8Array(broken), 0, 2), /UTF-8/);
    assert.throws(() => readFrame({ shapes: new ArrayBuffer(0), views: new ArrayBuffer(3), text }), /whole number of rows/);
  });

  test("a scene row must be well formed and belong to a drawn canvas", () => { assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 1 }] }), /names no canvas/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 20, index: 0 }] }), /twice/);
    assert.throws(() => shown({ views: [{ kind: 0 }], scenes: [{ code: 1, x: 1 }] }), /before any scene/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0, x: 100, y: 0 }] }), /zero width/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 99 }] }), /unknown scene code/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 6, index: 9 }] }), /lettering anchor/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 3, index: -1 }] }), /sprite index/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 22, index: 9 }, { code: 7, x: 0, y: 1 }] }), /accessible role/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 1, x: Number.POSITIVE_INFINITY }] }), /not finite/);
  });

  test("scene paths, lettering and described elements read every documented code", () => { const checked = shown({ views: [{ kind: 13, action: 1 }], scenes: [
      { code: 20, index: 0, x: 100, y: 100, width: 1 }, { code: 21, text: "atlas/sprite.png" }, { code: 22, index: 0, x: 1, y: 2, width: 3, height: 4, colour: 1, text: "Play" }, { code: 7, x: 0, y: 1 },
      { code: 5, index: 2, x: 3, y: 4, width: 1, depth: 1 }, { code: 8, index: 0, x: 1, y: 2 }, { code: 8, index: 2, x: 3, y: 4, width: 5, height: 6 }, { code: 9, index: 1, x: 2, colour: 1, depth: 1 },
      { code: 7, x: 4, y: 0 }, { code: 10, index: 0 }, { code: 11, index: 0, x: 1, y: 0, width: 0, height: 1 }, { code: 6, index: 1, x: 10, y: 20, width: 12, height: 600, text: "Hello" }, ] });
    const scene = checked.scenes.get(0);
    assert.ok(scene);
    assert.deepStrictEqual(scene.atlas, ["atlas/sprite.png"]);
    assert.deepStrictEqual(scene.access, [{ role: "button", label: "Play", x: 1, y: 2, w: 3, h: 4, value: 0, min: 0, max: 1, selected: false }]);
    assert.strictEqual(scene.shapes.length, 2);
    const path = scene.shapes[0];
    assert.ok(path && path.shape === "path");
    assert.deepStrictEqual(path.steps, [{ step: "move", to: { x: 1, y: 2 } }, { step: "curve", to: { x: 3, y: 4 }, control: { x: 5, y: 6 } }]);
    assert.deepStrictEqual(path.outline, { outline: "stroke", width: 2, bevel: true, dashes: [4] });
    assert.deepStrictEqual(path.paint, { paint: "solid", tint: { space: "rgb", channels: [1, 0, 0, 1] } });
    assert.deepStrictEqual(path.transform, { x: 3, y: 4, scale: 1 });
    const lettering = scene.shapes[1];
    assert.ok(lettering && lettering.shape === "lettering");
    assert.deepStrictEqual([lettering.text, lettering.anchor, lettering.weight], ["Hello", 1, 600]);
  });

  test("asks are read with their capability code, ticket, fields and texts", () => {
    const built = frame({ asks: [{ code: 13, ticket: "sdk/open", fields: [[1, 0, "ospml"], [2, 1024, ""], [3, 0, "document"]] }], views: [{ kind: 0, words: "x" }] });
    const asks = readAsks(built.asks, built.text);
    assert.strictEqual(asks.length, 1);
    assert.strictEqual(asks[0]?.kind, "choose-file");
    assert.strictEqual(asks[0]?.ticket, "sdk/open");
    assert.deepStrictEqual(asks[0]?.texts(1), ["ospml"]);
    assert.strictEqual(asks[0]?.number(2), 1024);
    assert.strictEqual(asks[0]?.text(3), "document");
    assert.throws(() => asks[0]?.pairs(3), /name and value pairs/);
  });

  test("a bad ask row refuses the whole run", () => { const bad = (rows: { code: number; ticket: string; fields?: readonly (readonly [number, number, string])[] }[], match: RegExp) => {
      const full = frame({ asks: rows });
      assert.throws(() => readAsks(full.asks, full.text), match);
    };
    bad([{ code: 26, ticket: "t" }], /unknown capability code/);
    bad([{ code: 0, ticket: "t" }], /unknown capability code/);
    bad([{ code: 4, ticket: "t", fields: [[-1, 0, "x"]] }], /before any ask/);
    assert.throws(() => readAsks(new ArrayBuffer(3), new ArrayBuffer(0)), /whole number of rows/);
  });

  test("scene transforms fit the region and map the pointer back into scene units", () => {
    assert.deepStrictEqual(sceneTransform({ width: 200, height: 100, zoom: 0 }, 400, 400), { scale: 2, viewedWidth: 200, viewedHeight: 200 });
    assert.deepStrictEqual(sceneTransform({ width: 200, height: 100, zoom: 0.75 }, 160, 400), { scale: 0.75, viewedWidth: 160 / 0.75, viewedHeight: 400 / 0.75 });
    assert.deepStrictEqual(sceneTransform({ width: 200, height: 100, zoom: 0.75 }, 50, 25), { scale: 0.25, viewedWidth: 200, viewedHeight: 100 });
    assert.deepStrictEqual(sceneTransform({ width: 0, height: 0, zoom: 1 }, 300, 200), { scale: 1, viewedWidth: 300, viewedHeight: 200 });
    assert.deepStrictEqual(scenePoint({ scale: 2, viewedWidth: 10, viewedHeight: 10 }, 100, 60), { x: 50, y: 30 });
  });

  test("only the shared key vocabulary leaves a scene", () => {
    for (const [from, to] of [["ArrowLeft", "left"], ["ArrowDown", "down"], [" ", "space"], ["Enter", "enter"], ["Escape", "escape"], ["Tab", "tab"], ["Backspace", "backspace"], ["A", "a"], ["7", "7"]] as const) {
      assert.strictEqual(keyName(from), to);
    }
    for (const refused of ["Shift", "Alt", "F5", "PageUp", "", "ab", "<"]) assert.strictEqual(keyName(refused), undefined);
  });
});

// The framed component's shared contract ([UI-COMPONENTS-FRAMED]), restored from the base branch.
const frameStyle = { name: "solution-column", width: 430, height: 0, top: 34, right: 30, bottom: 34, left: 30, radius: 5, gap: 12, fill: "canvas", ink: "chalk", line: "divider", grow: false };

/** One root region holding `node`, the shape every renderer entry point takes. */
function region(node: unknown): Map<string, { revision: number; node: unknown }> {
  return new Map([["root", { revision: 3, node }]]);
}

function framed(properties: Record<string, unknown> = frameStyle): string {
  const body = { kind: "text", text: "A shared mathematical surface", role: "body", tone: "primary" };
  return renderWorkspace({ regions: new Map([["root", { revision: 1, node: { kind: "framed", style: properties, body } }]]) });
}

suite("native shared frame", () => {
  test("vertical scroll frames cap natural height at the authored viewport bound", () => {
    const html = framed({ ...frameStyle, height: 530, viewportHeightInset: 180, scrollY: true });
    for (const declaration of ["max-height:max(530px, calc(100dvh - 180px))", "min-height:530px", "height:auto", "overflow-y:auto", "flex:0 0 auto"]) {
      assert.ok(html.includes(declaration), `missing ${declaration}`);
    }
    assert.ok(!html.includes(";height:max("), "a short proof retains its natural height");
    assert.ok(!framed().includes("overflow-y:auto"), "default frames retain their prior overflow behavior");
    assert.ok(!framed({ ...frameStyle, scrollY: false }).includes("overflow-y:auto"));
    assert.ok(framed({ ...frameStyle, height: 200, scrollY: true }).includes("height:200px"));
    for (const scrollY of [null, "true", 1]) assert.throws(() => framed({ ...frameStyle, scrollY }), /scrollY/i);
  });

  test("vertical scroll frames cap natural height at the authored viewport bound (extra: a zero inset owns the whole viewport)", () => {
    const html = framed({ ...frameStyle, height: 40, viewportHeightInset: 0, scrollY: true });
    assert.ok(html.includes("max-height:max(40px, calc(100dvh - 0px))"), html);
    assert.ok(html.includes("min-height:40px"));
    assert.ok(framed({ ...frameStyle, height: 640, viewportHeightInset: 32768, scrollY: true }).includes("calc(100dvh - 32768px)"));
    for (const viewportHeightInset of [32769, 1.5]) { assert.throws(() => framed({ ...frameStyle, viewportHeightInset, scrollY: true }), /viewportHeightInset/i);
    }
  });

  test("viewport-relative height retains the shared minimum and validates its inset", () => {
    const html = framed({ ...frameStyle, height: 440, grow: true, viewportHeightInset: 270 });
    assert.ok(html.includes("height:max(440px, calc(100dvh - 270px))"));
    assert.ok(html.includes("flex:0 0 auto"));
    assert.ok(!html.includes("flex:1"), "viewport-sized frames keep their authored height instead of growing");
    assert.ok(framed({ ...frameStyle, height: 440, viewportHeightInset: -1 }).includes("height:440px"));
    assert.ok(!framed().includes("100dvh"), "absent viewport metrics preserve existing frame dimensions");
    assert.ok(framed({ ...frameStyle, viewportHeightInset: 0 }).includes("height:max(0px, calc(100dvh - 0px))"));
    for (const viewportHeightInset of [-2, 0.5, 32769, "270px", null]) assert.throws(() => framed({ ...frameStyle, viewportHeightInset }), /viewportHeightInset/i); }); test("viewport-relative height retains the shared minimum and validates its inset (extra: an authored width survives the viewport bound)", () => {
    const html = framed({ ...frameStyle, width: 320, height: 300, viewportHeightInset: 90 });
    assert.ok(html.includes("width:320px"), html);
    assert.ok(html.includes("height:max(300px, calc(100dvh - 90px))"));
    assert.ok(html.includes("flex-shrink:0"), "a fixed frame keeps its authored extent");
  });

  test("symbol size follows the shared frame and rejects malformed metrics", () => { assert.ok(framed({ ...frameStyle, symbolSize: 13 }).includes("--osp-frame-symbol-size:13px"));
    assert.ok(!framed({ ...frameStyle, symbolSize: 0 }).includes("--osp-frame-symbol-size:"), "zero inherits the surrounding symbol size");
    for (const symbolSize of [-1, 0.5, 32769, "13px", null]) assert.throws(() => framed({ ...frameStyle, symbolSize }), /symbolSize/i);
  });

  test("symbol size follows the shared frame and rejects malformed metrics (extra: the upper bound is inclusive)", () => {
    assert.ok(framed({ ...frameStyle, symbolSize: 32768 }).includes("--osp-frame-symbol-size:32768px"));
    assert.ok(framed({ ...frameStyle, symbolSize: 1 }).includes("--osp-frame-symbol-size:1px"));
    for (const symbolSize of [32770, Number.MAX_SAFE_INTEGER]) assert.throws(() => framed({ ...frameStyle, symbolSize }), /symbolSize/i);
  });

  test("a layer preserves the existing fill contract of its aside surface", () => { const text = { kind: "text", text: "A shared pane", role: "body", tone: "primary" };
    const node = { kind: "layer", children: [{ kind: "aside", pane: text, aside: text, width: 300, side: "trailing", event: 0 }] };
    const html = renderWorkspace({ regions: region(node) });
    assert.ok(html.includes('class="osp-layer osp-layer-fill"'), "an overlay must retain the underlying full-height aside");
  });

  test("a layer preserves the existing fill contract of its aside surface (extra: a growing framed child fills too)", () => {
    const child = { kind: "framed", style: { ...frameStyle, width: 0, height: 0, grow: true }, body: { kind: "text", text: "x", role: "body", tone: "primary" } };
    const html = renderWorkspace({ regions: region({ kind: "layer", children: [child] }) });
    assert.ok(html.includes('class="osp-layer osp-layer-fill"'), html);
    const plain = renderWorkspace({ regions: region({ kind: "layer", children: [{ kind: "text", text: "x", role: "body", tone: "primary" }] }) });
    assert.ok(!plain.includes("osp-layer-fill"), "a plain child does not fake an overlay fill");
  });

  test("authored backdrop fading and trailing symbols reach both presentation contracts", () => {
    const html = framed({ ...frameStyle, fadeBackdrop: true, trailingSymbols: true });
    assert.ok(html.includes("--osp-frame-backdrop-mask:linear-gradient(to right, transparent, #000 5%, #000 60%, transparent), linear-gradient(to bottom, transparent, #000 5%, #000 86%, transparent)"), "a board's illustrative backdrop needs its authored fade");
    assert.ok(html.includes("--osp-frame-symbol-direction:row-reverse"), "trailing symbols belong to the shared frame");
    for (const properties of [{ fadeBackdrop: "yes" }, { trailingSymbols: "yes" }]) { assert.throws(() => framed({ ...frameStyle, ...properties }), /frame/i);
    }
  });

  test("authored backdrop fading and trailing symbols reach both presentation contracts (extra: plain frames mask nothing)", () => {
    const html = framed();
    assert.ok(html.includes("--osp-frame-backdrop-mask:none"), html);
    assert.ok(html.includes("--osp-frame-backdrop-blend:normal"));
    assert.ok(html.includes("--osp-frame-symbol-direction:inherit"));
  });

  test("floating shadow and plain nested symbol direction preserve the shared contract", () => {
    assert.ok(framed({ ...frameStyle, shadow: "floating" }).includes("box-shadow:0 20px 50px color-mix(in srgb, var(--osp-brand-primary) 13.33%, transparent)"));
    assert.ok(!framed().includes("box-shadow:"), "plain frames have no floating shadow");
    assert.ok(framed().includes("--osp-frame-symbol-direction:inherit"), "plain nested frames retain the enclosing symbol placement");
    for (const shadow of ["raised", "floating;display:none", 1, null]) assert.throws(() => framed({ ...frameStyle, shadow }), /shadow/i);
  });

  test("floating shadow and plain nested symbol direction preserve the shared contract (extra: quiet and flat row switches draw their variables)", () => {
    const html = framed({ ...frameStyle, quietControls: true, flatRows: true, iconOnly: true, selectionGroup: true });
    for (const declaration of ["--osp-frame-control-fill:transparent", "--osp-frame-row-padding:0", "--osp-frame-row-radius:3px", "--osp-frame-row-border:0", "--osp-frame-icon-label:none", "--osp-frame-button-padding:0"]) {
      assert.ok(html.includes(declaration), declaration);
    }
  });

  test("application-authored geometry and semantic surfaces survive rendering", () => {
    const html = framed();
    for (const expected of ["osp-frame-solution-column", "width:430px", "padding:34px 30px 34px 30px", "border-radius:5px", "--osp-stack-gap:12px", "color:var(--osp-brand-chalk)", "--osp-brand-canvas", "--osp-brand-divider", "A shared mathematical surface"]) {
      assert.ok(html.includes(expected), expected);
    }
    assert.ok(!html.includes("height:0px"), "zero retains natural height");
  });

  test("application-authored geometry and semantic surfaces survive rendering (extra: each edge carries its own inset)", () => {
    const html = framed({ ...frameStyle, top: 1, right: 2, bottom: 3, left: 4, radius: 9, gap: 21 });
    assert.ok(html.includes("padding:1px 2px 3px 4px"), html);
    assert.ok(html.includes("border-radius:9px"));
    assert.ok(html.includes("--osp-stack-gap:21px"));
  });

  test("growing frames fill their parent without inventing application dimensions", () => {
    const html = framed({ ...frameStyle, width: 0, grow: true });
    assert.ok(html.includes("flex:1"));
    assert.ok(!html.includes("width:0px"));
  });

  test("growing frames fill their parent without inventing application dimensions (extra: a hug frame shrinks to its content)", () => {
    const html = framed({ ...frameStyle, width: 0, hug: true });
    assert.ok(html.includes("width:fit-content"), html);
    assert.ok(html.includes("flex-shrink:0"));
    assert.ok(html.includes('data-frame-width="content"'));
  });

  test("typography alignment and intrinsic sizing come from the shared tree", () => {
    const html = framed({ ...frameStyle, width: 0, font: "serif", textSize: 27, italic: true, hug: true, align: "center", cross: "end", controlHeight: 40, controlPadding: 12, lineHeight: 32.48, tracking: -0.6, mathScale: 1.08, quietControls: true, iconOnly: true });
    for (const expected of ["--osp-frame-font:var(--osp-font-serif)", "--osp-frame-text-size:27px", "--osp-frame-font-style:italic", "width:fit-content", "align-content:center", "align-items:end", "--osp-frame-control-height:40px", "--osp-frame-control-padding:12px", "--osp-frame-line-height:32.48px", "--osp-frame-tracking:-0.6px", "--osp-frame-math-scale:1.08", "--osp-frame-control-fill:transparent", "--osp-frame-icon-label:none"]) {
      assert.ok(html.includes(expected), expected);
    }
    for (const bad of [{ font: "serif;display:none" }, { textSize: -1 }, { italic: "yes" }, { hug: 1 }, { align: "diagonal" }, { cross: "diagonal" }, { controlHeight: -1 }, { controlPadding: -2 }, { lineHeight: -1 }, { lineHeight: Infinity }, { tracking: NaN }, { mathScale: -1 }, { mathScale: Infinity }, { quietControls: "yes" }, { iconOnly: "yes" }, { selectionGroup: "yes" }]) {
      assert.throws(() => framed({ ...frameStyle, ...bad }), /frame/i);
    }
  });

  test("typography alignment and intrinsic sizing come from the shared tree (extra: baseline alignment and mono source runs)", () => {
    const html = framed({ ...frameStyle, width: 0, font: "mono", align: "baseline", cross: "start", textSize: 13 });
    for (const expected of ["--osp-frame-font:var(--osp-font-mono)", "--osp-frame-text-size:13px", "justify-content:start", "align-items:start", "--osp-stack-align:baseline"]) {
      assert.ok(html.includes(expected), expected);
    }
    assert.throws(() => framed({ ...frameStyle, font: "system" }), /font/i);
  });

  test("scoped slate palette and transcription dialog preserve accessible content and events", () => {
    const html = renderWorkspace({ regions: region({ kind: "themed", theme: { tokens: [{ name: "canvas", light: "#202723", dark: "#202723" }] },
      body: { kind: "dialog", title: "Review <transcription>", event: 0, body: { kind: "button", label: "Insert", enabled: true, event: 1 } } }) });
    for (const expected of ["osp-themed", "#202723", 'role="dialog"', 'aria-modal="true"', "Review &lt;transcription&gt;", 'data-event-ordinal="0"', 'data-event-ordinal="1"']) {
      assert.ok(html.includes(expected), expected);
    }
  });

  test("scoped slate palette and transcription dialog preserve accessible content and events (extra: malformed tokens cannot inject into the scoped theme)", () => {
    const html = renderWorkspace({ regions: region({ kind: "themed", theme: { tokens: [{ name: "canvas;}body{color:red", light: "#202723", dark: "#202723" }] },
      body: { kind: "text", text: "Kept", role: "body", tone: "primary" } }) });
    assert.ok(!html.includes("body{"), html);
    assert.ok(html.includes("Kept"));
  });

  test("inline mathematical and source runs retain emphasis colours and escaped wording", () => {
    const render = (runs: unknown) => renderWorkspace({ regions: new Map([["root", { revision: 1, node: { kind: "rich-text", role: "mono", runs } }]]) });
    const html = render([{ text: "<p>", ink: "sourceType", italic: true }]);
    assert.ok(html.includes('osp-role-mono osp-tone-primary'), "rich text must inherit primary text colour unless a run explicitly changes it");
    assert.ok(html.includes("&lt;p&gt;"));
    assert.ok(html.includes("color:var(--osp-brand-sourceType);font-style:italic"));
    assert.ok(!html.includes("data-event-ordinal"));
    assert.throws(() => render([{ text: "bad", ink: "red;display:none", italic: false }]), /ink/);
  });

  test("inline mathematical and source runs retain emphasis colours and escaped wording (extra: runs keep their order and an empty ink inherits)", () => {
    const render = (runs: unknown) => renderWorkspace({ regions: new Map([["root", { revision: 1, node: { kind: "rich-text", role: "mono", runs } }]]) });
    const html = render([{ text: "plain", ink: "", italic: false }, { text: "kw", ink: "sourceKeyword", italic: true }]);
    assert.ok(html.indexOf("plain") < html.indexOf("kw"), html);
    assert.ok(html.includes('style="font-style:normal">plain'));
    assert.ok(!html.includes("color:var(--osp-brand-);"));
    for (const run of [{ text: 1, ink: "", italic: false }, { text: "x", ink: "", italic: "y" }, "run"]) { assert.throws(() => render([run]), /rich text|run/i);
    }
  });

  test("invalid dimensions and unsafe token names fail explicitly", () => {
    for (const properties of [{ ...frameStyle, width: -1 }, { ...frameStyle, top: 1.5 }, { ...frameStyle, fill: "red;position:fixed" }, { ...frameStyle, line: "x\"onclick" }, { ...frameStyle, name: "x\"onclick" }, { ...frameStyle, grow: "true" }]) {
      assert.throws(() => framed(properties), /frame/i);
    }
  });

  test("invalid dimensions and unsafe token names fail explicitly (extra: bounds apply to every metric and the name grammar is enforced)", () => {
    for (const properties of [{ ...frameStyle, radius: -1 }, { ...frameStyle, gap: 32769 }, { ...frameStyle, right: "8" }, { ...frameStyle, ink: "Canvas" }, { ...frameStyle, name: "1solution" }, { ...frameStyle, name: "solution column" }]) {
      assert.throws(() => framed(properties), /frame/i);
    }
    assert.ok(framed({ ...frameStyle, name: "" }).includes('data-frame-width="fixed"'));
  });
});

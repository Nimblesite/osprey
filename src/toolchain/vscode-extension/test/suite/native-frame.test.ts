// [NATIVE-SHM-VIEW] [NATIVE-SHM-SCENE] [NATIVE-SHM-ASK]: a frame's rows are checked like the
// Rust host's rows.rs and drawn with every style value the row carries. A bad row refuses the
// whole frame; nothing from the application is guessed.
import * as assert from "node:assert/strict";
import { readAsks } from "../../client/src/osprey-ui/asks";
import { keyName, scenePoint, sceneTransform } from "../../client/src/osprey-ui/webview/events";
import { renderShown } from "../../client/src/osprey-ui/webview/render";
import { FLAGS, readFrame, textAt, type Shown } from "../../client/src/osprey-ui/webview/rows";
import { frame, TextRun, viewRows } from "./native-rows-kit";

function shown(options: Parameters<typeof frame>[0]): Shown {
  return readFrame(frame(options));
}

function html(options: Parameters<typeof frame>[0]): string {
  return renderShown(shown(options));
}

suite("native shared rows", () => {
  test("a column applies the row's fill, ink, padding, spacing, radius and extent", () => {
    const page = html({ views: [{ kind: 0, fill: 0x112233ff, ink: 0xffffffff, padding: 12, spacing: 7,
      radius: 5, width: 430, grow: 1, border: 0x445566ff }, { kind: 2, parent: 0, words: "Page" }] });
    for (const declaration of ["background:#112233ff", "color:#ffffffff", "padding:12px", "gap:7px",
      "border-radius:5px", "width:430px", "flex-grow:1", "border:1px solid #445566ff", "display:flex",
      "flex-direction:column"]) {
      assert.ok(page.includes(declaration), `missing ${declaration}: ${page}`);
    }
    assert.ok(page.includes(">Page</span>"), page);
  });

  test("line, scroll, layer and gap draw with their own axis and depth", () => {
    const page = html({ views: [{ kind: 1, spacing: 4 }, { kind: 0, parent: 0, grow: 1 }] });
    assert.ok(page.includes("flex-direction:row"), page);
    assert.ok(page.includes("align-items:center"), page);
    const scroll = html({ views: [{ kind: 6 }] });
    assert.ok(scroll.includes("overflow-y:auto"), scroll);
    assert.ok(scroll.includes("min-height:0"), scroll);
    const layer = html({ views: [{ kind: 7 }, { kind: 2, parent: 0, words: "under" }, { kind: 2, parent: 0, words: "over" }] });
    assert.ok(layer.includes("position:relative"), layer);
    assert.strictEqual((layer.match(/position:absolute;inset:0/g) ?? []).length, 1, "only later layers sit over the first");
    assert.ok(layer.indexOf("under") < layer.indexOf("osp-layer-slot"), "the first child stays in the flow");
    assert.ok(html({ views: [{ kind: 5 }] }).includes("flex-grow:1"));
  });

  test("press, toggle, picker and sheet deliver the row's control number", () => {
    const page = html({ views: [
      { kind: 3, action: 4, words: "Save", hint: "check", flags: FLAGS.chosen },
      { kind: 8, action: 5, words: "Night", flags: FLAGS.chosen },
      { kind: 9, action: 6, words: "Size" },
      { kind: 10, parent: 2, words: "Small", hint: "small", flags: FLAGS.chosen },
      { kind: 10, parent: 2, words: "Large", hint: "large" },
      { kind: 16, action: 7, words: "Sheet" },
    ] });
    assert.ok(page.includes('data-ordinal="4"'), page);
    assert.ok(page.includes('aria-checked="true"'), page);
    assert.ok(page.includes('data-words="false"'), "a chosen switch delivers false when tapped");
    assert.ok(page.includes('data-ordinal="6" data-words="small"'), page);
    assert.ok(page.includes('data-ordinal="6" data-words="large"'), page);
    assert.ok(page.includes('data-ordinal="7" data-words=""'), page);
    assert.ok(page.includes("osp-symbol"), page);
  });

  test("fields apply secret, on-commit and disabled flags from the row", () => {
    const page = html({ views: [
      { kind: 4, action: 1, words: "visible", hint: "Type here" },
      { kind: 4, action: 2, words: "secret", flags: FLAGS.secret | FLAGS.onCommit },
      { kind: 14, action: 3, words: "many", flags: FLAGS.disabled },
    ] });
    assert.ok(page.includes('type="text"'), page);
    assert.ok(page.includes('value="visible"'), page);
    assert.ok(page.includes('placeholder="Type here"'), page);
    assert.ok(page.includes('type="password"'), page);
    assert.ok(page.includes('data-commit="enter"'), page);
    assert.ok(page.includes(" disabled readonly"), page);
    assert.ok(page.includes('data-commit="change"'), page);
  });

  test("label flags set fixed width, slant and centring; images and rules size from the row", () => {
    const page = html({ views: [
      { kind: 2, words: "Code", flags: FLAGS.fixedWidth | FLAGS.slanted | FLAGS.centred, size: 14 },
      { kind: 11, size: 3, ink: 0x000000ff },
      { kind: 12, words: "media/logo.png", width: 40, height: 20 },
    ] });
    assert.ok(page.includes("font-family:var(--vscode-editor-font-family,monospace)"), page);
    assert.ok(page.includes("font-style:italic"), page);
    assert.ok(page.includes("text-align:center"), page);
    assert.ok(page.includes("font-size:14px"), page);
    assert.ok(page.includes("height:3px"), page);
    assert.strictEqual(page.match(/width:100%/g)?.length, 1);
    assert.ok(page.includes("osp-image-missing"), "an unresolved image is a placeholder, never a raw path URL");
    assert.ok(page.includes("media/logo.png"), page);
  });

  test("choices draw nothing and unknown application text is escaped", () => {
    const page = html({ views: [{ kind: 10, words: "<script>alert(1)</script>" },
      { kind: 2, words: "<img src=x onerror=alert(1)>", hint: "\" onmouseover=\"alert(2)" }] });
    assert.ok(!page.includes("<script>alert"), page);
    assert.ok(!page.includes("<img src=x"), page);
    assert.ok(page.includes("&lt;img src=x onerror=alert(1)&gt;"), page);
    assert.ok(!page.includes("onmouseover=\"alert"), page);
  });

  test("a canvas names its scene and control, and its scene is checked and kept", () => {
    const checked = shown({
      views: [{ kind: 13, action: 9, width: 320, height: 200 }],
      scenes: [{ code: 20, index: 0, x: 800, y: 600, width: 0.5 }, { code: 1, x: 40, y: 50, width: 10, colour: 0xff0000ff, depth: 2 }],
    });
    const canvas = renderShown(checked);
    assert.ok(canvas.includes('data-ordinal="9"'), canvas);
    assert.ok(canvas.includes('data-scene="0"'), canvas);
    assert.strictEqual(checked.scenes.get(0)?.width, 800);
    assert.deepStrictEqual(checked.scenes.get(0)?.shapes, [{ shape: "disc", x: 40, y: 50, radius: 10, colour: 0xff0000ff, depth: 2 }]);
  });

  test("a bad view row refuses the whole frame", () => {
    const bad = (control: Parameters<typeof viewRows>[0][number], match: RegExp) =>
      assert.throws(() => shown({ views: [control] }), match);
    bad({ kind: 99 }, /unknown kind/);
    bad({ kind: 0, flags: 128 }, /unknown flags/);
    bad({ kind: 0, weight: 99 }, /font weight/);
    bad({ kind: 0, ink: -1 }, /colour/);
    bad({ kind: 0, size: Number.NaN }, /extent/);
    bad({ kind: 0, parent: 0 }, /earlier row/);
    assert.throws(() => shown({ views: [{ kind: 0 }, { kind: 0, parent: 5 }] }), /earlier row/);
  });

  test("a text range outside the run or not UTF-8 refuses the frame", () => {
    const run = new TextRun();
    const views = viewRows([{ kind: 2, words: "ok" }], run);
    const text = run.buffer();
    const outside = views.slice(0);
    new DataView(outside).setBigInt64(32, 100n, true);
    assert.throws(() => readFrame({ shapes: new ArrayBuffer(0), views: outside, text }), /text range outside/);
    const broken = Uint8Array.from([0xc3, 0x28]).buffer;
    assert.throws(() => textAt(new Uint8Array(broken), 0, 2), /UTF-8/);
    assert.throws(() => readFrame({ shapes: new ArrayBuffer(0), views: new ArrayBuffer(3), text }), /whole number of rows/);
  });

  test("a scene row must be well formed and belong to a drawn canvas", () => {
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 1 }] }), /names no canvas/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 20, index: 0 }] }), /twice/);
    assert.throws(() => shown({ views: [{ kind: 0 }], scenes: [{ code: 1, x: 1 }] }), /before any scene/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0, x: 100, y: 0 }] }), /zero width/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 99 }] }), /unknown scene code/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 6, index: 9 }] }), /lettering anchor/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 3, index: -1 }] }), /sprite index/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 22, index: 9 }, { code: 7, x: 0, y: 1 }] }), /accessible role/);
    assert.throws(() => shown({ views: [{ kind: 13 }], scenes: [{ code: 20, index: 0 }, { code: 1, x: Number.POSITIVE_INFINITY }] }), /not finite/);
  });

  test("scene paths, lettering and described elements read every documented code", () => {
    const checked = shown({ views: [{ kind: 13, action: 1 }], scenes: [
      { code: 20, index: 0, x: 100, y: 100, width: 1 },
      { code: 21, text: "atlas/sprite.png" },
      { code: 22, index: 0, x: 1, y: 2, width: 3, height: 4, colour: 1, text: "Play" },
      { code: 7, x: 0, y: 1 },
      { code: 5, index: 2, x: 3, y: 4, width: 1, depth: 1 },
      { code: 8, index: 0, x: 1, y: 2 },
      { code: 8, index: 2, x: 3, y: 4, width: 5, height: 6 },
      { code: 9, index: 1, x: 2, colour: 1, depth: 1 },
      { code: 7, x: 4, y: 0 },
      { code: 10, index: 0 },
      { code: 11, index: 0, x: 1, y: 0, width: 0, height: 1 },
      { code: 6, index: 1, x: 10, y: 20, width: 12, height: 600, text: "Hello" },
    ] });
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

  test("a bad ask row refuses the whole run", () => {
    const bad = (rows: { code: number; ticket: string; fields?: readonly (readonly [number, number, string])[] }[], match: RegExp) => {
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
    for (const [from, to] of [["ArrowLeft", "left"], ["ArrowDown", "down"], [" ", "space"], ["Enter", "enter"],
      ["Escape", "escape"], ["Tab", "tab"], ["Backspace", "backspace"], ["A", "a"], ["7", "7"]] as const) {
      assert.strictEqual(keyName(from), to);
    }
    for (const refused of ["Shift", "Alt", "F5", "PageUp", "", "ab", "<"]) assert.strictEqual(keyName(refused), undefined);
  });
});

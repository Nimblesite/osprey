import * as assert from "node:assert/strict";
import { renderWorkspace } from "../../client/src/osprey-ui/render";

const style = { name: "solution-column", width: 430, height: 0, top: 34, right: 30, bottom: 34, left: 30, radius: 5, gap: 12, fill: "canvas", ink: "chalk", line: "divider", grow: false };
function frame(properties: Record<string, unknown> = style): string {
  return renderWorkspace({ regions: new Map([["root", { revision: 1, node: {
    kind: "framed", style: properties, body: { kind: "text", text: "A shared mathematical surface", role: "body", tone: "primary" },
  } }]]) });
}

suite("native shared frame", () => {
  test("vertical scroll frames cap natural height at the authored viewport bound", () => {
    const html = frame({ ...style, height: 530, viewportHeightInset: 180, scrollY: true });
    for (const declaration of ["max-height:max(530px, calc(100dvh - 180px))", "min-height:530px", "height:auto", "overflow-y:auto", "flex:0 0 auto"]) {
      assert.ok(html.includes(declaration), `missing ${declaration}`);
    }
    assert.ok(!html.includes(";height:max("), "a short proof retains its natural height");
    assert.ok(!frame().includes("overflow-y:auto"), "default frames retain their prior overflow behavior");
    assert.ok(!frame({ ...style, scrollY: false }).includes("overflow-y:auto"));
    assert.ok(frame({ ...style, height: 200, scrollY: true }).includes("height:200px"));
    for (const scrollY of [null, "true", 1]) assert.throws(() => frame({ ...style, scrollY }), /scrollY/i);
  });

  test("viewport-relative height retains the shared minimum and validates its inset", () => {
    const html = frame({ ...style, height: 440, grow: true, viewportHeightInset: 270 });
    assert.ok(html.includes("height:max(440px, calc(100dvh - 270px))"));
    assert.ok(html.includes("flex:0 0 auto"));
    assert.ok(!html.includes("flex:1"), "viewport-sized frames keep their authored height instead of growing");
    assert.ok(frame({ ...style, height: 440, viewportHeightInset: -1 }).includes("height:440px"));
    assert.ok(!frame().includes("100dvh"), "absent viewport metrics preserve existing frame dimensions");
    assert.ok(frame({ ...style, viewportHeightInset: 0 }).includes("height:max(0px, calc(100dvh - 0px))"));
    for (const viewportHeightInset of [-2, 0.5, 32769, "270px", null]) assert.throws(() => frame({ ...style, viewportHeightInset }), /viewportHeightInset/i);
  });
  test("symbol size follows the shared frame and rejects malformed metrics", () => {
    assert.ok(frame({ ...style, symbolSize: 13 }).includes("--osp-frame-symbol-size:13px"));
    assert.ok(!frame({ ...style, symbolSize: 0 }).includes("--osp-frame-symbol-size:"), "zero inherits the surrounding symbol size");
    for (const symbolSize of [-1, 0.5, 32769, "13px", null]) assert.throws(() => frame({ ...style, symbolSize }), /symbolSize/i);
  });
  test("a layer preserves the existing fill contract of its aside surface", () => {
    const text = { kind: "text", text: "A shared pane", role: "body", tone: "primary" };
    const node = { kind: "layer", children: [{ kind: "aside", pane: text, aside: text, width: 300, side: "trailing", event: 0 }] };
    const html = renderWorkspace({ regions: new Map([["root", { revision: 1, node }]]) });
    assert.ok(html.includes('class="osp-layer osp-layer-fill"'), "an overlay must retain the underlying full-height aside");
  });

  test("authored backdrop fading and trailing symbols reach both presentation contracts", () => {
    const html = frame({ ...style, fadeBackdrop: true, trailingSymbols: true });
    assert.ok(html.includes("--osp-frame-backdrop-mask:linear-gradient(to right, transparent, #000 5%, #000 60%, transparent), linear-gradient(to bottom, transparent, #000 5%, #000 86%, transparent)"), "a board's illustrative backdrop needs its authored fade");
    assert.ok(html.includes("--osp-frame-symbol-direction:row-reverse"), "trailing symbols belong to the shared frame");
    for (const properties of [{ fadeBackdrop: "yes" }, { trailingSymbols: "yes" }]) {
      assert.throws(() => frame({ ...style, ...properties }), /frame/i);
    }
  });

  test("floating shadow and plain nested symbol direction preserve the shared contract", () => {
    assert.ok(frame({ ...style, shadow: "floating" }).includes("box-shadow:0 20px 50px color-mix(in srgb, var(--osp-brand-primary) 13.33%, transparent)"));
    assert.ok(!frame().includes("box-shadow:"), "plain frames have no floating shadow");
    assert.ok(frame().includes("--osp-frame-symbol-direction:inherit"), "plain nested frames retain the enclosing symbol placement");
    for (const shadow of ["raised", "floating;display:none", 1, null]) assert.throws(() => frame({ ...style, shadow }), /shadow/i);
  });

  test("application-authored geometry and semantic surfaces survive rendering", () => {
    const html = frame();
    for (const expected of ["osp-frame-solution-column", "width:430px", "padding:34px 30px 34px 30px", "border-radius:5px", "--osp-stack-gap:12px", "color:var(--osp-brand-chalk)", "--osp-brand-canvas", "--osp-brand-divider", "A shared mathematical surface"]) {
      assert.ok(html.includes(expected), expected);
    }
    assert.ok(!html.includes("height:0px"), "zero retains natural height");
  });

  test("growing frames fill their parent without inventing application dimensions", () => {
    const html = frame({ ...style, width: 0, grow: true });
    assert.ok(html.includes("flex:1"));
    assert.ok(!html.includes("width:0px"));
  });

  test("typography alignment and intrinsic sizing come from the shared tree", () => {
    const html = frame({ ...style, width: 0, font: "serif", textSize: 27, italic: true, hug: true, align: "center", cross: "end", controlHeight: 40, controlPadding: 12, lineHeight: 32.48, tracking: -0.6, mathScale: 1.08, quietControls: true, iconOnly: true });
    for (const expected of ["--osp-frame-font:var(--osp-font-serif)", "--osp-frame-text-size:27px", "--osp-frame-font-style:italic", "width:fit-content", "align-content:center", "align-items:end", "--osp-frame-control-height:40px", "--osp-frame-control-padding:12px", "--osp-frame-line-height:32.48px", "--osp-frame-tracking:-0.6px", "--osp-frame-math-scale:1.08", "--osp-frame-control-fill:transparent", "--osp-frame-icon-label:none"]) {
      assert.ok(html.includes(expected), expected);
    }
    for (const bad of [{ font: "serif;display:none" }, { textSize: -1 }, { italic: "yes" }, { hug: 1 }, { align: "diagonal" }, { cross: "diagonal" }, { controlHeight: -1 }, { controlPadding: -2 }, { lineHeight: -1 }, { lineHeight: Infinity }, { tracking: NaN }, { mathScale: -1 }, { mathScale: Infinity }, { quietControls: "yes" }, { iconOnly: "yes" }]) {
      assert.throws(() => frame({ ...style, ...bad }), /frame/i);
    }
  });

  test("scoped slate palette and transcription dialog preserve accessible content and events", () => {
    const html = renderWorkspace({ regions: new Map([["root", { revision: 3, node: {
      kind: "themed", theme: { tokens: [{ name: "canvas", light: "#202723", dark: "#202723" }] }, body: {
        kind: "dialog", title: "Review <transcription>", event: 0, body: { kind: "button", label: "Insert", enabled: true, event: 1 },
      },
    } }]]) });
    for (const expected of ["osp-themed", "#202723", 'role="dialog"', 'aria-modal="true"', "Review &lt;transcription&gt;", 'data-event-ordinal="0"', 'data-event-ordinal="1"']) {
      assert.ok(html.includes(expected), expected);
    }
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

  test("invalid dimensions and unsafe token names fail explicitly", () => {
    for (const properties of [{ ...style, width: -1 }, { ...style, top: 1.5 }, { ...style, fill: "red;position:fixed" }, { ...style, line: "x\"onclick" }, { ...style, name: "x\"onclick" }, { ...style, grow: "true" }]) {
      assert.throws(() => frame(properties), /frame/i);
    }
  });
});

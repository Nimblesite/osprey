import * as assert from "node:assert/strict";
import { renderWorkspace } from "../../client/src/osprey-ui/render";

const sym = (text: string) => ({ kind: "sym", text });
const num = (text: string) => ({ kind: "num", text });
const formula = (math: unknown, spoken = "x < 2"): string => renderWorkspace({
  regions: new Map([["root", { revision: 1, node: { kind: "math", math, spoken } }]]),
});

suite("native mathematical renderer", () => {
  test("structured fractions, radicals, scripts and operators become semantic MathML", () => {
    const html = formula({ kind: "op", symbol: "<", left: { kind: "frac", top: { kind: "sup", base: sym("x"), script: num("2") },
      bottom: { kind: "root", body: { kind: "sub", base: sym("a"), script: num("1") } } }, right: num("2") });
    for (const tag of ["math", "mrow", "mo", "mi", "mn", "mfrac", "msqrt", "msub", "msup"]) assert.ok(html.includes(`<${tag}`), tag);
    assert.ok(html.includes('xmlns="http://www.w3.org/1998/Math/MathML"'));
    assert.ok(html.includes('aria-label="x &lt; 2"'));
    assert.ok(html.includes("<mo>&lt;</mo>"));
    assert.ok(!html.includes("data-event-ordinal"));
  });

  test("binders, fences and large operators preserve operand order", () => {
    const html = formula({ kind: "binder", symbol: "∀", names: [sym("n")], body: { kind: "paren", body:
      { kind: "big", symbol: "∑", lower: num("0"), upper: sym("n"), body: { kind: "seq", items: [sym("x"), sym("y")] } } } });
    assert.ok(html.includes("<munderover>"));
    assert.ok(html.includes('<mo fence="true" stretchy="true">(</mo>'));
    assert.ok(html.includes('<mo fence="true" stretchy="true">)</mo>'));
    assert.ok(html.indexOf("<mn>0</mn>") < html.indexOf("<mi>n</mi>", html.indexOf("<munderover>")));
    assert.ok(html.includes("<mrow><mi>x</mi><mi>y</mi></mrow>"));
  });

  test("standalone operator glyphs keep mathematical operator spacing", () => {
    assert.ok(formula({ kind: "seq", items: [sym("¬"), sym("p")] }).includes("<mo>¬</mo><mi>p</mi>"));
  });

  test("every mathematical string is escaped, including accessible text", () => {
    const html = formula(sym('</mi><script>alert("x")</script>'), '"><img src=x onerror=alert(1)>');
    assert.ok(!html.includes("<script>"));
    assert.ok(!html.includes("<img src=x"));
    assert.ok(html.includes("&lt;/mi&gt;&lt;script&gt;"));
    assert.ok(html.includes('aria-label="&quot;&gt;&lt;img'));
  });

  test("unknown and malformed mathematics fails explicitly", () => {
    for (const tree of [null, {}, ...["tex", "constructor", "toString", "__proto__"].map((kind) => ({ kind })), sym(2 as unknown as string), { kind: "seq", items: [null] },
      { kind: "seq", items: "x" }, { kind: "frac", top: sym("x") }, { kind: "binder", symbol: "∀", names: null, body: sym("x") }]) {
      assert.throws(() => formula(tree), /math/i);
    }
    assert.throws(() => formula(sym("x"), ""), /accessible/i);
  });

  test("mathematical descendants share the component depth and node budgets", () => {
    let tree: unknown = sym("x");
    for (let depth = 0; depth < 66; depth++) tree = { kind: "root", body: tree };
    assert.throws(() => formula(tree), /too deep/);
    assert.throws(() => renderWorkspace({ regions: new Map([["root", { revision: 1,
      node: { kind: "math", math: { kind: "seq", items: [sym("x"), sym("y")] }, spoken: "x y" } }]]), maxNodes: 3 }), /too large/);
  });
});

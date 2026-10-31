import * as assert from "node:assert/strict";
import { renderWorkspace, type RenderOptions } from "../../client/src/osprey-ui/render";
import { resolveSymbols } from "../../client/src/osprey-ui/symbol-assets";

suite("native shared symbol assets", () => {
  test("an approved symbol uses its shared SVG instead of a host font substitute", () => {
    const regions = new Map([["root", { revision: 1, node: { kind: "icon", symbol: "moon" } }]]);
    const options = { regions, symbols: new Map([["moon", "approved://icons/moon.svg"]]) } as RenderOptions;
    const html = renderWorkspace(options);
    assert.ok(html.includes('class="osp-symbol"'), html);
    assert.ok(html.includes("approved://icons/moon.svg"), html);
    assert.ok(html.includes('aria-hidden="true"'), html);
    assert.ok(!html.includes("codicon"), "a declared source asset must not be replaced by a font icon");
  });

  test("a registry resolves each declared local SVG and preserves neutral fallback", () => {
    const paths: string[] = [];
    const symbols = resolveSymbols('{"moon":"icons/moon.svg","sun.max":"icons/sun.svg"}', path => { paths.push(path); return `approved:${path}`; });
    assert.deepStrictEqual(paths, ["icons/moon.svg", "icons/sun.svg"]);
    assert.strictEqual(symbols.get("sun.max"), "approved:icons/sun.svg");
    const html = renderWorkspace({ regions: new Map([["root", { revision: 1, node: { kind: "icon", symbol: "unknown" } }]]), symbols });
    assert.ok(html.includes("codicon-info"));
    assert.ok(!html.includes("unknown"), "unknown semantic names must not become visible text");
  });

  test("invalid inventories keys and unapproved paths fail before resolving a file", () => {
    for (const value of [null, [], {}, { moon: 1 }, { moon: "../moon.svg" }, { moon: "icons/../moon.svg" },
      { moon: "https://example.com/moon.svg" }, { moon: "icons/moon.svg?x" }, { moon: "icons/moon.png" }, { ["x".repeat(129)]: "icons/moon.svg" }]) {
      assert.throws(() => resolveSymbols(JSON.stringify(value), () => assert.fail("invalid symbol reached resolver")), /symbol/i);
    }
    assert.throws(() => resolveSymbols("{", () => ""), /symbol.*JSON/i);
  });

  test("registry byte and entry limits and declared missing files fail loudly", () => {
    assert.throws(() => resolveSymbols(" ".repeat(65537), () => ""), /64 KiB/);
    const excessive = Object.fromEntries(Array.from({ length: 129 }, (_, index) => [`symbol${index}`, "icons/moon.svg"]));
    assert.throws(() => resolveSymbols(JSON.stringify(excessive), () => ""), /inventory/);
    assert.throws(() => resolveSymbols('{"moon":"icons/moon.svg"}', () => { throw new Error("Missing official SVG"); }), /Missing official SVG/);
    assert.throws(() => resolveSymbols('{"moon":"icons/moon.svg"}', () => ""), /Missing symbol asset/);
  });
});

import * as assert from "node:assert/strict";
import { renderWorkspace } from "../../client/src/osprey-ui/render";

// Restored from the JSON-protocol host, told rows edition: the board renders from its host-held
// slices exactly as it did over the envelope; strokes and lassos now cross as typed events, so
// the old parseUiEvent suite is gone with the envelope it checked.

const stroke = { points: [[12, 20], [18, 24]], width: 3, at: 42 };
const state = { version: 1, notes: "", strokes: [stroke], size: { width: 1536, height: 1024 }, view: { x: 40, y: -12, scale: 150 } };
const slices = () => {
  const { strokes, ...metadata } = state;
  return new Map([["working-board", JSON.stringify(metadata)], ...strokes.map((stroke, index): [string, string] => [`working-board:${index}`, JSON.stringify(stroke)])]);
};
const regions = new Map([["root", { revision: 3, node: { kind: "board", slice: "working-board", count: 1, lasso: true, event: 4, lassoEvent: 5 } }]]);

suite("native board host", () => {
  test("an approved decorative board backdrop renders without weakening standalone media labels", () => {
    const node = { kind: "board", slice: "working-board", count: 1, lasso: true, event: 4, lassoEvent: 5, backdrop: "chalk-study.png" };
    const html = renderWorkspace({ regions: new Map([["root", { revision: 3, node }]]), slices: slices(), resolveAsset: source => `https://assets.example.test/${source}` });
    assert.ok(html.includes('src="https://assets.example.test/chalk-study.png"'));
    assert.ok(html.includes('alt="" aria-hidden="true"'));
    assert.ok(html.includes('role="img" aria-label="Drawing board"'));
    assert.throws(() => renderWorkspace({ regions: new Map([["root", { revision: 1, node: {
      kind: "image", source: "chalk-study.png", alt: "", width: 1536, height: 1024,
    } }]]), resolveAsset: source => `https://assets.example.test/${source}` }), /alt/);
  });
  test("the board draws from a host-held slice with both issued bindings", () => {
    const html = renderWorkspace({ regions, slices: slices() });
    assert.ok(html.includes("<canvas"));
    assert.ok(html.includes('data-board-slice="working-board"'));
    assert.ok(html.includes('data-event-ordinal="4"'));
    assert.ok(html.includes('data-lasso-ordinal="5"'));
    assert.ok(html.includes("&quot;scale&quot;:150"));
    assert.ok(!html.includes("Checked"));
  });

  test("missing or malformed drawing data fails visibly instead of losing working", () => {
    for (const slices of [new Map(), new Map([["working-board", "not JSON"]]), new Map([["working-board", "{}"]]),
      new Map([["working-board", JSON.stringify({ ...state, strokes: [] })]])]) {
      assert.throws(() => renderWorkspace({ regions, slices }), /board/i);
    }
  });
});

import * as assert from "node:assert/strict";
import { renderWorkspace } from "../../client/src/osprey-ui/render";
import { parseUiEvent } from "../../client/src/osprey-ui/protocol";

const stroke = { points: [[12, 20], [18, 24]], width: 3, at: 42 };
const state = { version: 1, notes: "", strokes: [stroke], size: { width: 1536, height: 1024 }, view: { x: 40, y: -12, scale: 150 } };
const slices = () => {
  const { strokes, ...metadata } = state;
  return new Map([["working-board", JSON.stringify(metadata)], ...strokes.map((stroke, index): [string, string] => [`working-board:${index}`, JSON.stringify(stroke)])]);
};
const regions = new Map([["root", { revision: 3, node: { kind: "board", slice: "working-board", count: 1, lasso: true, event: 4, lassoEvent: 5 } }]]);

suite("native board host", () => {
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

  test("stroke and lasso values reach the existing typed event boundary", () => {
    const event = { type: "event", region: "root", revision: 3, ordinal: 4, value: stroke };
    assert.deepStrictEqual(parseUiEvent(event, regions, 8)?.value, stroke);
    assert.deepStrictEqual(parseUiEvent({ ...event, ordinal: 5, value: [0] }, regions, 8)?.value, [0]);
    for (const value of [{ points: [[1, 2, 3]], width: 3, at: 42 }, { ...stroke, width: 0 }, { ...stroke, at: -1 }, [-1], [0.5]]) {
      assert.equal(parseUiEvent({ ...event, value }, regions, 8), undefined);
    }
  });
});

import * as assert from "node:assert/strict";
import { renderWorkspace } from "../../client/src/osprey-ui/render";
import { encloseEvent, lassoEvent, pointEvent, strokeEvent, wordsOf } from "../../client/src/osprey-ui/webview/events";
import { parseTouch } from "../../client/src/osprey-ui/session";
import { sourcesOf } from "../../client/src/osprey-ui/nodes";

// Restored from the JSON-protocol host, told rows edition: the board renders from its host-held
// slices exactly as it did over the envelope; strokes and lassos cross as typed events now.

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

    // The session asks the host to approve exactly the backdrops a frame names, once each.
    const board = (backdrop: unknown) => ({ ...node, backdrop });
    const named = sourcesOf({ theme: { kind: "board", backdrop: "theme-paper.png" }, regions: [
      { key: "root", node: { kind: "stack", children: [node, board("chalk-study.png"), board(""), board(7), { ...node, kind: "canvas" }, null, "chalk.png"] } },
      { key: "aside", node: [board("graph-paper.png"), { kind: "image", source: "chalk-study.png", alt: "Chalk study" }] },
    ] });
    assert.deepStrictEqual(named, ["theme-paper.png", "chalk-study.png", "graph-paper.png"],
      "only non-empty board backdrops are asset sources; empty or non-text ones, and other kinds' backdrop fields, name nothing");
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

  test("stroke and lasso values reach the existing typed event boundary", () => {
    // A finished stroke crosses as kind 5 with its width in x and its clock in y; each point of
    // the stroke being drawn crosses as kind 4; a lasso as kinds 6 and 7 ([NATIVE-SHM-EVENT]).
    assert.deepStrictEqual(strokeEvent(4, 3, 42), { type: "event", kind: 5, ordinal: 4, words: "", detail: "", x: 3, y: 42, down: false, shift: false });
    assert.deepStrictEqual(pointEvent(12, 20), { type: "event", kind: 4, ordinal: 0, words: "", detail: "", x: 12, y: 20, down: false, shift: false });
    assert.deepStrictEqual(encloseEvent(1), { ...encloseEvent(0), ordinal: 1, kind: 6 });
    assert.deepStrictEqual(lassoEvent(5), { ...pointEvent(0, 0), kind: 7, ordinal: 5 });
    // The typed event boundary refuses what cannot cross: unknown kinds, fractional ordinals and
    // non-finite positions never reach the application as guessed values.
    for (const bad of [{ type: "event", kind: 12 }, { type: "event", kind: 4, ordinal: -1 }, { type: "event", kind: 4, ordinal: 0.5 },
      { type: "event", kind: 4, x: Number.NaN }, { type: "event", kind: 5, words: 7 }, { type: "event", kind: 5, down: "yes" }]) {
      assert.strictEqual(parseTouch(bad), undefined, JSON.stringify(bad));
    }
    assert.deepStrictEqual(parseTouch({ type: "event", kind: 5, ordinal: 4, x: 3, y: 42 }),
      { kind: 5, ordinal: 4, words: "", detail: "", x: 3, y: 42, down: false, shift: false });
  });

  test("stroke and lasso values reach the existing typed event boundary (extra: a camera pan crosses as a control event in words)", () => {
    // The board's pan binding delivers its final camera offsets as the control's words, exactly as
    // a switch delivers its next state; the words encoder keeps only real values.
    const pan = parseTouch({ type: "event", kind: 0, ordinal: 6, words: "40,-12" });
    assert.deepStrictEqual([pan?.kind, pan?.ordinal, pan?.words], [0, 6, "40,-12"]);
    for (const [value, words] of [[null, ""], [true, "true"], [7, "7"], [Number.NaN, ""], ["p ∧ q", "p ∧ q"]] as const) {
      assert.strictEqual(wordsOf(value), words);
    }
  });
});

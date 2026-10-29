import * as assert from "node:assert/strict";
import { renderWorkspace } from "../../client/src/osprey-ui/render";

const editor = (extra: Record<string, unknown> = {}): string => renderWorkspace({
  regions: new Map([["root", { revision: 3, node: { kind: "editor", label: "Argument", value: "Assume h : p ∧ q.\nFrom hp.", cursor: 0,
    marks: [{ line: 2, underlined: true, tone: "danger", badge: "Needs attention", note: "The goal is q; this step establishes p." }],
    event: 4, placeEvent: 5, ...extra } }]]),
});

suite("native marked editor", () => {
  test("the Osprey marks retain their line, tone and explanatory text", () => {
    const html = editor();
    assert.ok(html.includes("<textarea"));
    assert.ok(html.includes('data-place-ordinal="5"'));
    assert.ok(html.includes('data-event-ordinal="4"'));
    assert.ok(html.includes('data-mark-line="2"'));
    assert.ok(html.includes("osp-editor-underlined osp-tone-danger"));
    assert.ok(html.includes("Needs attention") && html.includes("The goal is q; this step establishes p."));
    assert.ok(html.includes("aria-describedby="));
  });

  test("source text, labels, badges and notes cannot inject markup", () => {
    const html = editor({ label: '"><script>label</script>', value: "</textarea><script>source</script>",
      marks: [{ line: 1, underlined: true, tone: 'danger" onclick="bad', badge: "<img src=x>", note: "<script>note</script>" }] });
    assert.ok(!html.includes("<script>") && !html.includes("<img src=x>"));
    assert.ok(html.includes("&lt;/textarea&gt;"));
    assert.ok(!html.includes('onclick="bad'));
  });

  test("malformed editor values and marks fail explicitly", () => {
    for (const extra of [{ value: 1 }, { cursor: -1 }, { cursor: 0.5 }, { marks: null },
      { marks: [{ line: 0, underlined: true, tone: "danger", badge: "", note: "" }] }]) {
      assert.throws(() => editor(extra), /editor/i);
    }
  });
});

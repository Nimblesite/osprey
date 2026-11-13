import * as assert from "node:assert/strict";
import { renderWorkspace } from "../../client/src/osprey-ui/render";

// Restored from the JSON-protocol host, told rows edition: the pure renderer suites. The focus
// and typing suites that shared this file drove a live webview through the windowed extension
// host and stay with it.

const editorNode = (extra: Record<string, unknown> = {}) => ({ kind: "editor", label: "Argument", value: "Assume h : p ∧ q.\nFrom hp.", cursor: 0,
  marks: [{ line: 2, underlined: true, tone: "danger", badge: "Needs attention", note: "The goal is q; this step establishes p." }],
  event: 4, placeEvent: 5, ...extra });
const editor = (extra: Record<string, unknown> = {}): string => renderWorkspace({
  regions: new Map([["root", { revision: 3, node: editorNode(extra) }]]),
});

suite("native marked editor", () => {
  test("focus epochs preserve an absent field as zero and encode explicit requests", () => {
    assert.ok(editor().includes('data-focus-epoch="0"'));
    for (const focusEpoch of [0, 1, Number.MAX_SAFE_INTEGER]) {
      assert.ok(editor({ focusEpoch }).includes(`data-focus-epoch="${focusEpoch}"`));
    }
  });

  test("malformed focus epochs are refused rather than becoming focus requests", () => {
    for (const focusEpoch of [null, -1, 0.5, Number.MAX_SAFE_INTEGER + 1, "1", true]) {
      assert.throws(() => editor({ focusEpoch }), /focusEpoch/);
    }
  });

  test("the Osprey marks retain their line, tone and explanatory text", () => {
    const html = editor();
    for (const mark of [
      "<textarea",
      'data-place-ordinal="5"',
      'data-event-ordinal="4"',
      'data-mark-line="2"',
      "osp-editor-underlined osp-tone-danger",
      "Needs attention",
      "The goal is q; this step establishes p.",
      "aria-describedby=",
    ]) {
      assert.ok(html.includes(mark), mark);
    }
  });

  test("source text, labels, badges and notes cannot inject markup", () => {
    const html = editor({ label: '"><script>label</script>', value: "</textarea><script>source</script>",
      marks: [{ line: 1, underlined: true, tone: 'danger" onclick="bad', badge: "<img src=x>", note: "<script>note</script>" }] });
    for (const injected of ["<script>", "<img src=x>", 'onclick="bad']) {
      assert.ok(!html.includes(injected), injected);
    }
    assert.ok(html.includes("&lt;/textarea&gt;"));
  });

  test("malformed editor values and marks fail explicitly", () => {
    for (const extra of [{ value: 1 }, { cursor: -1 }, { cursor: 0.5 }, { marks: null },
      { marks: [{ line: 0, underlined: true, tone: "danger", badge: "", note: "" }] }]) {
      assert.throws(() => editor(extra), /editor/i);
    }
  });
});

import * as assert from "node:assert/strict";
import { randomBytes } from "node:crypto";
import { ViewColumn, window } from "vscode";
import type { Frame } from "playwright-core";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { renderWorkspace } from "../../client/src/osprey-ui/render";
import { record } from "../../client/src/osprey-ui/protocol";
import { workspaceHtml } from "../../client/src/osprey-ui/webview-html";
import { connectEditorBrowser, editorWebviewFrame } from "./profiler-kit";

const editorNode = (extra: Record<string, unknown> = {}) => ({ kind: "editor", label: "Argument", value: "Assume h : p ∧ q.\nFrom hp.", cursor: 0,
  marks: [{ line: 2, underlined: true, tone: "danger", badge: "Needs attention", note: "The goal is q; this step establishes p." }],
  event: 4, placeEvent: 5, ...extra });
const editor = (extra: Record<string, unknown> = {}): string => renderWorkspace({
  regions: new Map([["root", { revision: 3, node: editorNode(extra) }]]),
});

type EditorDialog = "none" | "outside" | "inside";
type EditorUpdate = (epoch: number, value?: string, shown?: boolean, dialog?: EditorDialog, region?: string) => Promise<void>;

function editorFrame(epoch: number, value: string, shown: boolean, revision: number, dialog: EditorDialog, region: string): string {
  const field = editorNode({ focusEpoch: epoch, value, cursor: Buffer.byteLength(value), marks: [] });
  const content = region === "root" ? field : { kind: "slot", key: region };
  const fields = shown && dialog !== "inside" ? [content] : [];
  const button = { kind: "button", label: "Insert symbol", symbol: "", role: "plain", enabled: true, event: 6 };
  const keep = { ...button, label: "Keep dialog", event: 8 };
  const overlay = dialog === "none" ? [] : [{ kind: "dialog", title: "Focus boundary", event: 7,
    body: { kind: "stack", axis: "vertical", children: dialog === "inside" ? [keep, content] : [keep] } }];
  const regions = new Map<string, { revision: number; node: unknown }>([["root", { revision, node: { kind: "stack", axis: "vertical", children: [...fields, button, ...overlay] } }]]);
  if (shown && region !== "root") regions.set(region, { revision, node: field });
  return renderWorkspace({ regions });
}

async function withEditorView(run: (frame: Frame, update: EditorUpdate, events: Record<string, unknown>[]) => Promise<void>): Promise<void> {
  const browser = await connectEditorBrowser(), events: Record<string, unknown>[] = [];
  const panel = window.createWebviewPanel("osprey.editor.focus", "Editor focus contract", ViewColumn.One, { enableScripts: true });
  let revision = 0;
  const update: EditorUpdate = async (epoch, value = "p", shown = true, dialog = "none", region = "root") => {
    await panel.webview.postMessage({ type: "render", html: editorFrame(epoch, value, shown, ++revision, dialog, region), diagnostics: [] });
  };
  const listener = panel.webview.onDidReceiveMessage((raw: unknown) => {
    if (!record(raw)) return;
    if (raw.type === "ready") void update(5);
    if (raw.type === "event") events.push(raw);
  });
  try {
    panel.webview.html = workspaceHtml({ nonce: randomBytes(16).toString("hex"), assetUri: "", cspSource: panel.webview.cspSource });
    await run(await editorWebviewFrame(browser, 'textarea[aria-label="Argument"]'), update, events);
  } finally { listener.dispose(); panel.dispose(); await browser.close(); }
}

async function waitEditorFrame(frame: Frame, epoch: number, value: string): Promise<void> {
  await frame.waitForFunction(({ epoch, value }) => {
    const field = document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Argument"]');
    return field?.value === value && field.dataset.focusEpoch === String(epoch);
  }, { epoch, value }, { timeout: 1000 });
}

suite("native marked editor", () => {
  test("qualified challenge regions baseline separately and retain the new challenge's first request", async () => {
    await withEditorView(async (frame, update, events) => {
      const field = frame.getByRole("textbox", { name: "Argument", exact: true });
      await frame.getByRole("button", { name: "Insert symbol", exact: true }).focus();
      await update(8, "lesson A", true, "none", "challenge:lesson-a/claim");
      await waitEditorFrame(frame, 8, "lesson A");
      assert.ok(!await field.evaluate(node => node === document.activeElement));
      await update(0, "lesson B", true, "none", "challenge:lesson-b/claim");
      await waitEditorFrame(frame, 0, "lesson B");
      assert.ok(!await field.evaluate(node => node === document.activeElement));
      await update(1, "q → ", true, "none", "challenge:lesson-b/claim");
      await waitEditorFrame(frame, 1, "q → ");
      await frame.waitForFunction(() => document.activeElement?.getAttribute("aria-label") === "Argument", undefined, { timeout: 1000 });
      await frame.page().keyboard.type("p");
      await pollUntilResult({ fn: () => events.some(event => event.value === "q → p" && event.region === "challenge:lesson-b/claim" && event.ordinal === 4),
        until: Boolean, timeoutMs: 1000, description: "the first request of the newly scoped challenge retains its own binding" });
    });
  });

  test("focus requests respect the modal boundary and work inside the dialog", async () => {
    await withEditorView(async (frame, update) => {
      await frame.getByRole("button", { name: "Insert symbol", exact: true }).focus();
      await update(6, "outside", true, "outside");
      await waitEditorFrame(frame, 6, "outside");
      assert.ok(await frame.evaluate(() => !!document.activeElement?.closest("[data-native-dialog]")), "background editor focus must not escape a modal");
      await update(7, "inside", true, "inside");
      await waitEditorFrame(frame, 7, "inside");
      await frame.waitForFunction(() => document.activeElement?.getAttribute("aria-label") === "Argument", undefined, { timeout: 1000 });
    });
  });

  test("inactive views never reclaim focus for text updates or epoch advances", async () => {
    await withEditorView(async (frame, update) => {
      const parent = frame.parentFrame();
      assert.ok(parent, "the fixture must provide a real focus target outside the webview");
      await frame.getByRole("textbox", { name: "Argument", exact: true }).focus();
      await parent.evaluate(() => {
        const other = document.createElement("input");
        other.id = "osprey-test-outside-focus"; document.body.appendChild(other); other.focus();
      });
      try {
        assert.ok(!await frame.evaluate(() => document.hasFocus()), "establish an actually inactive webview before testing");
        for (const [epoch, modal] of [[5, "none"], [6, "none"], [7, "outside"], [8, "none"]] as const) {
          await update(epoch, `background ${epoch}`, true, modal);
          await waitEditorFrame(frame, epoch, `background ${epoch}`);
          assert.ok(!await frame.evaluate(() => document.hasFocus()), "a background update must not focus the webview");
          assert.strictEqual(await parent.evaluate(() => document.activeElement?.id), "osprey-test-outside-focus");
        }
      } finally { await parent.evaluate(() => document.getElementById("osprey-test-outside-focus")?.remove()); }
    });
  });

  test("an authored epoch advance returns focus for continued Unicode typing", async () => {
    await withEditorView(async (frame, update, events) => {
      await frame.getByRole("button", { name: "Insert symbol", exact: true }).click();
      await update(6, "p → ");
      await waitEditorFrame(frame, 6, "p → ");
      await frame.waitForFunction(() => document.activeElement?.getAttribute("aria-label") === "Argument", undefined, { timeout: 1000 });
      await frame.page().keyboard.type("r");
      assert.strictEqual(await frame.getByRole("textbox", { name: "Argument", exact: true }).inputValue(), "p → r");
      await pollUntilResult({ fn: () => events.some(event => event.value === "p → r" && event.region === "root" && event.revision === 2 && event.ordinal === 4), until: Boolean,
        timeoutMs: 1000, description: "continued typing uses the newly issued editor binding" });
    });
  });

  test("baseline ordinary patches and reintroduced editors never steal focus", async () => {
    await withEditorView(async (frame, update) => {
      const field = frame.getByRole("textbox", { name: "Argument", exact: true });
      assert.ok(!await field.evaluate(node => node === document.activeElement));
      await frame.getByRole("button", { name: "Insert symbol", exact: true }).focus();
      for (const [at, epoch] of [5, 4, 4, 5].entries()) {
        await update(epoch, `ordinary ${at}:${epoch}`);
        await waitEditorFrame(frame, epoch, `ordinary ${at}:${epoch}`);
        assert.ok(!await field.evaluate(node => node === document.activeElement));
      }
      await update(10, "", false);
      await field.waitFor({ state: "detached" });
      await update(100, "reintroduced");
      await waitEditorFrame(frame, 100, "reintroduced");
      assert.ok(!await field.evaluate(node => node === document.activeElement));
    });
  });

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

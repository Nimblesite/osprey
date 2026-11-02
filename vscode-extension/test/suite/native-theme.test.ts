import * as assert from "node:assert/strict";
import * as sinon from "sinon";
import { window, type WebviewPanel } from "vscode";
import type { Frame } from "playwright-core";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import * as NativeRendering from "../../client/src/osprey-ui/render";
import * as WebviewHtml from "../../client/src/osprey-ui/webview-html";
import * as Wasi from "../../client/src/osprey-ui/wasi";
import { withWorkbench } from "./workbench-browser-kit";
import { COMMAND_MS } from "./profiler-kit";
import { themeStyle } from "../../client/src/osprey-ui/theme";

suite("native application theme", () => {
  test("the application owns surfaces, typography colours and source roles", () => {
    const css = themeStyle({ tokens: [
      { name: "canvas", light: "#FAFAF7", dark: "#202723" },
      { name: "primary", light: "#242923", dark: "#ECEDE5" },
      { name: "sourceKeyword", light: "#794A64", dark: "#D5B0C7" },
      { name: "dangerSurface", light: "#F6E8E3", dark: "#453330" },
    ], feedback: 140, settle: 240 });
    assert.ok(css.includes("--osp-canvas: light-dark(#FAFAF7, #202723);"));
    assert.ok(css.includes("--osp-text: light-dark(#242923, #ECEDE5);"));
    assert.ok(css.includes("--osp-source-keyword: light-dark(#794A64, #D5B0C7);"));
    assert.ok(css.includes("--osp-danger-surface: light-dark(#F6E8E3, #453330);"));
    assert.ok(css.includes("--osp-motion:140ms;") && css.includes("--osp-settle:240ms;"));
  });

  test("malformed names, colours and motion cannot become style declarations", () => {
    const css = themeStyle({ tokens: [
      { name: "canvas;}body{color:red", light: "#FFFFFF", dark: "#000000" },
      { name: "canvas", light: "url(https://example.test)", dark: "#000000" },
      { name: "sourceKeyword", light: "#794A64", dark: "#D5B0C7" },
    ], feedback: "0;}body{color:red", settle: -1 });
    assert.ok(!css.includes("body") && !css.includes("url("));
    assert.ok(!css.includes("--osp-motion:") && !css.includes("--osp-settle:"));
    assert.ok(css.includes("--osp-source-keyword:"));
    assert.equal(themeStyle(null), "");
  });

  test("brand properties remain available and empty themes retain host defaults", () => {
    assert.ok(themeStyle({ tokens: [{ name: "accent", light: "#006B86", dark: "#77D7F4" }] })
      .includes("--osp-brand-accent: light-dark(#006B86, #77D7F4);"));
    assert.equal(themeStyle({ tokens: [] }), "");
  });
});

type GestureEnd = "commit" | "off" | "outside" | "unrelated" | "primary-touch" | "cancel" | "fatal";
const gestureRuns: { started: number; title: boolean; finished?: number }[] = [];

function observeGestureRuntime(): void {
  gestureRuns.length = 0;
  const actual = Wasi.runWasi;
  sinon.stub(Wasi, "runWasi").callsFake(async (module, input, options) => {
    const run = { started: Date.now(), title: input.includes("Keep held gesture"), finished: undefined as number | undefined };
    gestureRuns.push(run);
    try { return await actual(module, input, options); }
    finally { run.finished = Date.now(); }
  });
}

function observeGestureMessages(): void {
  observeGestureRuntime();
  const actual = WebviewHtml.workspaceHtml;
  sinon.stub(WebviewHtml, "workspaceHtml").callsFake(options => {
    const html = actual(options), boundary = "const vscode = acquireVsCodeApi();";
    assert.ok(html.includes(boundary), "observe the real host API without replacing its event delivery");
    return html.replace(boundary, String.raw`const api = acquireVsCodeApi();
      window.ospreyGestureMessages = []; window.ospreyGestureInbox = []; window.ospreyGestureErrors = [];
      window.addEventListener("message", function (event) { window.ospreyGestureInbox.push({ at: Date.now(), type: event.data && event.data.type, active: event.data && event.data.active }); });
      window.addEventListener("error", function (event) { window.ospreyGestureErrors.push(String(event.message)); });
      const vscode = Object.assign({}, api, { postMessage: function (message) { window.ospreyGestureMessages.push(Object.assign({ recordedAt: Date.now() }, message)); return api.postMessage(message); } });`);
  });
}

// The title reaches the host only when the webview's 160 ms edit timer fires, and a
// loaded CI runner can hold that timer back past a second.
async function heldTitleRender(frame: Frame): Promise<void> {
  const awaitedAt = Date.now();
  try {
    await pollUntilResult({ fn: () => (NativeRendering.renderWorkspace as sinon.SinonSpy).getCalls().some(call =>
      JSON.stringify([...call.args[0].regions]).includes("Keep held gesture")), until: Boolean, timeoutMs: COMMAND_MS, description: "the real title command renders while the appearance button is pressed" });
  } catch (error) {
    const snapshot = await frame.evaluate(() => {
      const title = document.querySelector<HTMLInputElement>('input[aria-label="Solution title"]');
      const recorded = document.defaultView as unknown as { ospreyGestureMessages: unknown[]; ospreyGestureInbox: unknown[]; ospreyGestureErrors: unknown[] };
      return { messages: recorded.ospreyGestureMessages, inbox: recorded.ospreyGestureInbox, errors: recorded.ospreyGestureErrors,
        visibility: document.visibilityState, focused: document.hasFocus(), busy: document.getElementById("osprey-root")?.getAttribute("aria-busy"),
        draft: title?.value, committed: title?.getAttribute("data-value"), active: document.activeElement?.getAttribute("aria-label"),
        theme: document.querySelector<HTMLButtonElement>('button[aria-label="Switch appearance"]')?.disabled };
    });
    console.error("Held title timing", JSON.stringify({ awaitedAt, observed: Date.now(), runs: gestureRuns, snapshot }));
    throw error;
  }
  await frame.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => resolve())));
}

async function interruptGesture(frame: Frame, end: GestureEnd): Promise<void> {
  if (end === "primary-touch") await frame.getByRole("button", { name: "Switch appearance", exact: true }).evaluate(node => {
    for (const type of ["pointerdown", "pointerup", "click"]) node.dispatchEvent(new PointerEvent(type, { pointerId: 999, pointerType: "touch", isPrimary: true, button: 0, detail: type === "click" ? 1 : 0, bubbles: true }));
  });
  if (end === "unrelated") {
    await frame.evaluate(() => document.dispatchEvent(new PointerEvent("pointerup", { pointerId: 999, isPrimary: false, bubbles: true })));
    await frame.evaluate(() => document.dispatchEvent(new PointerEvent("pointercancel", { pointerId: 999, isPrimary: false, bubbles: true })));
    await frame.evaluate(() => new Promise<void>(resolve => setTimeout(resolve, 0)));
  }
  if (end === "cancel") await frame.evaluate(() => document.dispatchEvent(new PointerEvent("pointercancel", { pointerId: 1, isPrimary: true, bubbles: true })));
  if (end === "fatal") {
    const panel = (window.createWebviewPanel as sinon.SinonSpy).lastCall.returnValue as WebviewPanel;
    await panel.webview.postMessage({ type: "fatal", message: "Held gesture fatal sentinel" });
    await frame.getByText("Held gesture fatal sentinel", { exact: true }).waitFor();
  }
}

async function pendingTitleGesture(frame: Frame, end: GestureEnd = "commit"): Promise<void> {
  const title = frame.getByRole("textbox", { name: "Solution title", exact: true });
  const theme = frame.getByRole("button", { name: "Switch appearance", exact: true }), bounds = await theme.boundingBox();
  assert.ok(bounds);
  await title.fill("Keep held gesture");
  await frame.page().mouse.move(bounds.x + (end === "cancel" || end === "fatal" ? 1 : bounds.width / 2), bounds.y + bounds.height / 2);
  await frame.page().mouse.down();
  try { await heldTitleRender(frame); await interruptGesture(frame, end); }
  finally { await finishGesture(frame, end, bounds); }
  await assertGestureApplied(frame, end);
}

async function finishGesture(frame: Frame, end: GestureEnd, bounds: { x: number; y: number; width: number; height: number }): Promise<void> {
  const current = end === "fatal" ? await frame.getByRole("button", { name: "Switch appearance", exact: true }).boundingBox() : bounds;
  assert.ok(current);
  if (end === "outside") await frame.page().mouse.move(0, 0);
  if (end === "off") await frame.page().mouse.move(bounds.x - 30, bounds.y + 60);
  if (end === "fatal") await frame.page().mouse.move(current.x + 1, current.y + current.height / 2);
  await frame.page().mouse.up();
}

async function assertGestureApplied(frame: Frame, end: GestureEnd): Promise<void> {
  const title = frame.getByRole("textbox", { name: "Solution title", exact: true });
  const theme = frame.getByRole("button", { name: "Switch appearance", exact: true });
  await frame.waitForFunction(() => document.getElementById("osprey-root")?.getAttribute("aria-busy") === "false", undefined, { timeout: COMMAND_MS });
  const actions = await gestureActions(frame);
  assert.strictEqual(actions.length, end === "commit" || end === "unrelated" ? 1 : 0, "the gesture must emit exactly its owned action");
  await theme.locator(end === "commit" || end === "unrelated" ? '[data-symbol="sun.max"]' : '[data-symbol="moon"]').waitFor();
  await pollUntilResult({ fn: () => title.getAttribute("data-value"), until: value => value === "Keep held gesture", timeoutMs: COMMAND_MS, description: "the frame renders the committed title while its button is held" });
  assert.strictEqual(await title.inputValue(), "Keep held gesture");
  if (end === "fatal") {
    assert.strictEqual(await theme.getAttribute("data-event-revision"), await title.getAttribute("data-event-revision"), "fatal cancellation must leave the latest committed control binding usable");
    await frame.getByText("Held gesture fatal sentinel", { exact: true }).waitFor();
    await assertFatalRecovery(frame);
  }
}

async function gestureActions(frame: Frame): Promise<Record<string, unknown>[]> {
  return frame.evaluate(() => (document.defaultView as unknown as { ospreyGestureMessages: Record<string, unknown>[] }).ospreyGestureMessages
    .filter(message => message.type === "event" && message.value === null));
}

async function assertFatalRecovery(frame: Frame): Promise<void> {
  const theme = frame.getByRole("button", { name: "Switch appearance", exact: true });
  const region = await theme.getAttribute("data-event-region"), revision = Number(await theme.getAttribute("data-event-revision")), ordinal = Number(await theme.getAttribute("data-event-ordinal"));
  await theme.click();
  await theme.locator('[data-symbol="sun.max"]').waitFor();
  const actions = await gestureActions(frame);
  assert.strictEqual(actions.length, 1, "a fresh gesture after fatal cancellation must emit exactly one action");
  assert.deepStrictEqual({ region: actions[0].region, revision: actions[0].revision, ordinal: actions[0].ordinal }, { region, revision, ordinal });
}

async function liveFrameWhilePressed(frame: Frame, end: "commit" | "primary-touch" | "fatal" = "commit"): Promise<void> {
  const theme = frame.getByRole("button", { name: "Switch appearance", exact: true });
  const title = frame.getByRole("textbox", { name: "Solution title", exact: true });
  const held = await theme.elementHandle(), bounds = await theme.boundingBox();
  assert.ok(held && bounds);
  const issued = await held.evaluate(node => ["data-event-region", "data-event-revision", "data-event-ordinal"].map(key => node.getAttribute(key)));
  await frame.page().mouse.move(bounds.x + (end === "fatal" ? 1 : bounds.width / 2), bounds.y + bounds.height / 2);
  await frame.page().mouse.down();
  try {
    await title.fill("Keep held gesture");
    await heldTitleRender(frame);
    if (end === "primary-touch") await interruptGesture(frame, end);
    assert.ok(await held.evaluate(node => node.isConnected), "the pressed DOM node must survive the actual render");
    assert.deepStrictEqual(await held.evaluate(node => ["data-event-region", "data-event-revision", "data-event-ordinal"].map(key => node.getAttribute(key))), issued);
    assert.strictEqual(await title.getAttribute("data-value"), "Keep held gesture", "the rest of the frame must update before release");
    if (end === "fatal") await interruptGesture(frame, end);
  } finally { await finishGesture(frame, end, bounds); }
  await assertGestureApplied(frame, end === "primary-touch" ? "commit" : end);
}

suite("workbench VSIX browser gestures", () => {
  setup(observeGestureMessages);
  teardown(() => sinon.restore());
  test("a live render retains the pressed DOM node and its exact original binding", async () => {
    await withWorkbench(frame => liveFrameWhilePressed(frame), "failure-held-node-live-frame");
  });
  test("a primary touch cannot replace the mouse gesture's held binding", async () => {
    await withWorkbench(frame => liveFrameWhilePressed(frame, "primary-touch"), "failure-secondary-primary-pointer");
  });
  test("pending title gesture handles fatal without losing or reordering its render", async () => {
    await withWorkbench(frame => liveFrameWhilePressed(frame, "fatal"), "failure-title-gesture-fatal");
  });
  for (const end of ["outside", "unrelated", "cancel"] as const) test(`pending title gesture handles ${end} without losing or reordering its render`, async () => {
    await withWorkbench(frame => pendingTitleGesture(frame, end), `failure-title-gesture-${end}`);
  });
  test("a pending title gesture preserves the appearance action across its render", async () => {
    await withWorkbench(frame => pendingTitleGesture(frame), "failure-pending-title-gesture");
  });
  test("a pending title gesture dragged off its button leaves the appearance unchanged", async () => {
    await withWorkbench(frame => pendingTitleGesture(frame, "off"), "failure-cancelled-title-gesture");
  });
});

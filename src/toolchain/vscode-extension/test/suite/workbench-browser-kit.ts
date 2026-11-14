// The real Workbench in the test window's webview, driven through Playwright over the editor's
// debugging port ([WORKBENCH-VSIX] [NATIVE-SHM-WASM]). The application's own runtime is wrapped,
// never replaced: a test can count the events that reach it and hold a call to watch the
// webview while it waits.
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as sinon from "sinon";
import type { Frame } from "playwright-core";
import { ColorThemeKind, commands, window, workspace, type WebviewPanel } from "vscode";
import * as Runner from "../../client/src/osprey-ui/wasi-runner";
import type { NativeTouch } from "../../client/src/osprey-ui/wasi-runner";
import { activateWithServer, connectEditorBrowser, editorWebviewFrame, press, waitForIdleWorkbench } from "./profiler-kit";

export const COMMAND = "osprey.workbench.open";
export const ARGUMENT = "Theorem and_swap. For all p q, p ∧ q → q ∧ p.\nProof.\nAssume h : p ∧ q.\nBy cases on h, as hp and hq.\nIt suffices to show q and p.\nFrom hq.\nFrom hp.";
const FREEHAND_CONTROLS = ["Draw", "Pan", "Pan board", "Clear", "Undo", "Board", "Show working board", "Preview sample transcription"];

/** The events that reached the live application, and a gate a test may close on them. */
export class RuntimeProbe {
  readonly events: NativeTouch[] = [];
  private gate: Promise<void> = Promise.resolve();

  /** Holds every later event until the returned function is called. */
  hold(): () => void {
    let release = (): void => undefined;
    this.gate = new Promise<void>((resolve) => { release = resolve; });
    return () => { this.gate = Promise.resolve(); release(); };
  }

  /** Waits at the gate, then records `touch`. */
  async pass(touch: NativeTouch): Promise<void> {
    await this.gate;
    this.events.push(touch);
  }
}

/** Wraps the real runtime the panel starts, so `probe` sees each event before the instance does. */
function observeRuntime(probe: RuntimeProbe): void {
  const actual = Runner.startNativeRuntime;
  sinon.stub(Runner, "startNativeRuntime").callsFake((module, signal) => {
    const live = actual(module, signal);
    return { ...live, event: async (touch) => { await probe.pass(touch); return live.event(touch); } };
  });
}

export interface Bench {
  readonly frame: Frame;
  readonly runtime: RuntimeProbe;
  /** How many webview panels the Workbench command created. */
  opened(): number;
}

/** Sizes the webview's content to `size` CSS pixels. */
export async function viewport(frame: Frame, size = { width: 1488, height: 1058 }): Promise<void> {
  const page = frame.page();
  for (let attempt = 0; attempt < 3; attempt++) {
    const shell = await page.evaluate(() => ({ width: innerWidth, height: innerHeight }));
    const content = await frame.evaluate(() => ({ width: innerWidth, height: innerHeight }));
    if (content.width === size.width && content.height === size.height) break;
    await page.setViewportSize({ width: shell.width + size.width - content.width, height: shell.height + size.height - content.height });
    await frame.waitForFunction(({ width, height }) => innerWidth !== width || innerHeight !== height, content, { timeout: 1000 }).catch(() => undefined);
  }
  assert.deepStrictEqual(await frame.evaluate(() => [innerWidth, innerHeight]), [size.width, size.height]);
}

/** Opens the Workbench with no project in `appearance`, runs `run` against it, and closes it. */
export async function withWorkbench(run: (bench: Bench) => Promise<void>, failure: string, appearance = ColorThemeKind.Light): Promise<void> {
  sinon.stub(window, "activeColorTheme").get(() => ({ kind: appearance }));
  const runtime = new RuntimeProbe();
  observeRuntime(runtime);
  await activateWithServer();
  assert.strictEqual(workspace.workspaceFolders?.length ?? 0, 0, "the standalone Workbench requires no project");
  const panels = sinon.spy(window, "createWebviewPanel");
  const browser = await connectEditorBrowser();
  try {
    await commands.executeCommand(COMMAND);
    const frame = await editorWebviewFrame(browser, ".osp-frame-workbench");
    await viewport(frame);
    await waitForIdleWorkbench(frame);
    try {
      await run({ frame, runtime, opened: () => panels.callCount });
    } catch (error) {
      await evidence(frame, failure);
      throw error;
    }
  } finally {
    for (const call of panels.getCalls()) (call.returnValue as WebviewPanel).dispose();
    await browser.close();
  }
}

/** A screenshot of the webview under `target/workbench-vsix`, kept when a case fails. */
export async function evidence(frame: Frame, name: string): Promise<void> {
  const file = path.resolve(__dirname, `../../../../../../target/workbench-vsix/${name}.png`);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  await frame.locator("body").screenshot({ path: file, scale: "css" }).catch(() => undefined);
}

export async function assertNoFreehandInk(frame: Frame): Promise<void> {
  assert.strictEqual(await frame.locator("canvas[data-board-slice], .osp-board, [data-lasso-ordinal], [data-stroke-ordinal]").count(), 0,
    "no freehand board canvas, stroke slice or lasso binding may be rendered");
  assert.strictEqual(await frame.getByRole("img", { name: "Drawing board", exact: true }).count(), 0, "the drawing-board role must be absent");
  for (const name of FREEHAND_CONTROLS) {
    assert.strictEqual(await frame.getByRole("button", { name, exact: true }).count(), 0, `the removed freehand control "${name}" must not exist`);
  }
}

/** Types `source` into the argument editor and waits for the kernel's checked mathematics. */
export async function mathematics(frame: Frame, source = ARGUMENT, last = "From hq."): Promise<void> {
  const editor = frame.getByRole("textbox", { name: "Argument & calculations" });
  const bounds = await editor.boundingBox();
  assert.ok(bounds && bounds.height > 250, `the argument editor must fill its authored writing surface: ${JSON.stringify(bounds)}`);
  await editor.fill(source);
  await frame.locator("math[aria-label]").first().waitFor();
  assert.ok((await frame.locator(".osp-frame-proof-rows").textContent())?.includes(last), "the shared proof keeps the entered reasoning");
  assert.ok((await frame.locator("math[aria-label]").first().getAttribute("aria-label"))?.trim());
  assert.strictEqual(await frame.locator("#osprey-diagnostics:not([hidden])").count(), 0);
}

/** Opens the conjunction lesson from Examples and returns to working. */
export async function lesson(frame: Frame): Promise<void> {
  await press(frame, "Examples");
  await press(frame, "Open conjunction example");
  await frame.locator(".osp-frame-example-context").waitFor();
  await press(frame, "Return to working");
  await frame.locator(".osp-frame-example-context").waitFor({ state: "detached" });
}

export { press, waitForIdleWorkbench };

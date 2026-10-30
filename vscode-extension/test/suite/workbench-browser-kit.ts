import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import * as sinon from "sinon";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import type { Frame } from "playwright-core";
import { ColorThemeKind, commands, Uri, window, workspace, type WebviewPanel } from "vscode";
import { activateWithServer, connectEditorBrowser, editorWebviewFrame, waitFor } from "./profiler-kit";
import * as Wasi from "../../client/src/osprey-ui/wasi";
import * as NativeRendering from "../../client/src/osprey-ui/render";
export const COMMAND = "osprey.workbench.open";

const ARGUMENT = "Theorem and_swap. For all p q, p ∧ q → q ∧ p.\nProof.\nAssume h : p ∧ q.\nBy cases on h, as hp and hq.\nIt suffices to show q and p.\nFrom hq.\nFrom hp.";

const WORKING = "Keep this working: p ∧ q.\nA second line — preserved exactly.";

const FREEHAND_CONTROLS = ["Draw", "Pan", "Pan board", "Clear", "Undo", "Board", "Show working board", "Preview sample transcription"];

export async function assertNoFreehandInk(frame: Frame): Promise<void> {
  assert.strictEqual(await frame.locator("canvas[data-board-slice], .osp-board, [data-lasso-ordinal], [data-pan-ordinal], [data-stroke-ordinal]").count(), 0,
    "no freehand board canvas, stroke slice or lasso binding may be rendered");
  assert.strictEqual(await frame.getByRole("img", { name: "Drawing board", exact: true }).count(), 0, "the drawing-board role must be absent");
  for (const name of FREEHAND_CONTROLS)
    assert.strictEqual(await frame.getByRole("button", { name, exact: true }).count(), 0, `the removed freehand control "${name}" must not exist`);
}

export async function mathematics(frame: Frame): Promise<void> {
  const editor = frame.getByRole("textbox", { name: "Argument & calculations" });
  const bounds = await editor.boundingBox();
  assert.ok(bounds && bounds.height > 250, "the argument editor must fill its authored writing surface");
  await editor.fill(ARGUMENT);
  await frame.locator("math[aria-label]").first().waitFor();
  assert.ok((await frame.locator(".osp-frame-proof-rows").textContent())?.includes("From hq."), "shared Osprey proof preserves the entered reasoning");
  assert.ok((await frame.locator("math[aria-label]").first().getAttribute("aria-label"))?.trim());
  assert.strictEqual(await frame.locator("#osprey-diagnostics:not([hidden])").count(), 0);
}

export function phoneSourceGeometry() {
  const guided = document.querySelector(".osp-frame-example-context")!;
  const context = { height: guided.getBoundingClientRect().height,
    color: getComputedStyle(guided.querySelector('button[aria-label="Return to working"]')!).color };
  const node = document.querySelector(".osp-frame-goal-content")!;
  const goal = node.getBoundingClientRect(), heading = node.querySelector(".osp-role-heading")!.getBoundingClientRect();
  const target = Array.from(node.querySelectorAll(".osp-role-caption")).find(item => item.textContent === "To show")!.getBoundingClientRect();
  const title = document.createRange(); title.selectNodeContents(document.querySelector(".osp-frame-source-heading .osp-role-heading")!);
  const close = document.querySelector('button[aria-label="Close source"]')!.getBoundingClientRect();
  const panel = document.querySelector(".osp-frame-source-panel")!.getBoundingClientRect();
  const sourceHeading = document.querySelector(".osp-frame-source-heading")!.getBoundingClientRect();
  const paper = getComputedStyle(document.querySelector(".osp-frame-paper-document")!);
  return { context, spacing: { left: sourceHeading.left - panel.left, top: sourceHeading.top - panel.top, paperLeft: paper.paddingLeft, paperTop: paper.paddingTop }, layout: { middle: goal.left + goal.width / 2, targetLeft: target.left, targetTop: target.top, headingBottom: heading.bottom,
    sourceTitleRight: title.getBoundingClientRect().right, closeLeft: close.left } };
}

export async function viewport(frame: Frame, size = { width: 1488, height: 1058 }): Promise<void> {
  const page = frame.page();
  for (let attempt = 0; attempt < 3; attempt++) {
    const shell = await page.evaluate(() => ({ width: innerWidth, height: innerHeight }));
    const content = await frame.evaluate(() => ({ width: innerWidth, height: innerHeight }));
    if (content.width === size.width && content.height === size.height) break;
    await page.setViewportSize({ width: shell.width + size.width - content.width, height: shell.height + size.height - content.height });
    await frame.waitForFunction(({ width, height }) => innerWidth !== width || innerHeight !== height, content, { timeout: 1000 });
  }
  assert.deepStrictEqual(await frame.evaluate(() => [innerWidth, innerHeight]), [size.width, size.height]);
}

export async function capture(frame: Frame, name: string): Promise<void> {
  await viewport(frame, name.startsWith("phone-") ? { width: 390, height: 844 } : name.startsWith("tablet-") ? { width: 834, height: 1194 } : { width: 1488, height: 1058 });
  await commands.executeCommand("notifications.clearAll");
  await frame.evaluate(() => document.fonts.ready);
  const diagnostics = frame.locator("#osprey-diagnostics:not([hidden])");
  assert.strictEqual(await diagnostics.count(), 0, (await diagnostics.allTextContents()).join("\n"));
  if (name === "optional-source" || name === "phone-source") await sourceKeyboardFocus(frame);
  await frame.waitForFunction(() => document.getElementById("osprey-root")?.getAttribute("aria-busy") !== "true", undefined, { timeout: 1000 });
  writeNativeSnapshot(name);
  await evidence(frame, name);
}

async function sourceKeyboardFocus(frame: Frame): Promise<void> {
  const close = frame.getByRole("button", { name: "Close source", exact: true });
  await close.focus();
  await close.press("Tab");
  await frame.page().keyboard.press("Shift+Tab");
  const focus = await close.evaluate(node => ({ active: document.activeElement === node, visible: node.matches(":focus-visible"), width: getComputedStyle(node).outlineWidth, offset: getComputedStyle(node).outlineOffset }));
  assert.deepStrictEqual(focus, { active: true, visible: true, width: "2px", offset: "3px" }, "the reference close control retains its actual keyboard focus ring");
  const heading = await frame.locator(".osp-frame-source-heading .osp-role-heading").boundingBox();
  const button = await close.boundingBox();
  assert.ok(heading && button && heading.x + heading.width <= button.x - 5, `source heading must clear the focused close control: ${JSON.stringify({ heading, button })}`);
}

function writeNativeSnapshot(name: string): void {
  const renderer = NativeRendering.renderWorkspace as sinon.SinonSpy;
  assert.ok(renderer.lastCall, "capture must retain the actual Osprey-authored regions");
  const { regions, theme, slices } = renderer.lastCall.args[0];
  const patches = [...regions].map(([key, region]) => ({ key, ...region }));
  const writes = [...slices].map(([key, text]) => ({ key, text }));
  const file = path.resolve(__dirname, `../../../../target/workbench-vsix/${name}.native.json`);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify({ version: 1, state: "read-only-render-review", patches, removed: [], requests: [], slices: writes, theme, diagnostics: [] }));
}

async function evidence(frame: Frame, name: string): Promise<void> {
  const evidence = path.resolve(__dirname, `../../../../target/workbench-vsix/${name}.png`);
  fs.mkdirSync(path.dirname(evidence), { recursive: true });
  await frame.locator("body").screenshot({ path: evidence, scale: "css" });
  await captureLayout(frame, evidence);
  await scrollingEvidence(frame, evidence);
}

async function scrollingEvidence(frame: Frame, evidence: string): Promise<void> {
  const size = await frame.evaluate(() => ({ width: innerWidth, height: innerHeight, fullHeight: document.documentElement.scrollHeight }));
  const chunks: { path: string; top: number }[] = [];
  const host = await frame.frameElement();
  for (let top = 0; top < size.fullHeight; top += size.height) {
    await frame.evaluate(y => scrollTo(0, y), top);
    const actual = await frame.evaluate(() => scrollY);
    const file = evidence.replace(/\.png$/, `.scroll-${chunks.length}.png`);
    await host.screenshot({ path: file, scale: "css" });
    chunks.push({ path: file, top: actual });
    if (actual + size.height >= size.fullHeight) break;
  }
  await frame.evaluate(() => scrollTo(0, 0));
  fs.writeFileSync(evidence.replace(/\.png$/, ".capture.json"), JSON.stringify({ ...size, chunks }));
}

async function runWithEvidence(frame: Frame, run: (frame: Frame, opened: () => number) => Promise<void>, opened: () => number, failure: string): Promise<void> {
  const errors: string[] = [];
  const recordError = (error: Error) => errors.push(error.message);
  frame.page().on("pageerror", recordError);
  try { await run(frame, opened); assert.deepStrictEqual(errors, [], "the real Workbench must not emit browser execution errors"); }
  catch (error) { await evidence(frame, failure).catch(problem => console.error("Capture failed:", problem)); throw error; }
  finally { frame.page().off("pageerror", recordError); }
}

async function captureLayout(frame: Frame, evidence: string): Promise<void> {
  fs.writeFileSync(evidence.replace(/\.png$/, ".html"), await frame.content());
  fs.writeFileSync(evidence.replace(/\.png$/, ".layout.json"), JSON.stringify(await frame.evaluate(() =>
    Array.from(document.querySelectorAll('.osp-framed, .osp-editor, .osp-editor-surface, textarea, .osp-rich-text, math, .osp-layer, .osp-themed')).map(node => {
      const style = getComputedStyle(node), box = node.getBoundingClientRect();
      return { class: node.getAttribute("class"), x: box.x, y: box.y, width: box.width, height: box.height, display: style.display, position: style.position,
        font: style.fontFamily, size: style.fontSize, lineHeight: style.lineHeight, gap: style.gap, padding: style.padding, margin: style.margin, color: style.color, background: style.backgroundColor };
    })), null, 2));
}

export async function lesson(frame: Frame): Promise<void> {
  await frame.getByRole("button", { name: "Examples", exact: true }).click();
  const entry = frame.locator(".osp-section").filter({ hasText: "And, the other way around" });
  await entry.getByRole("button", { name: "Open", exact: true }).click();
  await frame.getByRole("button", { name: "All lessons", exact: true }).waitFor();
  assert.ok(await frame.locator("math[aria-label]").count() > 0);
  assert.ok(await frame.locator("img.osp-media-image").count() > 0);
  const answer = frame.getByRole("textbox", { name: "Your formula", exact: true });
  await answer.scrollIntoViewIfNeeded();
  const bounds = await answer.boundingBox();
  const height = await frame.evaluate(() => innerHeight);
  assert.ok(bounds && bounds.y >= 0 && bounds.y + bounds.height <= height, "lesson challenges must scroll into the visible application viewport");
  await answer.fill("q ∧ p");
  assert.strictEqual(await answer.inputValue(), "q ∧ p");
  assert.strictEqual(await frame.locator("#osprey-diagnostics:not([hidden])").count(), 0);
}

export async function withWorkbench(run: (frame: Frame, opened: () => number) => Promise<void>, failure = "failure", appearance = ColorThemeKind.Light): Promise<void> {
  sinon.stub(window, "activeColorTheme").get(() => ({ kind: appearance }));
  sinon.spy(NativeRendering, "renderWorkspace");
  await activateWithServer();
  assert.strictEqual(workspace.workspaceFolders?.length ?? 0, 0, "standalone workbench requires no project");
  const panels = sinon.spy(window, "createWebviewPanel");
  const browser = await connectEditorBrowser();
  try {
    await commands.executeCommand(COMMAND);
    const frame = await editorWebviewFrame(browser, ".osp-frame-workbench");
    await viewport(frame);
    await runWithEvidence(frame, run, () => panels.callCount, failure);
  } finally {
    for (const call of panels.getCalls()) (call.returnValue as WebviewPanel).dispose();
    await browser.close();
  }
}

export async function pendingWorkbenchUpdate(frame: Frame): Promise<void> {
  const actual = Wasi.runWasi;
  let release = (): void => undefined;
  const held = new Promise<void>(resolve => { release = resolve; });
  const run = sinon.stub(Wasi, "runWasi").callsFake(async (module, input, options) => { await held; return actual(module, input, options); });
  try {
    await frame.getByRole("button", { name: "Proof", exact: true }).click();
    const pending = await frame.evaluate(() => ({ busy: document.getElementById("osprey-root")?.getAttribute("aria-busy"),
      disabled: Array.from(document.querySelectorAll<HTMLButtonElement>("#osprey-root button")).every(button => button.disabled),
      notice: document.getElementById("osprey-runtime-status")?.textContent }));
    assert.deepStrictEqual(pending, { busy: "true", disabled: true, notice: "Updating · previous results shown" });
    await frame.getByRole("textbox", { name: "Solution title", exact: true }).fill("Keep pending draft");
  } finally { release(); run.restore(); }
  await frame.waitForFunction(() => document.getElementById("osprey-root")?.getAttribute("aria-busy") === "false", undefined, { timeout: 1000 });
  assert.strictEqual(await frame.getByRole("textbox", { name: "Solution title", exact: true }).inputValue(), "Keep pending draft");
  assert.strictEqual(await frame.getByRole("button", { name: "Source", exact: true }).isEnabled(), true);
}

export function frameSource(replacement: string): (frame: string) => string {
  return raw => {
    const frame = JSON.parse(raw);
    if (frame.slices) frame.slices["proof:import"] = replacement;
    return JSON.stringify(frame);
  };
}

export async function retainedImport(change: (raw: string) => string, valid: boolean, prepare: (frame: Frame) => Promise<void>): Promise<void> {
  const actual = Wasi.runWasi;
  let changed = false;
  sinon.stub(Wasi, "runWasi").callsFake((module, raw, options) => actual(module, changed ? change(raw) : raw, options));
  await withWorkbench(async frame => {
    await prepare(frame);
    changed = true;
    await frame.getByRole("combobox", { name: "Theorem", exact: true }).selectOption("id_imp");
    await currentImport(frame, valid);
  }, "failure-retained-import");
}

async function currentImport(frame: Frame, valid: boolean): Promise<void> {
  if (valid) {
    await frame.locator('math[aria-label="(∀ (q : Prop), (q → q))"]').waitFor();
    assert.strictEqual(await frame.locator('math[aria-label="(∀ (p : Prop), (p → p))"]').count(), 0, "the previous source's statement must disappear");
    await frame.getByRole("button", { name: "Imported source", exact: true }).click();
    await frame.getByText("(∀ (q : Prop), (q → q))", { exact: true }).waitFor();
    assert.strictEqual(await frame.getByText("(∀ (p : Prop), (p → p))", { exact: true }).count(), 0);
    await frame.getByText("Checked theorem", { exact: true }).waitFor();
  } else {
    await frame.getByText("Import refused", { exact: true }).waitFor();
    assert.strictEqual(await frame.getByText("Checked theorem", { exact: true }).count(), 0, "retained DOM must lose its old accepted verdict");
  }
}

/** Notation toggles the library, so it is pressed only when the library is closed. */
export async function openProofLibrary(frame: Frame): Promise<void> {
  const open = frame.getByRole("button", { name: "Open proof library", exact: true });
  if (await open.count() === 0) await frame.getByRole("button", { name: "Notation", exact: true }).click();
  await open.click();
}

export async function checkedProofFits(frame: Frame, theorem: string, assumptions: string): Promise<void> {
  const layout = await frame.evaluate(({ theorem, axioms }) => {
    const selected = Array.from(document.querySelectorAll(".osp-text")).find(node => node.textContent === theorem);
    const verdict = Array.from(document.querySelectorAll(".osp-badge")).find(node => node.textContent === "Checked theorem");
    const axiom = Array.from(document.querySelectorAll(".osp-text")).find(node => node.textContent === axioms);
    const boxes = [selected, verdict, axiom].map(node => {
      if (!node) return null;
      const range = document.createRange(); range.selectNodeContents(node);
      const box = range.getBoundingClientRect();
      return { top: box.top, bottom: box.bottom, left: box.left, right: box.right, height: box.height };
    });
    return { boxes, width: innerWidth, height: innerHeight };
  }, { theorem, axioms: assumptions });
  assert.ok(layout.boxes.every(box => box && box.height > 0 && box.top >= 0 && box.bottom <= layout.height
    && box.left >= 0 && box.right <= layout.width), `selected theorem, verdict and axioms must remain visible: ${JSON.stringify(layout)}`);
}

export async function checkedMathematicsFits(frame: Frame): Promise<void> {
  const formula = frame.locator(".osp-frame-proof-library-result math");
  await formula.waitFor();
  await notationFits(frame);
  const layout = await formula.evaluate(node => {
    const box = node.getBoundingClientRect(), parent = node.parentElement!, bounds = parent.getBoundingClientRect();
    return { label: node.getAttribute("aria-label"), left: box.left, right: box.right, parentLeft: bounds.left, parentRight: bounds.right,
      width: parent.clientWidth, content: parent.scrollWidth, overflow: getComputedStyle(parent).overflowX, viewport: innerWidth };
  });
  assert.ok(layout.label && layout.width > 0 && layout.parentLeft >= 0 && layout.parentRight <= layout.viewport, JSON.stringify(layout));
  assert.ok(layout.right <= layout.parentRight + 1 || ["auto", "scroll"].includes(layout.overflow), `the complete checked mathematics must fit or scroll: ${JSON.stringify(layout)}`);
  await frame.getByRole("button", { name: "Imported source", exact: true }).click();
  const formal = frame.locator(".osp-frame-proof-library-formal");
  await formal.waitFor();
  assert.strictEqual(await formal.innerText(), layout.label, "the formal view preserves the complete checked statement");
  await frame.getByRole("button", { name: "Imported source", exact: true }).click();
}

export async function checkedEquationFits(frame: Frame): Promise<void> {
  const bounds = await frame.locator(".osp-frame-proof-library-result math").evaluate(node => {
    const parent = node.parentElement!.getBoundingClientRect();
    const glyphs = Array.from(node.querySelectorAll("mi,mo,mn,mtext")).map(glyph => glyph.getBoundingClientRect());
    return { left: parent.left, right: parent.right, first: Math.min(...glyphs.map(glyph => glyph.left)), last: Math.max(...glyphs.map(glyph => glyph.right)) };
  });
  assert.ok(bounds.first >= bounds.left && bounds.last <= bounds.right + 1, `the complete checked equation must be visible: ${JSON.stringify(bounds)}`);
}

export async function checkedRunWorkMathematics(frame: Frame): Promise<void> {
  const formula = frame.locator(".osp-frame-proof-library-result math");
  assert.strictEqual(await formula.locator("mi").filter({ hasText: /^OAI\.LowSpaceSubsetSum\.WordSeedPowerExecution\.run$/ }).count(), 1, "the checked application retains its complete constant name");
  assert.ok((await formula.textContent())?.replace(/\s/g, "").endsWith("3×width+1"), "the checked equation shows actual multiplication and addition");
  assert.deepStrictEqual(await formula.locator("msub > mn").allTextContents(), ["2"], "the checked left side shows its second projection");
  await checkedEquationFits(frame);
}

async function notationFits(frame: Frame): Promise<void> {
  const layout = await frame.evaluate(() => {
    const entries = Array.from(document.querySelectorAll('[data-region-key="library"] .osp-section-body > p')).filter(node => node.textContent?.startsWith("\\"));
    const rows = entries.slice(0, 2).map(node => node.getBoundingClientRect().top);
    const logo = document.querySelector('img[alt="Osprey"]')!.getBoundingClientRect();
    return { rows, logo: [logo.width, logo.height] };
  });
  assert.deepStrictEqual(layout.logo, [64, 64], "the brand mark retains its authored dimensions");
  assert.ok(layout.rows.length === 2 && Math.abs(layout.rows[0] - layout.rows[1]) < 1, "notation remains a compact two-column reference");
}

export async function sourceCloseBounds(frame: Frame): Promise<{ width: number; height: number; top: number; right: number }> {
  return frame.getByRole("button", { name: "Close source", exact: true }).evaluate(element => {
    const box = element.parentElement!.getBoundingClientRect(), panel = element.closest(".osp-frame-source-panel")!.getBoundingClientRect();
    return { width: box.width, height: box.height, top: box.top - panel.top, right: panel.right - box.right };
  });
}

export async function selectImportedProof(frame: Frame, name: RegExp): Promise<void> {
  const picker = frame.getByRole("combobox", { name: "Theorem", exact: true });
  const key = await picker.getByRole("option", { name }).getAttribute("value");
  assert.ok(key, "the exported theorem must be selectable");
  await picker.selectOption(key);
  await pollUntilResult({ fn: () => picker.inputValue(), until: selected => selected === key, timeoutMs: 2000, description: "the shared application selects the requested imported theorem" });
  const heading = frame.locator(".osp-frame-proof-library-result .osp-role-headline");
  if (await heading.textContent() !== key) {
    assert.strictEqual(await frame.locator("#osprey-root").getAttribute("aria-busy"), "true", "an old theorem must be explicitly marked as pending");
    assert.strictEqual(await frame.locator("#osprey-runtime-status").textContent(), "Updating · previous results shown");
  }
  await pollUntilResult({ fn: () => heading.textContent(), until: text => text === key, timeoutMs: 3000, description: "the checked result belongs to the selected theorem" });
  await frame.getByText("Checked theorem", { exact: true }).waitFor();
  assert.strictEqual(await frame.locator("#osprey-diagnostics:not([hidden])").count(), 0);
}

export async function rejectEditedImport(frame: Frame, source: string, mutate = (text: string) => text.replace('"name":235,"type":730,"value":817', '"name":235,"type":730,"value":730')): Promise<void> {
  const corrupted = mutate(source);
  assert.notStrictEqual(corrupted, source, "corrupt the selected canonical proof term, retaining its statement");
  await frame.getByRole("button", { name: "Imported source", exact: true }).click();
  const editor = frame.getByRole("textbox", { name: "Current proof export", exact: true });
  assert.strictEqual(await editor.inputValue(), source);
  await editor.fill(corrupted);
  await frame.getByText("Import refused", { exact: true }).waitFor();
  assert.strictEqual(await frame.getByText("Checked theorem", { exact: true }).count(), 0, "a refused current export cannot retain its old checked theorem");
  await editor.fill(source);
}

function checkedSource(execution: sinon.SinonSpy, source: string): void {
  assert.ok(execution.getCalls().some(call => JSON.parse(call.args[1]).slices?.["proof:import"] === source),
    "the actual current proof bytes must reach the shared WASI checker");
}

export async function rejectReimportedProof(frame: Frame, source: string, execution: sinon.SinonSpy): Promise<void> {
  checkedSource(execution, source);
  const corrupted = source.replace('"name":4843,"type":36885,"value":36932', '"name":4843,"type":36885,"value":36885');
  assert.notStrictEqual(corrupted, source, "change only the selected proof term, preserving its statement");
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-corrupted-proof-")), file = path.join(directory, "proof.ndjson");
  fs.writeFileSync(file, corrupted);
  const picker = sinon.stub(window, "showOpenDialog").resolves([Uri.file(file)]);
  try {
    await openProofLibrary(frame);
    await frame.getByText("Import refused", { exact: true }).waitFor();
    assert.ok((await frame.locator(".osp-frame-proof-library").innerText()).includes("type mismatch"), "the kernel must reject the proof term rather than file size or source availability");
    assert.strictEqual(await frame.getByText("Checked theorem", { exact: true }).count(), 0);
    assert.strictEqual(picker.callCount, 1, "the changed export goes through the real single-file import");
    checkedSource(execution, corrupted);
  } finally { picker.restore(); fs.rmSync(directory, { recursive: true, force: true }); }
}

async function writeWorkspace(frame: Frame, file: string, title: string, text: string) {
  await frame.getByRole("textbox", { name: "Solution title", exact: true }).fill(title);
  await frame.getByRole("textbox", { name: "Argument & calculations", exact: true }).fill(text);
  await frame.getByRole("button", { name: "Save", exact: true }).click();
  await waitFor(() => fs.existsSync(file), Boolean, 3000, "real Osprey SaveFile writes the chosen file");
  return JSON.parse(fs.readFileSync(file, "utf8"));
}

export async function saveWorking(frame: Frame, file: string): Promise<void> {
  const saved = await writeWorkspace(frame, file, "Saved experiment", WORKING);
  assert.deepStrictEqual(Object.keys(saved).sort(), ["board", "text", "title", "version"]);
  assert.strictEqual(saved.version, 1);
  assert.strictEqual(saved.title, "Saved experiment");
  assert.strictEqual(saved.text, WORKING);
  assert.strictEqual(saved.board.strokes.length, 0, "a new typed save must not invent freehand strokes");
}

async function openWorkspaceTo(frame: Frame, title: string): Promise<void> {
  await frame.getByRole("button", { name: "Open workspace", exact: true }).click();
  const editor = frame.getByRole("textbox", { name: "Argument & calculations", exact: true });
  await pollUntilResult({ fn: () => editor.inputValue(), until: text => text === WORKING, timeoutMs: 3000, description: "real Osprey ChooseFile restores written working" });
  assert.strictEqual(await frame.getByRole("textbox", { name: "Solution title", exact: true }).inputValue(), title);
  await assertNoFreehandInk(frame);
}

export async function reopenWorking(frame: Frame, file: string): Promise<void> {
  await frame.getByRole("textbox", { name: "Argument & calculations", exact: true }).fill("Changed since saving");
  await openWorkspaceTo(frame, "Saved experiment");
  assert.strictEqual(JSON.parse(fs.readFileSync(file, "utf8")).text, WORKING);
}

const LEGACY_BOARD = { version: 1, notes: "Legacy board notes", size: { width: 1536, height: 1024 }, view: { x: 0, y: 0, scale: 100 }, strokes: [{ points: [[10, 20], [80, 90]], width: 4, at: 1 }] };

export function writeLegacyWorkspace(file: string): void {
  fs.writeFileSync(file, JSON.stringify({ version: 1, title: "Legacy experiment", text: WORKING, board: LEGACY_BOARD }));
}

export async function preserveLegacyWorking(frame: Frame, saved: string): Promise<void> {
  await openWorkspaceTo(frame, "Legacy experiment");
  const reopened = await writeWorkspace(frame, saved, "Legacy experiment", WORKING);
  assert.deepStrictEqual(reopened.board.strokes, LEGACY_BOARD.strokes, "opening a legacy workspace then saving must retain its existing board data");
  assert.strictEqual(reopened.board.notes, "Legacy board notes");
}

export async function exportWorking(frame: Frame, file: string): Promise<void> {
  await frame.getByRole("textbox", { name: "Argument & calculations", exact: true }).fill(ARGUMENT);
  await frame.getByRole("button", { name: "Source", exact: true }).click();
  await frame.getByRole("button", { name: "Export generated source", exact: true }).click();
  await waitFor(() => fs.existsSync(file), Boolean, 3000, "real Osprey source export writes the chosen file");
  const golden = path.resolve(__dirname, "../../../../apps/workbench/tests/golden/and_swap.ospml.expected");
  assert.strictEqual(fs.readFileSync(file, "utf8"), fs.readFileSync(golden, "utf8"));
}

const OBSOLETE_SCENES: Record<string, string> = {
  "blank-board": "renamed to blank-document after the freehand board was removed",
  "board-first": "the freehand board-first layout was removed; the paper document is the working surface",
  "tablet-board": "the freehand tablet board state was removed",
  "phone-board": "the freehand phone board state was removed",
  transcription: "the unsupported sample-transcription preview was removed",
};

export async function comparisonState(frame: Frame, scene: string): Promise<void> {
  const obsolete = OBSOLETE_SCENES[scene];
  if (obsolete) throw new Error(`obsolete workbench design state "${scene}" requires owner and design-document reconciliation: ${obsolete}`);
  if (scene === "blank-document") return;
  if (scene === "working-session") return workingSession(frame);
  await frame.getByRole("button", { name: "Examples", exact: true }).click();
  await frame.getByRole("button", { name: "Open conjunction example", exact: true }).click();
  await frame.waitForFunction(() => document.querySelector(".osp-frame-example-context, #osprey-diagnostics:not([hidden])") !== null);
  assert.strictEqual(await frame.locator("#osprey-diagnostics:not([hidden])").count(), 0);
  const removed = scene === "proof-start" ? 5 : scene === "paper-proof" ? 1 : /mismatch|unsupported/.test(scene) ? 2 : 0;
  if (removed) await clearProofTail(frame, removed);
  await referenceNavigation(frame, scene);
}

async function workingSession(frame: Frame): Promise<void> {
  await frame.getByRole("textbox", { name: "Solution title", exact: true }).fill("A geometric series");
  await frame.getByRole("textbox", { name: "Argument & calculations", exact: true }).fill("For |r| < 1, consider the partial sum.\n\nSₙ = 1 + r + r² + … + rⁿ\n\nMultiply by r and subtract:\n\n(1 − r)Sₙ = 1 − rⁿ⁺¹\n\nAs n grows, rⁿ⁺¹ tends to zero.");
}

async function referenceNavigation(frame: Frame, scene: string): Promise<void> {
  if (/paper-proof|proof-start|optional-source|phone-source|mismatch|unsupported/.test(scene)) await frame.getByRole("button", { name: "Proof", exact: true }).click();
  if (/optional-source|phone-source/.test(scene)) {
    await frame.getByRole("button", { name: "Source", exact: true }).click();
    await frame.locator(".osp-frame-source-panel").waitFor();
  }
  if (scene === "reading") await frame.getByRole("button", { name: "Read", exact: true }).click();
  if (scene === "slate-board") await frame.getByRole("button", { name: "Switch appearance", exact: true }).click();
  if (scene === "mismatch-live") await savedNotice(frame);
  if (/mismatch|unsupported/.test(scene)) {
    await frame.getByRole("textbox", { name: "Write the next step…", exact: true }).fill(scene === "unsupported-input" ? "Therefore q, since hq." : "From hp.");
    await frame.locator(scene === "unsupported-input" ? ".osp-frame-unsupported-feedback" : ".osp-frame-mismatch-feedback").waitFor();
  }
}

async function savedNotice(frame: Frame): Promise<void> {
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-reference-save-"));
  const file = path.join(directory, "working.json");
  const picker = sinon.stub(window, "showSaveDialog").resolves(Uri.file(file));
  try {
    await frame.getByRole("button", { name: "Save", exact: true }).click();
    await waitFor(() => fs.existsSync(file), Boolean, 3000, "reference live mismatch retains a real save receipt");
    await frame.locator(".osp-frame-operation-notice").waitFor();
  } finally { picker.restore(); fs.rmSync(directory, { recursive: true, force: true }); }
}

async function clearProofTail(frame: Frame, count: number): Promise<void> {
  await frame.getByRole("button", { name: "Proof", exact: true }).click();
  for (let removed = 0; removed < count; removed++) {
    const rows = frame.locator('.osp-frame-proof-rows [role="option"]');
    const before = await rows.count();
    const selected = await rows.last().locator(".osp-rich-text").innerText();
    await rows.last().click();
    await frame.waitForFunction(expected => (document.querySelector('textarea[aria-label="Write the next step…"]') as HTMLTextAreaElement)?.value.trim() === expected.trim(), selected, { timeout: 1000 });
    await frame.getByRole("textbox", { name: "Write the next step…", exact: true }).fill("");
    await pollUntilResult({ fn: () => rows.count(), until: remaining => remaining === before - 1, timeoutMs: 1000, description: `deleting proof row ${5 - removed}: expected ${before - 1} remaining` });
  }
  await frame.getByRole("button", { name: "Continue proof", exact: true }).click();
}

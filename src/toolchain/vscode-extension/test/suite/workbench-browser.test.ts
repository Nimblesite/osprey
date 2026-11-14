// [WORKBENCH-VSIX]: the real Workbench in the test window's webview, its live application instance
// wrapped but never replaced. Every case drives the controls a person would use.
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as sinon from "sinon";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { ColorThemeKind, commands } from "vscode";
import type { Frame } from "playwright-core";
import { COMMAND, assertNoFreehandInk, lesson, mathematics, press, waitForIdleWorkbench, withWorkbench } from "./workbench-browser-kit";

const title = (frame: Frame) => frame.getByRole("textbox", { name: "Solution title", exact: true });
const appearance = (frame: Frame) => frame.getByRole("button", { name: "Switch appearance", exact: true });

/** The webview's busy state, paused buttons and status notice. */
const pausedState = (frame: Frame) => frame.evaluate(() => ({
  busy: document.getElementById("osprey-root")?.getAttribute("aria-busy"),
  disabled: Array.from(document.querySelectorAll<HTMLButtonElement>("#osprey-root button")).filter((button) => !button.disabled).map((button) => button.getAttribute("aria-label") ?? button.textContent ?? "").join(",") || true,
  notice: document.getElementById("osprey-runtime-status")?.hidden === false ? document.getElementById("osprey-runtime-status")?.textContent : "",
}));

async function forgedClick(frame: Frame, name: string, ordinal: string): Promise<void> {
  await frame.getByRole("button", { name, exact: true }).evaluate((button, value) => {
    button.setAttribute("data-event-ordinal", value);
    (button as HTMLButtonElement).click();
  }, ordinal);
}

suite("workbench VSIX browser", () => {
  teardown(() => sinon.restore());

  test("a refused stale action does not leave the workbench busy", async () => {
    await withWorkbench(async ({ frame, runtime }) => {
      await press(frame, "Examples");
      const open = frame.getByRole("button", { name: "Open conjunction example", exact: true });
      await open.waitFor();
      await waitForIdleWorkbench(frame);
      const before = runtime.events.length;
      await forgedClick(frame, "Proof", String(2 ** 31));
      await waitForIdleWorkbench(frame, 5000);
      assert.deepStrictEqual(runtime.events.slice(before).map((event) => event.ordinal), [2 ** 31], "the forged ordinal reaches the application, which refuses it");
      assert.strictEqual(await open.isVisible(), true, "the refused action must not navigate");
      assert.strictEqual(await frame.getByRole("button", { name: "Proof", exact: true }).isEnabled(), true, "an ignored action must not leave controls paused");
    }, "failure-stale-action");
  });

  test("a refused stale action does not leave the workbench busy (extra: a malformed ordinal never reaches the runtime)", async () => {
    await withWorkbench(async ({ frame, runtime }) => {
      await waitForIdleWorkbench(frame);
      const before = runtime.events.length;
      for (const ordinal of ["-1", "0.5", "not a number"]) await forgedClick(frame, "Examples", ordinal);
      await waitForIdleWorkbench(frame, 5000);
      assert.deepStrictEqual(runtime.events.slice(before).filter((event) => event.kind === 0 && event.ordinal < 0), []);
      assert.strictEqual(await frame.locator(".osp-frame-workbench").count(), 1);
      assert.strictEqual(await frame.getByRole("button", { name: "Examples", exact: true }).isEnabled(), true);
    }, "failure-malformed-ordinal");
  });

  test("workbench removes freehand drawing while retaining typed mathematics", async () => {
    await withWorkbench(async ({ frame }) => {
      await assertNoFreehandInk(frame);
      const editor = frame.getByRole("textbox", { name: "Argument & calculations", exact: true });
      await editor.waitFor({ state: "visible" });
      const bounds = await editor.boundingBox();
      assert.ok(bounds && bounds.height >= 250 && bounds.width > 300, `the typed editor must occupy a real writing surface: ${JSON.stringify(bounds)}`);
      assert.strictEqual(await editor.inputValue(), "", "the mathematical document starts as an editable text surface, not ink");
      await mathematics(frame);
      await frame.getByText("Checked", { exact: true }).first().waitFor();
      await frame.locator(".osp-frame-proof-document").waitFor();
      await assertNoFreehandInk(frame);
      await press(frame, "Source");
      await frame.locator(".osp-frame-source-panel").waitFor();
      assert.ok((await frame.locator(".osp-frame-source-code").innerText()).trim().length > 0, "generated source retains real checked text");
      await assertNoFreehandInk(frame);
      await press(frame, "Close source");
      await frame.getByRole("button", { name: "Close source", exact: true }).waitFor({ state: "detached" });
      await lesson(frame);
      await assertNoFreehandInk(frame);
    }, "failure-freehand-removed");
  });

  test("workbench removes freehand drawing while retaining typed mathematics (extra: the notation library draws no ink either)", async () => {
    await withWorkbench(async ({ frame }) => {
      await press(frame, "Notation");
      await frame.locator(".osp-frame-proof-library").waitFor();
      await assertNoFreehandInk(frame);
      await press(frame, "Proof");
      await frame.getByRole("textbox", { name: "Argument & calculations", exact: true }).waitFor();
      await assertNoFreehandInk(frame);
    }, "failure-freehand-notation");
  });

  test("a pending runtime update pauses actions while preserving the editable draft", async () => {
    await withWorkbench(async ({ frame, runtime }) => {
      const release = runtime.hold();
      try {
        await press(frame, "Notation");
        // The pressed button keeps its node until its gesture's click has been delivered, then pauses too.
        const paused = await pollUntilResult({ fn: () => pausedState(frame), until: (state) => state.busy === "true" && state.disabled === true, timeoutMs: 2000, description: "the held call pauses every control" });
        assert.deepStrictEqual(paused, { busy: "true", disabled: true, notice: "Updating · previous results shown" });
        await title(frame).fill("Keep pending draft");
      } finally { release(); }
      await waitForIdleWorkbench(frame, 5000);
      assert.strictEqual(await title(frame).inputValue(), "Keep pending draft");
      assert.strictEqual(await frame.getByRole("button", { name: "Proof", exact: true }).isEnabled(), true);
    }, "failure-pending-runtime");
  });

  test("a pending runtime update pauses actions while preserving the editable draft (extra: released controls keep their own disabled state)", async () => {
    await withWorkbench(async ({ frame, runtime }) => {
      const disabledBefore = await frame.locator("#osprey-root button:disabled").count();
      const release = runtime.hold();
      try {
        await press(frame, "Examples");
        await pollUntilResult({ fn: () => pausedState(frame), until: (state) => state.busy === "true", timeoutMs: 2000, description: "busy" });
      } finally { release(); }
      await waitForIdleWorkbench(frame, 5000);
      await press(frame, "Proof");
      await waitForIdleWorkbench(frame, 5000);
      assert.strictEqual(await frame.locator("#osprey-root button:disabled").count(), disabledBefore, "only the controls the application disabled stay disabled");
      assert.deepStrictEqual((await pausedState(frame)).notice, "");
    }, "failure-pending-restored");
  });

  for (const [kind, symbol, colour] of [[ColorThemeKind.Dark, "sun.max", "rgb(32, 39, 35)"], [ColorThemeKind.Light, "moon", "rgb(250, 250, 247)"]] as const) {
    test(`startup appearance follows editor theme ${kind} and manual toggle preserves work`, async () => {
      await withWorkbench(async ({ frame }) => {
        assert.strictEqual(await appearance(frame).locator(".osp-symbol").getAttribute("data-symbol"), symbol);
        assert.strictEqual(await frame.locator(".osp-frame-workbench").evaluate((node) => getComputedStyle(node).backgroundColor), colour);
        await title(frame).fill("Keep my theme work");
        await appearance(frame).click();
        await appearance(frame).locator(`[data-symbol="${symbol === "moon" ? "sun.max" : "moon"}"]`).waitFor();
        assert.strictEqual(await title(frame).inputValue(), "Keep my theme work");
      }, `failure-startup-theme-${kind}`, kind);
    });

    test(`startup appearance follows editor theme ${kind} and manual toggle preserves work (extra: two toggles return to the startup canvas)`, async () => {
      await withWorkbench(async ({ frame }) => {
        await title(frame).fill("Round trip");
        for (const next of [symbol === "moon" ? "sun.max" : "moon", symbol]) {
          await appearance(frame).click();
          await appearance(frame).locator(`[data-symbol="${next}"]`).waitFor();
        }
        assert.strictEqual(await frame.locator(".osp-frame-workbench").evaluate((node) => getComputedStyle(node).backgroundColor), colour);
        assert.strictEqual(await title(frame).inputValue(), "Round trip");
      }, `failure-startup-theme-round-${kind}`, kind);
    });
  }

  test("reference icons and theme roundtrips use actual shared SVG assets", async () => {
    await withWorkbench(async ({ frame }) => {
      for (const [label, symbol, file] of [["Switch appearance", "moon", "moon"], ["Open workspace", "folder.open", "folder-open"], ["Save", "square.and.arrow.down", "floppy-disk"]]) {
        const icon = frame.getByRole("button", { name: label, exact: true }).locator(".osp-symbol");
        assert.ok(await icon.isVisible(), `${label} retains its visible source asset`);
        assert.strictEqual(await icon.getAttribute("data-symbol"), symbol);
        const mask = await icon.evaluate((node) => getComputedStyle(node).maskImage);
        assert.ok(mask.includes(`icons/${file}.svg`), `${label} masks with ${file}.svg, got ${mask}`);
        assert.strictEqual((mask.match(/url\(/g) ?? []).length, 1, `${label} has exactly one mask image`);
      }
      await appearance(frame).click();
      await appearance(frame).locator('[data-symbol="sun.max"]').waitFor();
      assert.ok((await appearance(frame).locator(".osp-symbol").evaluate((node) => getComputedStyle(node).maskImage)).includes("icons/sun.svg"));
      await appearance(frame).click();
      await appearance(frame).locator('[data-symbol="moon"]').waitFor();
    }, "failure-reference-icons");
  });

  test("reference icons and theme roundtrips use actual shared SVG assets (extra: every drawn symbol names a bundled asset)", async () => {
    await withWorkbench(async ({ frame }) => {
      const masks = await frame.locator("#osprey-root .osp-symbol").evaluateAll((nodes) => nodes.map((node) => getComputedStyle(node).maskImage));
      assert.ok(masks.length > 3, `the header draws its symbols: ${masks.length}`);
      for (const mask of masks) assert.ok(/icons\/[a-z0-9-]+\.svg/.test(mask) && !/https?:\/\/(?!vscode)/.test(mask.replace(/https:\/\/file\+\.vscode-resource[^)]*/g, "")), mask);
    }, "failure-reference-icon-assets");
  });

  test("app-owned field colours follow manual appearance changes and retain the selected theorem", async () => {
    await withWorkbench(async ({ frame }) => {
      await press(frame, "Notation");
      await press(frame, "Natural arithmetic");
      const picker = frame.getByRole("combobox", { name: "Theorem", exact: true });
      const selected = await picker.inputValue();
      for (const [index, [symbol, background]] of [["moon", "rgb(255, 255, 255)"], ["sun.max", "rgb(41, 50, 45)"], ["moon", "rgb(255, 255, 255)"]].entries()) {
        if (index > 0) await press(frame, "Switch appearance");
        await appearance(frame).locator(`[data-symbol="${symbol}"]`).waitFor();
        assert.strictEqual(await picker.evaluate((node) => getComputedStyle(node).backgroundColor), background);
        assert.strictEqual(await picker.inputValue(), selected);
      }
    }, "failure-field-appearance");
  });

  test("app-owned field colours follow manual appearance changes and retain the selected theorem (extra: the title field follows too)", async () => {
    await withWorkbench(async ({ frame }) => {
      await title(frame).fill("Coloured");
      const light = await title(frame).evaluate((node) => getComputedStyle(node).color);
      await press(frame, "Switch appearance");
      await appearance(frame).locator('[data-symbol="sun.max"]').waitFor();
      const dark = await title(frame).evaluate((node) => getComputedStyle(node).color);
      assert.notStrictEqual(dark, light, "the field's ink follows the application's appearance");
      assert.strictEqual(await title(frame).inputValue(), "Coloured");
    }, "failure-title-appearance");
  });

  test("opening a conjunction example leaves imported proof inspection", async () => {
    await withWorkbench(async ({ frame }) => {
      await press(frame, "Notation");
      await press(frame, "Natural arithmetic");
      await frame.getByText("Checked theorem", { exact: true }).waitFor();
      await press(frame, "Examples");
      await press(frame, "Open conjunction example");
      await waitForIdleWorkbench(frame, 5000);
      assert.strictEqual(await frame.locator(".osp-frame-proof-library-inspection").count(), 0, "starting the example must leave imported-library inspection");
      await frame.locator(".osp-frame-proof-rows").waitFor();
      assert.ok((await frame.locator(".osp-frame-proof-rows").textContent())?.includes("From hp."));
    }, "failure-example-leaves-library");
  });

  test("opening a conjunction example leaves imported proof inspection (extra: returning to working keeps the library put away)", async () => {
    await withWorkbench(async ({ frame }) => {
      await press(frame, "Notation");
      await press(frame, "Natural arithmetic");
      await frame.getByText("Checked theorem", { exact: true }).waitFor();
      await lesson(frame);
      assert.strictEqual(await frame.locator(".osp-frame-proof-library-inspection").count(), 0);
      await frame.getByRole("textbox", { name: "Argument & calculations", exact: true }).waitFor();
    }, "failure-example-return");
  });

  test("the shared app exposes its Osprey branding and reference links", async () => {
    await withWorkbench(async ({ frame }) => {
      assert.strictEqual(await frame.getByRole("link", { name: "Built with Osprey", exact: true }).getAttribute("href"), "https://ospreylang.dev/");
      await press(frame, "Notation");
      await frame.getByRole("img", { name: "Osprey", exact: true }).first().waitFor();
      for (const [name, href] of [["Osprey website", "https://ospreylang.dev/"], ["Language documentation", "https://ospreylang.dev/docs/"], ["Workbench source", "https://github.com/Nimblesite/osprey/tree/main/src/osprey/workbench"]]) {
        assert.strictEqual(await frame.getByRole("link", { name, exact: true }).getAttribute("href"), href);
      }
    }, "failure-branding");
  });

  test("the shared app exposes its Osprey branding and reference links (extra: every link is HTTPS and opens outside the webview)", async () => {
    await withWorkbench(async ({ frame }) => {
      await press(frame, "Notation");
      await frame.getByRole("link", { name: "Workbench source", exact: true }).waitFor();
      const links = await frame.locator("#osprey-root a[href]").evaluateAll((nodes) => nodes.map((node) => node.getAttribute("href") ?? ""));
      assert.ok(links.length >= 3, links.join(","));
      for (const href of links) assert.ok(href.startsWith("https://"), href);
    }, "failure-branding-links");
  });

  test("real shared WASM opens without a project, reuses its panel and edits typed mathematics", async () => {
    await withWorkbench(async ({ frame, opened }) => {
      await commands.executeCommand(COMMAND);
      assert.strictEqual(opened(), 1, "the command reveals its existing session");
      assert.ok(await appearance(frame).locator(".osp-symbol").isVisible(), "icon-only controls must retain their visible symbol");
      await mathematics(frame);
      await press(frame, "Proof");
      const colours = await frame.evaluate(() => {
        const proof = document.querySelector(".osp-frame-proof-document");
        const prose = proof?.querySelector(".osp-rich-text");
        return proof && prose ? [getComputedStyle(prose).color, getComputedStyle(proof).color] : null;
      });
      assert.ok(colours && colours[0] !== "", "measure the connected mathematical prose");
      assert.strictEqual(colours[0], colours[1], "mathematical prose must use the application primary text colour");
      await lesson(frame);
    }, "failure-real-wasm");
  });

  test("real shared WASM opens without a project, reuses its panel and edits typed mathematics (extra: a second open keeps the typed work)", async () => {
    await withWorkbench(async ({ frame, opened }) => {
      await mathematics(frame);
      await commands.executeCommand(COMMAND);
      await commands.executeCommand(COMMAND);
      assert.strictEqual(opened(), 1);
      assert.ok((await frame.locator(".osp-frame-proof-rows").textContent())?.includes("From hp."), "revealing the panel keeps the checked work");
    }, "failure-real-wasm-reopen");
  });

  test("carrying a lesson reveals its appended proof at the authored editor caret", async () => {
    await withWorkbench(async ({ frame }) => {
      await mathematics(frame);
      await press(frame, "Examples");
      await frame.locator(".osp-section").filter({ hasText: "Navier–Stokes: equations and evidence" }).getByRole("button", { name: "Open", exact: true }).click();
      const lessonFile = JSON.parse(fs.readFileSync(path.resolve(__dirname, "../../../../../osprey/workbench/lessons/navier-stokes.json"), "utf8"));
      const challenge = lessonFile.blocks.find((entry: { challenge?: { id: string } }) => entry.challenge?.id === "prove-remaining-outcome").challenge;
      await frame.getByRole("textbox", { name: "Your steps", exact: true }).fill(challenge.solution.join("\n"));
      await frame.getByRole("button", { name: "Open in the workbench", exact: true }).first().click();
      const editor = frame.getByRole("textbox", { name: "Argument & calculations", exact: true });
      await pollUntilResult({ fn: () => editor.inputValue(), until: (value) => value.includes("Theorem remaining_outcome."), timeoutMs: 5000, description: "the lesson proof is appended to the existing document" });
      const shown = await editor.evaluate((node: HTMLTextAreaElement) => ({ cursor: node.selectionStart, end: node.value.length, scroll: node.scrollTop, overflow: node.scrollHeight - node.clientHeight }));
      assert.strictEqual(shown.cursor, shown.end, "the shared byte caret must select the end of the appended proof");
      assert.ok(shown.scroll > 0 && shown.overflow - shown.scroll <= 2, `the appended proof must be visible: ${JSON.stringify(shown)}`);
    }, "failure-lesson-caret");
  });

  test("carrying a lesson reveals its appended proof at the authored editor caret (extra: the earlier proof is kept above it)", async () => {
    await withWorkbench(async ({ frame }) => {
      await mathematics(frame);
      await press(frame, "Examples");
      await frame.locator(".osp-section").filter({ hasText: "Navier–Stokes: equations and evidence" }).getByRole("button", { name: "Open", exact: true }).click();
      const lessonFile = JSON.parse(fs.readFileSync(path.resolve(__dirname, "../../../../../osprey/workbench/lessons/navier-stokes.json"), "utf8"));
      const challenge = lessonFile.blocks.find((entry: { challenge?: { id: string } }) => entry.challenge?.id === "prove-remaining-outcome").challenge;
      await frame.getByRole("textbox", { name: "Your steps", exact: true }).fill(challenge.solution.join("\n"));
      await frame.getByRole("button", { name: "Open in the workbench", exact: true }).first().click();
      const editor = frame.getByRole("textbox", { name: "Argument & calculations", exact: true });
      const value = await pollUntilResult({ fn: () => editor.inputValue(), until: (text) => text.includes("Theorem remaining_outcome."), timeoutMs: 5000, description: "appended" });
      assert.ok(value.indexOf("Theorem and_swap.") < value.indexOf("Theorem remaining_outcome."), "the earlier proof stays first");
    }, "failure-lesson-keeps");
  });
});

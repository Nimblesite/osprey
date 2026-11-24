// [WORKBENCH-VSIX]: the real Workbench in the test window's webview, its live application instance
// wrapped but never replaced. Every case drives the controls a person would use.
import * as assert from "node:assert/strict";
import * as sinon from "sinon";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { ColorThemeKind, commands } from "vscode";
import type { Frame } from "playwright-core";
import { COMMAND, answer, assertNoFreehandInk, firstLesson, press, waitForIdleWorkbench, withWorkbench } from "./workbench-browser-kit";

const library = (frame: Frame) => frame.locator(".osp-frame-proof-library-inspection");
const formula = (frame: Frame) => frame.getByRole("textbox", { name: "Your formula", exact: true });
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
      await press(frame, "Lessons");
      const open = frame.getByRole("button", { name: "Open", exact: true }).first();
      await open.waitFor();
      await waitForIdleWorkbench(frame);
      const before = runtime.events.length;
      await forgedClick(frame, "Library", String(2 ** 31));
      await waitForIdleWorkbench(frame, 5000);
      assert.deepStrictEqual(runtime.events.slice(before).map((event) => event.ordinal), [2 ** 31], "the forged ordinal reaches the application, which refuses it");
      assert.strictEqual(await open.isVisible(), true, "the refused action must not navigate");
      assert.strictEqual(await frame.getByRole("button", { name: "Library", exact: true }).isEnabled(), true, "an ignored action must not leave controls paused");
    }, "failure-stale-action");
  });

  test("a refused stale action does not leave the workbench busy (extra: a malformed ordinal never reaches the runtime)", async () => {
    await withWorkbench(async ({ frame, runtime }) => {
      await waitForIdleWorkbench(frame);
      const before = runtime.events.length;
      for (const ordinal of ["-1", "0.5", "not a number"]) await forgedClick(frame, "Lessons", ordinal);
      await waitForIdleWorkbench(frame, 5000);
      assert.deepStrictEqual(runtime.events.slice(before).filter((event) => event.kind === 0 && event.ordinal < 0), []);
      assert.strictEqual(await frame.locator(".osp-frame-workbench").count(), 1);
      assert.strictEqual(await frame.getByRole("button", { name: "Lessons", exact: true }).isEnabled(), true);
    }, "failure-malformed-ordinal");
  });

  test("workbench offers no English proof entry and no freehand drawing, only notation", async () => {
    await withWorkbench(async ({ frame }) => {
      await assertNoFreehandInk(frame);
      await library(frame).waitFor();
      for (const name of ["Argument & calculations", "Solution title", "Write the next step…"]) {
        assert.strictEqual(await frame.getByRole("textbox", { name, exact: true }).count(), 0, `the English proof field "${name}" must not exist`);
      }
      for (const name of ["Source", "Read", "Save", "Open workspace", "Open conjunction example"]) {
        assert.strictEqual(await frame.getByRole("button", { name, exact: true }).count(), 0, `the English proof control "${name}" must not exist`);
      }
      await firstLesson(frame);
      await answer(frame, "p → p", "Right");
      await assertNoFreehandInk(frame);
    }, "failure-no-english-entry");
  });

  test("workbench offers no English proof entry and no freehand drawing, only notation (extra: an English answer is never read as mathematics)", async () => {
    await withWorkbench(async ({ frame }) => {
      await firstLesson(frame);
      for (const sentence of ["p implies p", "if p then p"]) {
        await answer(frame, sentence, "Unfinished");
        assert.strictEqual(await frame.getByText("Right", { exact: true }).count(), 0, `"${sentence}" must not be accepted`);
      }
      await answer(frame, "p → p", "Right");
    }, "failure-english-answer");
  });

  test("a pending runtime update pauses actions while preserving the typed formula", async () => {
    await withWorkbench(async ({ frame, runtime }) => {
      await firstLesson(frame);
      const release = runtime.hold();
      try {
        await press(frame, "Hint");
        // The pressed button keeps its node until its gesture's click has been delivered, then pauses too.
        const paused = await pollUntilResult({ fn: () => pausedState(frame), until: (state) => state.busy === "true" && state.disabled === true, timeoutMs: 2000, description: "the held call pauses every control" });
        assert.deepStrictEqual(paused, { busy: "true", disabled: true, notice: "Updating · previous results shown" });
        await formula(frame).fill("p → p");
      } finally { release(); }
      await waitForIdleWorkbench(frame, 5000);
      assert.strictEqual(await formula(frame).inputValue(), "p → p");
      assert.strictEqual(await frame.getByRole("button", { name: "Library", exact: true }).isEnabled(), true);
    }, "failure-pending-runtime");
  });

  test("a pending runtime update pauses actions while preserving the typed formula (extra: released controls keep their own disabled state)", async () => {
    await withWorkbench(async ({ frame, runtime }) => {
      const disabledBefore = await frame.locator("#osprey-root button:disabled").count();
      const release = runtime.hold();
      try {
        await press(frame, "Lessons");
        await pollUntilResult({ fn: () => pausedState(frame), until: (state) => state.busy === "true", timeoutMs: 2000, description: "busy" });
      } finally { release(); }
      await waitForIdleWorkbench(frame, 5000);
      await press(frame, "Library");
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
        await firstLesson(frame);
        await formula(frame).fill("p → p");
        await appearance(frame).click();
        await appearance(frame).locator(`[data-symbol="${symbol === "moon" ? "sun.max" : "moon"}"]`).waitFor();
        assert.strictEqual(await formula(frame).inputValue(), "p → p");
      }, `failure-startup-theme-${kind}`, kind);
    });

    test(`startup appearance follows editor theme ${kind} and manual toggle preserves work (extra: two toggles return to the startup canvas)`, async () => {
      await withWorkbench(async ({ frame }) => {
        await firstLesson(frame);
        await formula(frame).fill("p ∧ q");
        for (const next of [symbol === "moon" ? "sun.max" : "moon", symbol]) {
          await appearance(frame).click();
          await appearance(frame).locator(`[data-symbol="${next}"]`).waitFor();
        }
        assert.strictEqual(await frame.locator(".osp-frame-workbench").evaluate((node) => getComputedStyle(node).backgroundColor), colour);
        assert.strictEqual(await formula(frame).inputValue(), "p ∧ q");
      }, `failure-startup-theme-round-${kind}`, kind);
    });
  }

  test("reference icons and theme roundtrips use actual shared SVG assets", async () => {
    await withWorkbench(async ({ frame }) => {
      const icon = appearance(frame).locator(".osp-symbol");
      assert.ok(await icon.isVisible(), "Switch appearance retains its visible source asset");
      assert.strictEqual(await icon.getAttribute("data-symbol"), "moon");
      const mask = await icon.evaluate((node) => getComputedStyle(node).maskImage);
      assert.ok(mask.includes("icons/moon.svg"), `Switch appearance masks with moon.svg, got ${mask}`);
      assert.strictEqual((mask.match(/url\(/g) ?? []).length, 1, "Switch appearance has exactly one mask image");
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
      await press(frame, "Natural arithmetic");
      const picker = frame.getByRole("combobox", { name: "Theorem", exact: true });
      const selected = await picker.inputValue();
      // [UI-COMPONENTS] A picker is a menu button: it shows the surface behind it,
      // and its ink is the application's for the appearance in force.
      const inks: string[] = [];
      for (const [index, symbol] of ["moon", "sun.max", "moon"].entries()) {
        if (index > 0) await press(frame, "Switch appearance");
        await appearance(frame).locator(`[data-symbol="${symbol}"]`).waitFor();
        const drawn = await picker.evaluate((node) => {
          const style = getComputedStyle(node);
          return { ink: style.color, fill: style.backgroundColor };
        });
        assert.strictEqual(drawn.fill, "rgba(0, 0, 0, 0)", "the picker has no fill of its own");
        inks.push(drawn.ink);
        assert.strictEqual(await picker.inputValue(), selected);
      }
      assert.notStrictEqual(inks[1], inks[0], "the picker's ink follows the application's appearance");
      assert.strictEqual(inks[2], inks[0], "switching back restores the first appearance's ink");
    }, "failure-field-appearance");
  });

  test("app-owned field colours follow manual appearance changes and retain the selected theorem (extra: the formula field follows too)", async () => {
    await withWorkbench(async ({ frame }) => {
      await firstLesson(frame);
      await formula(frame).fill("p → p");
      const light = await formula(frame).evaluate((node) => getComputedStyle(node).color);
      await press(frame, "Switch appearance");
      await appearance(frame).locator('[data-symbol="sun.max"]').waitFor();
      const dark = await formula(frame).evaluate((node) => getComputedStyle(node).color);
      assert.notStrictEqual(dark, light, "the field's ink follows the application's appearance");
      assert.strictEqual(await formula(frame).inputValue(), "p → p");
    }, "failure-formula-appearance");
  });

  test("Lessons leaves imported proof inspection and Library restores it", async () => {
    await withWorkbench(async ({ frame }) => {
      await press(frame, "Natural arithmetic");
      await frame.getByText("Checked theorem", { exact: true }).waitFor();
      await press(frame, "Lessons");
      await waitForIdleWorkbench(frame, 5000);
      assert.strictEqual(await library(frame).count(), 0, "Lessons must leave imported-library inspection");
      await press(frame, "Library");
      await frame.getByText("Checked theorem", { exact: true }).waitFor();
    }, "failure-lessons-leave-library");
  });

  test("Lessons leaves imported proof inspection and Library restores it (extra: the selected theorem survives the round trip)", async () => {
    await withWorkbench(async ({ frame }) => {
      await press(frame, "Natural arithmetic");
      const picker = frame.getByRole("combobox", { name: "Theorem", exact: true });
      const selected = await picker.inputValue();
      await firstLesson(frame);
      await press(frame, "Library");
      assert.strictEqual(await picker.inputValue(), selected);
    }, "failure-library-return");
  });

  test("the shared app exposes its Osprey branding and reference links", async () => {
    await withWorkbench(async ({ frame }) => {
      assert.strictEqual(await frame.getByRole("link", { name: "Built with Osprey", exact: true }).getAttribute("href"), "https://ospreylang.dev/");
      await frame.getByRole("img", { name: "Osprey", exact: true }).first().waitFor();
      for (const [name, href] of [["Osprey website", "https://ospreylang.dev/"], ["Language documentation", "https://ospreylang.dev/docs/"], ["Workbench source", "https://github.com/Nimblesite/osprey/tree/main/src/osprey/workbench"]]) {
        assert.strictEqual(await frame.getByRole("link", { name, exact: true }).getAttribute("href"), href);
      }
    }, "failure-branding");
  });

  test("the shared app exposes its Osprey branding and reference links (extra: every link is HTTPS and opens outside the webview)", async () => {
    await withWorkbench(async ({ frame }) => {
      await frame.getByRole("link", { name: "Workbench source", exact: true }).waitFor();
      const links = await frame.locator("#osprey-root a[href]").evaluateAll((nodes) => nodes.map((node) => node.getAttribute("href") ?? ""));
      assert.ok(links.length >= 3, links.join(","));
      for (const href of links) assert.ok(href.startsWith("https://"), href);
    }, "failure-branding-links");
  });

  test("real shared WASM opens without a project, reuses its panel and checks typed notation", async () => {
    await withWorkbench(async ({ frame, opened }) => {
      await commands.executeCommand(COMMAND);
      assert.strictEqual(opened(), 1, "the command reveals its existing session");
      assert.ok(await appearance(frame).locator(".osp-symbol").isVisible(), "icon-only controls must retain their visible symbol");
      await firstLesson(frame);
      await answer(frame, "p → p", "Right");
    }, "failure-real-wasm");
  });

  test("real shared WASM opens without a project, reuses its panel and checks typed notation (extra: a second open keeps the typed formula)", async () => {
    await withWorkbench(async ({ frame, opened }) => {
      await firstLesson(frame);
      await formula(frame).fill("p → p");
      await commands.executeCommand(COMMAND);
      await commands.executeCommand(COMMAND);
      assert.strictEqual(opened(), 1);
      assert.strictEqual(await formula(frame).inputValue(), "p → p", "revealing the panel keeps the typed formula");
    }, "failure-real-wasm-reopen");
  });
});

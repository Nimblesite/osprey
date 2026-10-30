import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import * as sinon from "sinon";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { ColorThemeKind, commands, Uri, window } from "vscode";
import { COMMAND, draw, mathematics, viewport, capture, lesson, withWorkbench, frameSource, retainedImport, openProofLibrary, checkedProofFits, checkedMathematicsFits, checkedEquationFits, checkedRunWorkMathematics, sourceCloseBounds, selectImportedProof, rejectEditedImport, saveWorking, reopenWorking, exportWorking, comparisonState } from "./workbench-browser-kit";


suite("workbench VSIX browser", () => {
  teardown(() => sinon.restore());
  for (const [kind, symbol, colour] of [[ColorThemeKind.Dark, "sun.max", "rgb(32, 39, 35)"],
    [ColorThemeKind.Light, "moon", "rgb(250, 250, 247)"]] as const) {
    test(`startup appearance follows editor theme ${kind} and manual toggle preserves work`, async () => {
      await withWorkbench(async frame => {
        const theme = frame.getByRole("button", { name: "Switch appearance", exact: true });
        assert.strictEqual(await theme.locator(".osp-symbol").getAttribute("data-symbol"), symbol);
        assert.strictEqual(await frame.locator(".osp-frame-workbench").evaluate(node => getComputedStyle(node).backgroundColor), colour);
        await frame.getByRole("textbox", { name: "Solution title", exact: true }).fill("Keep my theme work");
        await theme.click();
        await theme.locator(`[data-symbol="${symbol === "moon" ? "sun.max" : "moon"}"]`).waitFor();
        assert.strictEqual(await frame.getByRole("textbox", { name: "Solution title", exact: true }).inputValue(), "Keep my theme work");
      }, `failure-startup-theme-${kind}`, kind);
    });
  }
  test("reference icons and theme roundtrips use actual shared SVG assets", async () => {
    await withWorkbench(async frame => {
      const theme = frame.getByRole("button", { name: "Switch appearance", exact: true });
      for (const [label, symbol, file] of [["Switch appearance", "moon", "moon"], ["Open workspace", "folder.open", "folder-open"], ["Save", "square.and.arrow.down", "floppy-disk"]]) {
        const icon = frame.getByRole("button", { name: label, exact: true }).locator(".osp-symbol");
        assert.ok(await icon.isVisible(), `${label} retains its visible source asset`);
        assert.strictEqual(await icon.getAttribute("data-symbol"), symbol);
        assert.ok((await icon.evaluate(node => getComputedStyle(node).maskImage)).includes(`icons/${file}.svg`));
      }
      await theme.click();
      await theme.locator('[data-symbol="sun.max"]').waitFor();
      assert.ok((await theme.locator(".osp-symbol").evaluate(node => getComputedStyle(node).maskImage)).includes("icons/sun.svg"));
      await theme.click();
      await theme.locator('[data-symbol="moon"]').waitFor();
    }, "failure-reference-icons");
  });
  test("app-owned field colours follow manual appearance changes and retain the selected theorem", async () => {
    await withWorkbench(async frame => {
      await frame.getByRole("button", { name: "Notation", exact: true }).click();
      await frame.getByRole("button", { name: "Natural arithmetic", exact: true }).click();
      const picker = frame.getByRole("combobox", { name: "Theorem", exact: true }), selected = await picker.inputValue();
      for (const [index, [symbol, background]] of [["moon", "rgb(255, 255, 255)"], ["sun.max", "rgb(41, 50, 45)"], ["moon", "rgb(255, 255, 255)"]].entries()) {
        if (index > 0) await frame.getByRole("button", { name: "Switch appearance", exact: true }).click();
        await frame.getByRole("button", { name: "Switch appearance", exact: true }).locator(`[data-symbol="${symbol}"]`).waitFor();
        assert.strictEqual(await picker.evaluate(node => getComputedStyle(node).backgroundColor), background);
        assert.strictEqual(await picker.inputValue(), selected);
      }
    }, "failure-field-appearance");
  });
  test("source heading symbols use their shared frame's accent ink", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "optional-source");
      const color = await frame.locator(".osp-frame-source-heading .osp-icon").evaluate(element => getComputedStyle(element).color);
      assert.strictEqual(color, "rgb(54, 99, 92)");
    }, "failure-source-ink");
  });
  test("retained imported verdicts disappear when actual host source is lost or corrupted", async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-retained-proof-"));
    const source = fs.readFileSync(path.resolve(__dirname, "../../../../examples/proof/kernel/exports/id_imp.ndjson"), "utf8");
    const file = path.join(directory, "proof.ndjson");
    fs.writeFileSync(file, " ".repeat(1048577) + source);
    try {
      for (const replacement of ["", source.replace('"value":6', '"value":4'), source.replace('"str":"p"', '"str":"q"')]) {
        sinon.stub(window, "showOpenDialog").resolves([Uri.file(file)]);
        await retainedImport(frameSource(replacement), replacement.includes('"str":"q"'), async frame => {
          await openProofLibrary(frame);
          await selectImportedProof(frame, /^id_imp(?:\s|$)/);
        });
        sinon.restore();
      }
    } finally { fs.rmSync(directory, { recursive: true, force: true }); }
  });
  test("an imported arithmetic theorem is checked from current editable export and corruption removes its verdict", async () => {
    const file = path.resolve(__dirname, "../../../../examples/proof/kernel/exports/nat_add_comm.ndjson");
    sinon.stub(window, "showOpenDialog").resolves([Uri.file(file)]);
    await withWorkbench(async frame => {
      await openProofLibrary(frame);
      await selectImportedProof(frame, /^Nat\.add_comm(?:\s|$)/);
      await rejectEditedImport(frame, fs.readFileSync(file, "utf8"));
      await selectImportedProof(frame, /^Nat\.add_comm(?:\s|$)/);
      await capture(frame, "imported-arithmetic-proof");
    }, "failure-imported-arithmetic");
  });
  test("an imported OpenAI supporting lemma displays its actual axiom dependency", async () => {
    const file = path.resolve(__dirname, "../../../../examples/proof/openai/stacks.ndjson");
    sinon.stub(window, "showOpenDialog").resolves([Uri.file(file)]);
    await withWorkbench(async frame => {
      await openProofLibrary(frame);
      await selectImportedProof(frame, /^OAI\.PrefixFlows\.Interspersed\.push_inj(?:\s|$)/);
      await frame.getByText("Axioms: Quot.sound", { exact: true }).waitFor();
      await capture(frame, "imported-openai-proof");
    }, "failure-imported-openai");
  });
  for (const [library, theorem, axioms, name] of [
    ["Natural arithmetic", "Nat.add_comm", "none", "arithmetic"],
    ["OpenAI · infinite-stack lemmas", "OAI.PrefixFlows.Interspersed.push_inj", "Quot.sound", "openai"],
    ["OpenAI · doubling-loop lemmas", "OAI.LowSpaceSubsetSum.WordSeedPowerExecution.run_work", "propext", "doubling-loop"],
    ["List lemmas", "List.map_id", "propext, Quot.sound", "large-list"],
    ["OpenAI · five-colour orientation", "OAI.TwoPointCorrelations.five_color_isolates_forward", "propext", "five-colour"],
  ]) test(`bundled proof library ${name} checks the actual packaged export without a file picker`, async () => {
    const picker = sinon.spy(window, "showOpenDialog");
    await withWorkbench(async frame => {
      await frame.getByRole("button", { name: "Notation", exact: true }).click();
      await frame.getByRole("button", { name: library, exact: true }).click();
      await selectImportedProof(frame, new RegExp(`^${theorem.replace(/\./g, "\\.")}(?:\\s|$)`));
      await frame.getByText(`Axioms: ${axioms}`, { exact: true }).waitFor();
      assert.strictEqual(picker.callCount, 0, "bundled discovery must read approved assets directly");
      await checkedProofFits(frame, theorem, `Axioms: ${axioms}`);
      if (name === "doubling-loop") await checkedRunWorkMathematics(frame);
      if (name === "five-colour") {
        const source = fs.readFileSync(path.resolve(__dirname, "../../../../examples/proof/openai-number-theory/exports/five-color-orientation.ndjson"), "utf8");
        await rejectEditedImport(frame, source, text => text.replace('"name":4843,"type":36885,"value":36932', '"name":4843,"type":36885,"value":36885'));
      } else { await checkedMathematicsFits(frame); await capture(frame, `bundled-${name}-proof`); }
    }, `failure-bundled-${name}`);
  });
  for (const view of ["Proof", "Source"]) test(`the checked arithmetic statement fits completely in ${view} notation view`, async () => {
    await withWorkbench(async frame => {
      await frame.getByRole("button", { name: view, exact: true }).click();
      await frame.getByRole("button", { name: "Notation", exact: true }).click();
      await frame.getByRole("button", { name: "Natural arithmetic", exact: true }).click();
      await selectImportedProof(frame, /^Nat\.add_comm(?:\s|$)/);
      await checkedEquationFits(frame);
    });
  });
  test("the shared app exposes its Osprey branding and reference links", async () => {
    await withWorkbench(async frame => {
      assert.strictEqual(await frame.getByRole("link", { name: "Built with Osprey", exact: true }).getAttribute("href"), "https://ospreylang.dev/");
      await frame.getByRole("button", { name: "Notation", exact: true }).click();
      await frame.getByRole("img", { name: "Osprey", exact: true }).waitFor();
      for (const [name, href] of [["Osprey website", "https://ospreylang.dev/"], ["Language documentation", "https://ospreylang.dev/docs/"], ["Workbench source", "https://github.com/Nimblesite/osprey/tree/main/apps/workbench"]])
        assert.strictEqual(await frame.getByRole("link", { name, exact: true }).getAttribute("href"), href);
    }, "failure-branding");
  });
  test("shared Osprey saves and reopens source and strokes and exports its generated proof", async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-workbench-documents-"));
    const saved = path.join(root, "working.json"), exported = path.join(root, "proof.ospml");
    sinon.stub(window, "showSaveDialog").onFirstCall().resolves(Uri.file(saved)).onSecondCall().resolves(Uri.file(exported));
    sinon.stub(window, "showOpenDialog").resolves([Uri.file(saved)]);
    try {
      await withWorkbench(async frame => { await saveWorking(frame, saved); await reopenWorking(frame, saved); await exportWorking(frame, exported); }, "failure-documents");
    } finally { fs.rmSync(root, { recursive: true, force: true }); }
  });

  test("real shared WASM opens without a project, reuses its panel, edits mathematics and draws", async () => {
    await withWorkbench(async (frame, opened) => {
      await commands.executeCommand(COMMAND);
      assert.strictEqual(opened(), 1, "command reveals its existing session");
      await capture(frame, "blank-board");
      assert.ok(await frame.getByRole("button", { name: "Switch appearance", exact: true }).locator(".osp-symbol").isVisible(), "icon-only controls must retain their visible symbol");
      await draw(frame);
      await mathematics(frame);
      await frame.getByRole("button", { name: "Proof", exact: true }).click();
      const colours = await frame.evaluate(() => {
        const proof = document.querySelector(".osp-frame-proof-document"), prose = proof?.querySelector(".osp-rich-text");
        return proof && prose ? [getComputedStyle(prose).color, getComputedStyle(proof).color] : null;
      });
      assert.ok(colours && colours[0] !== "", "measure the connected mathematical prose");
      assert.strictEqual(colours[0], colours[1], "mathematical prose must use the application primary text colour");
      await capture(frame, "checked-proof");
      await lesson(frame);
    });
  });
  test("carrying a lesson reveals its appended proof at the authored editor caret", async () => {
    await withWorkbench(async frame => {
      await mathematics(frame);
      await frame.getByRole("button", { name: "Examples", exact: true }).click();
      await frame.locator(".osp-section").filter({ hasText: "Navier–Stokes: equations and evidence" }).getByRole("button", { name: "Open", exact: true }).click();
      const lesson = JSON.parse(fs.readFileSync(path.resolve(__dirname, "../../../../apps/workbench/lessons/navier-stokes.json"), "utf8"));
      const challenge = lesson.blocks.find((entry: { challenge?: { id: string } }) => entry.challenge?.id === "prove-remaining-outcome").challenge;
      await frame.getByRole("textbox", { name: "Your steps", exact: true }).fill(challenge.solution.join("\n"));
      await frame.getByRole("button", { name: "Open in the workbench", exact: true }).first().click();
      const editor = frame.getByRole("textbox", { name: "Argument & calculations", exact: true });
      await pollUntilResult({ fn: () => editor.inputValue(), until: value => value.includes("Theorem remaining_outcome."), timeoutMs: 1000, description: "the lesson proof is appended to the existing document" });
      const shown = await editor.evaluate((node: HTMLTextAreaElement) => ({ cursor: node.selectionStart, end: node.value.length, scroll: node.scrollTop, overflow: node.scrollHeight - node.clientHeight }));
      assert.strictEqual(shown.cursor, shown.end, "the shared byte caret must select the end of the appended proof");
      assert.ok(shown.scroll > 0 && shown.overflow - shown.scroll <= 2, `the appended proof must be visible: ${JSON.stringify(shown)}`);
    }, "failure-lesson-caret");
  });
});

suite("workbench VSIX visual comparison", () => {
  teardown(() => sinon.restore());
  test("transcription preserves the reference board height and filled primary action", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "transcription");
      await viewport(frame, { width: 1488, height: 1058 });
      const layout = await frame.evaluate(() => {
        const board = document.querySelector(".osp-frame-working-board")!.getBoundingClientRect();
        const action = document.querySelector('button[aria-label="Use this sample statement"]')!;
        return { bottom: board.bottom, fill: getComputedStyle(action).backgroundColor };
      });
      assert.ok(Math.abs(layout.bottom - 990) <= 5, `reference board ends at y990: ${JSON.stringify(layout)}`);
      assert.strictEqual(layout.fill, "rgb(54, 99, 92)", "the reference primary action retains its green fill");
      const symbol = await frame.getByRole("button", { name: "Zoom out", exact: true }).locator(".osp-symbol").evaluate(node => ({ size: getComputedStyle(node).fontSize, width: node.getBoundingClientRect().width }));
      assert.deepStrictEqual(symbol, { size: "13px", width: 13 }, "the board footer retains its authored compact symbols");
      await capture(frame, "transcription");
    }, "failure-transcription-reference");
  });
  test("source manuscript keeps the board action outside the goal divider and authors its title break", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "optional-source");
      const outside = await frame.getByRole("button", { name: "Show working board", exact: true }).evaluate(element => element.closest(".osp-aside") === null);
      const title = await frame.locator(".osp-frame-goal-content .osp-role-heading").first().innerText();
      assert.deepStrictEqual({ outside, title }, { outside: true, title: "The draft is\ncomplete." });
    }, "failure-source-manuscript");
  });
  test("source geometry uses the measured reference panel and code baselines", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "optional-source");
      const panel = await frame.locator(".osp-frame-source-panel").evaluate(element => ({ height: element.getBoundingClientRect().height }));
      const code = await frame.locator(".osp-frame-source-code").evaluate(element => ({ y: element.getBoundingClientRect().y, height: element.getBoundingClientRect().height }));
      assert.ok(panel && code);
      assert.ok(Math.abs(panel.height - 536.59) < 0.75, `reference source panel: ${panel.height}`);
      assert.ok(Math.abs(code.y - 331.75) < 0.75, `reference code top: ${code.y}`);
      assert.ok(Math.abs(code.height - 271.34) < 0.75, `reference code height: ${code.height}`);
      const micro = await frame.locator(".osp-frame-goal-content").getByText("Hypotheses", { exact: true }).boundingBox();
      assert.ok(micro && Math.abs(micro.height - 16) < 0.1);
    }, "failure-source-geometry");
  });
  test("proposition glyphs align with their goal caption instead of centering", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "board-first");
      await frame.getByText("Propositions", { exact: true }).waitFor();
      const bounds = await frame.evaluate(() => {
        const caption = Array.from(document.querySelectorAll(".osp-frame-goal-content .osp-role-caption")).find(node => node.textContent === "Propositions");
        const glyph = document.querySelector('[data-region-key="goal"] math[aria-label="p, q"] mi');
        return { caption: caption?.getBoundingClientRect().x ?? null, glyph: glyph?.getBoundingClientRect().x ?? null };
      });
      assert.ok(bounds.caption !== null && bounds.glyph !== null && Math.abs(bounds.caption - bounds.glyph) <= 2, `propositions share the reference left edge: ${JSON.stringify(bounds)}`);
    });
  });
  test("slate hypotheses share the reference baseline", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "slate-board");
      const [hp, hq] = await frame.evaluate(() => ["hp: p", "hq: q"].map(label =>
        document.querySelector(`[data-region-key="goal"] math[aria-label="${label}"]`)?.getBoundingClientRect().y));
      assert.ok(hp !== undefined && hq !== undefined && Math.abs(hp - hq) < 0.5, `the reference places hypothesis labels on one baseline: ${hp}, ${hq}`);
    }, "failure-slate-hypotheses");
  });
  test("the browser preserves the shared slate heading's authored line break", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "slate-board");
      const heading = frame.locator(".osp-frame-proof-document .osp-role-display");
      assert.strictEqual(await heading.innerText(), "Commutativity\nof conjunction");
    }, "failure-heading-line-break");
  });
  test("source actions preserve the reference outlines icon and route back to the board", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "optional-source");
      for (const name of ["Export generated source", "Close source"]) {
        const border = await frame.getByRole("button", { name, exact: true }).evaluate(element => {
          const style = getComputedStyle(element.parentElement!);
          return [style.borderTopWidth, style.borderRightWidth, style.borderBottomWidth, style.borderLeftWidth];
        });
        assert.deepStrictEqual(border, ["1px", "1px", "1px", "1px"], `${name} must retain its authored outline`);
      }
      assert.strictEqual(await frame.locator(".osp-frame-source-heading .osp-icon").count(), 1);
      const close = await sourceCloseBounds(frame);
      assert.deepStrictEqual(close, { width: 32, height: 32, top: 12, right: 12 }, "source close control matches the reference inset and size");
      await frame.getByRole("button", { name: "Show working board", exact: true }).click();
      await frame.locator("canvas[data-board-slice]").waitFor();
      assert.strictEqual(await frame.locator(".osp-frame-source-panel").count(), 0);
    }, "failure-source-actions");
  });
  test("editing a checked proof row reports kernel refusal and recovers when corrected", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "optional-source");
      await frame.getByText("Checked", { exact: true }).waitFor();
      const row = frame.locator('.osp-frame-proof-rows [role="option"]').filter({ hasText: "From hq." });
      await row.click();
      const editor = frame.getByRole("textbox", { name: "Write the next step…", exact: true });
      await pollUntilResult({ fn: () => editor.inputValue(), until: text => text.trim() === "From hq.", timeoutMs: 1000, description: "select the original checked proof row" });
      await editor.fill("From hp.");
      await frame.getByText("Refused", { exact: true }).waitFor({ timeout: 2000 });
      await capture(frame, "refused-proof-row");
      await editor.fill("From hq.");
      await frame.getByText("Checked", { exact: true }).waitFor({ timeout: 2000 });
      assert.strictEqual(await frame.getByText("Refused", { exact: true }).count(), 0);
    }, "failure-proof-row");
  });
  test("Source opens the paper document alongside generated source", async () => {
    await withWorkbench(async frame => {
      await frame.getByRole("button", { name: "Source", exact: true }).click();
      await frame.locator(".osp-frame-source-panel").waitFor();
      await frame.locator(".osp-frame-paper-document").waitFor({ timeout: 1000 });
      const selected = (await frame.getByRole("button", { name: "Source", exact: true }).getAttribute("class"))?.includes("osp-button-prominent");
      assert.strictEqual(selected, true, "Source navigation shows its selected state while the panel is open");
    });
  });
  test("guided context keeps its caption beside the return action", async () => {
    await withWorkbench(async frame => {
      await frame.getByRole("button", { name: "Examples", exact: true }).click();
      await frame.getByRole("button", { name: "Open conjunction example", exact: true }).click();
      await frame.getByRole("button", { name: "Return to working", exact: true }).waitFor();
      const gap = await frame.locator(".osp-frame-example-context").evaluate(node => {
        const action = node.querySelector("button")!.getBoundingClientRect();
        const caption = node.querySelector(".osp-role-caption")!.getBoundingClientRect();
        return caption.left - action.right;
      });
      assert.ok(Math.abs(gap - 37) <= 1, `the reference caption follows two18px gaps and its1px divider: ${gap}`);
    }, "failure-guided-context-gap");
  });
  test("the proof entry fills its row and keeps Continue at the trailing edge", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "paper-proof");
      const bounds = await frame.locator(".osp-frame-proof-step-editor").evaluate(node => {
        const row = node.getBoundingClientRect(), action = node.querySelector("button")!.getBoundingClientRect();
        return { right: row.right, actionRight: action.right };
      });
      assert.ok(bounds.right - bounds.actionRight <= 12, `the reference Continue action belongs at the row edge: ${JSON.stringify(bounds)}`);
    });
  });
  test("the board proof editor stays within its authored compact row", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "board-first");
      await frame.getByRole("textbox", { name: "Write the next step…", exact: true }).waitFor({ state: "visible" });
      const bounds = await frame.locator(".osp-frame-proof-step-editor").evaluate(node => {
        const row = node.getBoundingClientRect(), editor = node.querySelector("textarea")!.getBoundingClientRect();
        return { height: row.height, top: editor.top - row.top, bottom: row.bottom - editor.bottom };
      });
      assert.strictEqual(bounds.height, 49, "the board entry uses the shared 49px reference geometry");
      assert.ok(bounds.top >= 0 && bounds.bottom >= 0, `the editor fits inside its authored row: ${JSON.stringify(bounds)}`);
    }, "failure-compact-entry");
  });
  test("document dividers use the shared palette without a host-theme border", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "optional-source");
      await frame.locator(".osp-frame-source-panel").waitFor();
      const ink = await frame.evaluate(() => {
        const node = document.querySelector(".osp-frame-proof-document .osp-divider")!;
        const style = getComputedStyle(node); return { border: style.borderTopWidth, background: style.backgroundColor };
      });
      assert.deepStrictEqual(ink, { border: "0px", background: "rgb(215, 221, 213)" });
    });
  });
  test("shared frame padding owns proof-document spacing", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "optional-source");
      const margins = await frame.locator(".osp-frame-proof-document .osp-frame-claim, .osp-frame-proof-heading").evaluateAll(nodes => nodes.map(node => {
        const style = getComputedStyle(node); return [style.marginTop, style.marginBottom];
      }));
      const contained = await frame.locator(".osp-frame-example-context").evaluate(node => {
        const box = node.getBoundingClientRect();
        return Array.from(node.querySelectorAll(".osp-axis-horizontal, .osp-divider")).every(child => {
          const inner = child.getBoundingClientRect(); return inner.top >= box.top && inner.bottom <= box.bottom;
        });
      });
      const sized = await frame.locator(".osp-frame-proof-document .osp-frame-claim").evaluate(node => {
        const style = getComputedStyle(node), math = node.querySelector("math");
        const expected = parseFloat(style.getPropertyValue("--osp-frame-text-size")) * Number(style.getPropertyValue("--osp-frame-math-scale") || 1);
        return math !== null && Math.abs(parseFloat(getComputedStyle(math).fontSize) - expected) < 0.01;
      });
      assert.deepStrictEqual({ margins, contained, sized }, { margins: [["0px", "0px"], ["0px", "0px"]], contained: true, sized: true }, "shared frame metadata owns spacing and type size; context content stays within its frame");
    });
  });
  test("Board navigation closes generated source and restores the working surface", async () => {
    await withWorkbench(async frame => {
      await frame.getByRole("button", { name: "Source", exact: true }).click();
      await frame.locator(".osp-frame-source-panel").waitFor();
      await frame.getByRole("button", { name: "Board", exact: true }).click();
      await frame.locator("canvas[data-board-slice]").waitFor();
      assert.strictEqual(await frame.locator(".osp-frame-source-panel").count(), 0, "Board navigation closes the source panel");
      assert.ok((await frame.getByRole("button", { name: "Board", exact: true }).getAttribute("class"))?.includes("osp-button-prominent"));
    }, "failure-board-navigation");
  });
  for (const scene of ["blank-board", "working-session", "board-first", "paper-proof", "mismatch", "mismatch-live", "unsupported-input", "optional-source", "reading", "slate-board", "transcription", "tablet-board", "phone-board", "phone-source"]) {
    test(`shared WASM reference state: ${scene}`, async () => {
      await withWorkbench(async frame => { await comparisonState(frame, scene); await capture(frame, scene); }, `failure-${scene}`);
    });
  }
  test("the conjunction example remains reachable at tablet width", async () => {
    await withWorkbench(async frame => {
      await viewport(frame, { width: 834, height: 1194 });
      await comparisonState(frame, "paper-proof");
    });
  });
  test("the grouped phone header keeps every action visible within one row", async () => {
    await withWorkbench(async frame => {
      await viewport(frame, { width: 390, height: 844 });
      const geometry = await frame.evaluate(() => {
        const header = document.querySelector(".osp-frame-app-header")!.getBoundingClientRect(), navigation = document.querySelector(".osp-frame-workspace-nav")!;
        return { header: navigation.getBoundingClientRect().top - header.top,
          navigation: navigation.getBoundingClientRect().height + navigation.nextElementSibling!.getBoundingClientRect().height,
          wordmark: getComputedStyle(document.querySelector(".osp-frame-wordmark .osp-role-title")!).fontSize, actions: getComputedStyle(document.querySelector(".osp-frame-header-actions > .osp-axis-horizontal")!).gap,
          gap: getComputedStyle(navigation.firstElementChild!).gap, board: getComputedStyle(navigation.querySelector('button[aria-label="Board"]')!).paddingLeft, tools: getComputedStyle(navigation.querySelector('button[aria-label="Notation"]')!).paddingLeft };
      });
      assert.deepStrictEqual(geometry, { header: 70, navigation: 58, wordmark: "21px", actions: "2px", gap: "0px", board: "12px", tools: "9px" }, "the compact reference header retains its measured geometry");
      const bounds = await frame.locator(".osp-frame-header-actions button").evaluateAll(nodes => nodes.map(node => {
        const box = node.getBoundingClientRect(); return { label: node.getAttribute("aria-label"), left: box.left, right: box.right, top: box.top, bottom: box.bottom, width: box.width, height: box.height };
      }));
      assert.strictEqual(bounds.length, 4);
      assert.ok(bounds.every(box => box.left >= 0 && box.right <= 390 && box.top >= 0 && box.bottom <= 70 && box.width === 40 && box.height === 44), `phone header actions fit the visible header: ${JSON.stringify(bounds)}`);
    }, "failure-phone-header");
  });
  test("phone Source keeps the target beside the completed draft context", async () => {
    await withWorkbench(async frame => {
      await viewport(frame, { width: 390, height: 844 });
      await comparisonState(frame, "phone-source");
      const context = await frame.locator(".osp-frame-example-context").evaluate(node => ({ height: node.getBoundingClientRect().height,
        color: getComputedStyle(node.querySelector('button[aria-label="Return to working"]')!).color }));
      assert.deepStrictEqual(context, { height: 40, color: "rgb(54, 99, 92)" }, "the compact guided context matches the reference geometry and accent");
      const layout = await frame.locator(".osp-frame-goal-content").evaluate(node => {
        const goal = node.getBoundingClientRect(), heading = node.querySelector(".osp-role-heading")!.getBoundingClientRect();
        const target = Array.from(node.querySelectorAll(".osp-role-caption")).find(item => item.textContent === "To show")!.getBoundingClientRect();
        return { middle: goal.left + goal.width / 2, targetLeft: target.left, targetTop: target.top, headingBottom: heading.bottom };
      });
      assert.ok(layout.targetLeft >= layout.middle && layout.targetTop < layout.headingBottom, `the reference phone Source goal uses two adjacent columns: ${JSON.stringify(layout)}`);
    }, "failure-phone-source-columns");
  });
});

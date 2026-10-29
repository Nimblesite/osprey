import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import * as os from "node:os";
import * as sinon from "sinon";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { commands, Uri, window } from "vscode";
import { COMMAND, draw, mathematics, viewport, capture, lesson, withWorkbench, frameSource, retainedImport, openProofLibrary, checkedProofFits, checkedMathematicsFits, sourceCloseBounds, selectImportedProof, rejectEditedImport, saveWorking, reopenWorking, exportWorking, comparisonState } from "./workbench-browser-kit";


suite("workbench VSIX browser", () => {
  teardown(() => sinon.restore());
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
    ["List lemmas", "List.map_id", "propext, Quot.sound", "large-list"],
  ]) test(`bundled proof library ${name} checks the actual packaged export without a file picker`, async () => {
    const picker = sinon.spy(window, "showOpenDialog");
    await withWorkbench(async frame => {
      await frame.getByRole("button", { name: "Notation", exact: true }).click();
      await frame.getByRole("button", { name: library, exact: true }).click();
      await selectImportedProof(frame, new RegExp(`^${theorem.replace(/\./g, "\\.")}(?:\\s|$)`));
      await frame.getByText(`Axioms: ${axioms}`, { exact: true }).waitFor();
      assert.strictEqual(picker.callCount, 0, "bundled discovery must read approved assets directly");
      await checkedProofFits(frame, theorem, `Axioms: ${axioms}`);
      await checkedMathematicsFits(frame);
      await capture(frame, `bundled-${name}-proof`);
    }, `failure-bundled-${name}`);
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
  test("document dividers use the shared palette without a host-theme border", async () => {
    await withWorkbench(async frame => {
      await comparisonState(frame, "optional-source");
      const ink = await frame.locator(".osp-frame-proof-document .osp-divider").evaluate(node => {
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
});

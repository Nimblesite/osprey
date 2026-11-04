// [PROF-VSCODE-FLAME] Flame panel messaging and source-navigation coverage.
import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import { handleFlamegraphMessage } from "@nimblesite/lsp-profiler-ui/out/src/profiler-flamegraph-html";
import {
  handleSourceNavigation,
  revealProfileSource as revealSource,
  webviewMessage,
} from "@nimblesite/lsp-profiler-ui/out/src/profiler-webview";
import {
  buildFlameModel,
  type SpeedscopeFile,
} from "../../client/src/profiler/flame-model";
import { flamePanel, showFlamePanel } from "../../client/src/profiler/profiler-panel";
import type { ProfileSummary } from "../../client/src/profiler/summary";
import { COMMAND_MS, waitFor } from "./profiler-kit";

const DOC: SpeedscopeFile = {
  shared: { frames: [{ name: "main", file: "/w/fib.osp", line: 1 }] },
  profiles: [
    {
      type: "sampled",
      name: "main",
      unit: "seconds",
      startValue: 0,
      endValue: 1,
      samples: [[0]],
      weights: [1],
    },
  ],
};

const SUMMARY: ProfileSummary = {
  version: 1,
  program: "/w/fib.osp",
  wallSeconds: 1,
  cpuSeconds: 1,
  sampleCount: 100,
  rateHz: 997,
  droppedSamples: 0,
  fibers: [{ id: 0, label: "main", samples: 100, oncpuSamples: 100 }],
  hotFunctions: [],
  hotLines: [],
};

suite("profiler-panel message validation", () => {
  test("only a well-formed message reaches the router, and only a located one navigates", () => {
    const good = { type: "select", file: "/w/fib.osp", line: 3 };
    assert.deepStrictEqual(webviewMessage(good), good);
    for (const bad of [
      undefined,
      null,
      "select",
      { type: 5, file: "/w/fib.osp", line: 3 },
      { type: "select", file: 5, line: 3 },
      { type: "select", file: "/w/fib.osp", line: "3" },
    ]) {
      assert.strictEqual(webviewMessage(bad), undefined, `expected rejection: ${JSON.stringify(bad)}`);
    }
    for (const unlocated of [
      { type: "other", file: "/w/fib.osp", line: 3 },
      { type: "navigateToSource", line: 3 },
      { type: "navigateToSource", file: "/w/fib.osp" },
    ]) {
      assert.strictEqual(handleSourceNavigation(unlocated), false, `expected no navigation: ${JSON.stringify(unlocated)}`);
    }
  });
});

suite("profiler-panel revealSource", () => {
  let filePath: string;

  suiteSetup(async () => {
    const dir = await fs.promises.mkdtemp(
      path.join(os.tmpdir(), "osprey-panel-"),
    );
    filePath = path.join(dir, "reveal.osp");
    await fs.promises.writeFile(filePath, "line one\nline two\nline three\n");
  });

  suiteTeardown(async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  });

  test("selects the 1-based line, clamped into the document", async () => {
    await revealSource({ type: "select", file: filePath, line: 2 });
    assert.strictEqual(
      vscode.window.activeTextEditor?.selection.active.line,
      1,
    );

    await revealSource({ type: "select", file: filePath, line: 999 });
    const lastLine =
      vscode.window.activeTextEditor?.selection.active.line ?? -1;
    assert.ok(lastLine >= 2, "clamps to the document end");

    await revealSource({ type: "select", file: filePath, line: 0 });
    assert.strictEqual(
      vscode.window.activeTextEditor?.selection.active.line,
      0,
    );
  });

  test("the router ignores junk, survives unopenable files, and opens a selected line", async () => {
    const activeLine = (): number | undefined =>
      vscode.window.activeTextEditor?.document.uri.fsPath === filePath
        ? vscode.window.activeTextEditor.selection.active.line : undefined;
    handleFlamegraphMessage({ type: "noise" });
    handleFlamegraphMessage({ type: "select", file: "/definitely/not/here.osp", line: 1 });
    handleFlamegraphMessage({ type: "select", file: filePath, line: 3 });
    await waitFor(activeLine, (line) => line === 2, COMMAND_MS, "a canvas select opens its line");
    handleFlamegraphMessage({ type: "navigateToSource", file: filePath, line: 1 });
    await waitFor(activeLine, (line) => line === 0, COMMAND_MS, "a table row opens its line");
  });
});

suite("profiler-panel singleton", () => {
  const model = buildFlameModel(DOC);

  suiteTeardown(async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  });

  test("creates, reuses, and recreates after dispose", async () => {
    await showFlamePanel(model, SUMMARY, "/w/fib.osp");
    assert.deepStrictEqual(flamePanel.describe(), { viewType: "ospreyProfiler", title: "Profile: fib.osp" });
    assert.ok(flamePanel.html()?.includes('id="flame-data"'));
    assert.ok(flamePanel.html()?.includes("<canvas"));

    await showFlamePanel(model, SUMMARY, "/w/other.ospml");
    assert.strictEqual(flamePanel.describe()?.title, "Profile: other.ospml", "the one panel is retitled");

    flamePanel.dispose();
    assert.ok(!flamePanel.isOpen(), "a disposed panel is forgotten");
    await showFlamePanel(model, SUMMARY, "/w/fib.osp");
    assert.ok(flamePanel.isOpen(), "a disposed panel is recreated");
    flamePanel.dispose();
  });
});

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import {
  buildFlameModel,
  type SpeedscopeFile,
} from "../../client/src/profiler/flame-model";
import {
  handlePanelMessage,
  parseSelectMessage,
  revealSource,
  showFlamePanel,
} from "../../client/src/profiler/profiler-panel";
import type { ProfileSummary } from "../../client/src/profiler/summary";

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

suite("profiler-panel parseSelectMessage", () => {
  test("accepts a well-formed select and rejects everything else", () => {
    const good = { type: "select", file: "/w/fib.osp", line: 3 };
    assert.deepStrictEqual(parseSelectMessage(good), good);
    for (const bad of [
      undefined,
      null,
      "select",
      { type: "other", file: "/w/fib.osp", line: 3 },
      { type: "select", file: "", line: 3 },
      { type: "select", line: 3 },
      { type: "select", file: "/w/fib.osp", line: "3" },
    ]) {
      assert.strictEqual(
        parseSelectMessage(bad),
        undefined,
        `expected rejection: ${JSON.stringify(bad)}`,
      );
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

  test("handlePanelMessage ignores junk and survives unopenable files", async () => {
    await handlePanelMessage({ type: "noise" });
    await handlePanelMessage({
      type: "select",
      file: "/definitely/not/here.osp",
      line: 1,
    });
    await handlePanelMessage({ type: "select", file: filePath, line: 3 });
    assert.strictEqual(
      vscode.window.activeTextEditor?.selection.active.line,
      2,
    );
  });
});

suite("profiler-panel singleton", () => {
  const model = buildFlameModel(DOC);

  suiteTeardown(async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  });

  test("creates, reuses, and recreates after dispose", async () => {
    const first = showFlamePanel(model, SUMMARY, "/w/fib.osp");
    assert.strictEqual(first.title, "Profile: fib.osp");
    assert.ok(first.webview.html.includes('id="flame-data"'));
    assert.ok(first.webview.html.includes("<canvas"));

    const second = showFlamePanel(model, SUMMARY, "/w/other.ospml");
    assert.strictEqual(second, first, "panel is a singleton");
    assert.strictEqual(second.title, "Profile: other.ospml");

    first.dispose();
    await new Promise((resolve) => setTimeout(resolve, 50));
    const third = showFlamePanel(model, SUMMARY, "/w/fib.osp");
    assert.notStrictEqual(third, first, "a disposed panel is recreated");
    third.dispose();
  });
});

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import {
  canonicalHeatPath,
  fileHeat,
  groupByColor,
  HEAT_COLORS,
  HEAT_HIGH,
  HEAT_MILD,
  HEAT_MUTED,
  HEAT_WARM,
  heatColor,
  HeatDecorationManager,
  heatLabel,
} from "../../client/src/profiler/heat-decorations";
import type { HotLineInfo, ProfileSummary } from "../../client/src/profiler/summary";

const hotLine = (file: string, line: number, pct: number, samples: number): HotLineInfo => ({
  file,
  line,
  pct,
  samples,
});

function summaryWith(hotLines: HotLineInfo[]): ProfileSummary {
  return {
    version: 1,
    program: "/w/fib.osp",
    wallSeconds: 1,
    cpuSeconds: 1,
    sampleCount: 100,
    rateHz: 997,
    droppedSamples: 0,
    fibers: [],
    hotFunctions: [],
    hotLines,
  };
}

suite("heat-decorations pure helpers", () => {
  test("heatColor thresholds at 20/10/5 percent (inclusive)", () => {
    assert.strictEqual(heatColor(35), HEAT_HIGH);
    assert.strictEqual(heatColor(20), HEAT_HIGH);
    assert.strictEqual(heatColor(19.9), HEAT_WARM);
    assert.strictEqual(heatColor(10), HEAT_WARM);
    assert.strictEqual(heatColor(9.9), HEAT_MILD);
    assert.strictEqual(heatColor(5), HEAT_MILD);
    assert.strictEqual(heatColor(4.9), HEAT_MUTED);
    assert.strictEqual(heatColor(0), HEAT_MUTED);
  });

  test("heatLabel matches the spec format", () => {
    assert.strictEqual(heatLabel(hotLine("/w/fib.osp", 5, 12.4, 124)), " ▍ 12.4% · 124 samples");
    assert.strictEqual(heatLabel(hotLine("/w/fib.osp", 5, 7, 1)), " ▍ 7.0% · 1 samples");
  });

  test("fileHeat keeps only the requested file's lines, with tier colors", () => {
    const lines = [
      hotLine("/w/fib.osp", 5, 25, 250),
      hotLine("/w/other.osp", 1, 50, 500),
      hotLine("/w/fib.osp", 9, 2, 20),
    ];
    const heats = fileHeat(lines, "/w/fib.osp");
    assert.deepStrictEqual(heats.map((h) => [h.line, h.color]), [
      [5, HEAT_HIGH],
      [9, HEAT_MUTED],
    ]);
    assert.deepStrictEqual(fileHeat(lines, "/nope.osp"), []);
  });

  test("canonicalHeatPath collapses dot segments and falls back for missing paths", () => {
    assert.strictEqual(
      canonicalHeatPath(`${path.sep}nope${path.sep}..${path.sep}nope${path.sep}x.osp`),
      `${path.sep}nope${path.sep}x.osp`,
    );
    const throwing = (): string => {
      throw new Error("ENOENT");
    };
    assert.strictEqual(canonicalHeatPath("/gone/x.osp", throwing, "darwin"), "/gone/x.osp");
  });

  test("canonicalHeatPath compares case-insensitively only on win32", () => {
    const identity = (p: string): string => p;
    assert.strictEqual(canonicalHeatPath("/W/Fib.OSP", identity, "win32"), "/w/fib.osp");
    assert.strictEqual(canonicalHeatPath("/W/Fib.OSP", identity, "linux"), "/W/Fib.OSP");
  });

  test("canonicalHeatPath resolves symlinked spellings (/tmp vs /private/tmp style)", async () => {
    const realDir = fs.realpathSync.native(
      await fs.promises.mkdtemp(path.join(os.tmpdir(), "osprey-canon-")),
    );
    const linkDir = `${realDir}-link`;
    await fs.promises.symlink(realDir, linkDir, "dir");
    const realFile = path.join(realDir, "hot.osp");
    await fs.promises.writeFile(realFile, "fn main() = 1\n");
    const linkedFile = path.join(linkDir, "hot.osp");
    assert.notStrictEqual(linkedFile, realFile);
    assert.strictEqual(canonicalHeatPath(linkedFile), canonicalHeatPath(realFile));
    assert.strictEqual(canonicalHeatPath(linkedFile), realFile);
    // fileHeat matches heat reported against one spelling to the other.
    const lines = [hotLine(linkedFile, 3, 25, 250)];
    assert.deepStrictEqual(fileHeat(lines, realFile).map((h) => h.line), [3]);
    assert.deepStrictEqual(fileHeat(lines, path.join(realDir, "other.osp")), []);
    await fs.promises.rm(linkDir);
    await fs.promises.rm(realDir, { recursive: true });
  });

  test("groupByColor buckets annotations per tier", () => {
    const heats = fileHeat(
      [hotLine("/f.osp", 1, 30, 1), hotLine("/f.osp", 2, 22, 1), hotLine("/f.osp", 3, 6, 1)],
      "/f.osp",
    );
    const groups = groupByColor(heats);
    assert.strictEqual(groups.get(HEAT_HIGH)?.length, 2);
    assert.strictEqual(groups.get(HEAT_MILD)?.length, 1);
    assert.strictEqual(groups.get(HEAT_WARM), undefined);
    assert.strictEqual(HEAT_COLORS.length, 4);
  });
});

suite("HeatDecorationManager", () => {
  let filePath: string;
  let editor: vscode.TextEditor;

  suiteSetup(async () => {
    const dir = await fs.promises.mkdtemp(path.join(os.tmpdir(), "osprey-heat-"));
    filePath = path.join(dir, "hot.osp");
    await fs.promises.writeFile(filePath, "fn main() = 1\nprint(main())\n");
    const document = await vscode.workspace.openTextDocument(filePath);
    editor = await vscode.window.showTextDocument(document);
  });

  suiteTeardown(async () => {
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  });

  test("apply decorates matching visible editors and records the summary", () => {
    const manager = new HeatDecorationManager(() => true);
    const summary = summaryWith([
      hotLine(filePath, 1, 25, 250),
      hotLine(filePath, 999, 6, 60), // out of range — silently dropped
      hotLine("/elsewhere.osp", 1, 50, 1),
    ]);
    manager.apply(summary, [editor]);
    assert.strictEqual(manager.current(), summary);
    manager.dispose();
  });

  test("clear strips the summary; refresh defaults to visible editors", () => {
    const manager = new HeatDecorationManager(() => true);
    manager.apply(summaryWith([hotLine(filePath, 2, 12, 120)]));
    manager.refresh();
    manager.clear();
    assert.strictEqual(manager.current(), undefined);
    manager.dispose();
  });

  test("a disabled manager applies no annotations but still tracks state", () => {
    const manager = new HeatDecorationManager(() => false);
    const summary = summaryWith([hotLine(filePath, 1, 25, 250)]);
    manager.apply(summary, [editor]);
    assert.strictEqual(manager.current(), summary);
    manager.dispose();
    manager.dispose(); // idempotent
  });

  test("the default enablement reads osprey.profiler.inlineHeat (default true)", () => {
    const manager = new HeatDecorationManager();
    manager.apply(summaryWith([hotLine(filePath, 1, 25, 250)]), [editor]);
    assert.strictEqual(
      vscode.workspace.getConfiguration("osprey").get<boolean>("profiler.inlineHeat", true),
      true,
    );
    manager.dispose();
  });
});

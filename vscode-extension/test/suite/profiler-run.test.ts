import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import { HeatDecorationManager } from "../../client/src/profiler/heat-decorations";
import {
  artifactPaths,
  loadArtifacts,
  profileActiveFile,
  PROFILER_COMMANDS,
  registerProfilerCommands,
  runWithProfilingProgress,
  stderrTail,
  type ProgressRunner,
} from "../../client/src/profiler/profile-run";
import {
  formatSummaryHeader,
  onCpuPct,
  parseSummary,
} from "../../client/src/profiler/summary";

suite("profile-run pure helpers", () => {
  test("artifactPaths derives every export from the source stem", () => {
    const paths = artifactPaths("/tmp/run", "/w/fib.osp");
    assert.deepStrictEqual(paths, {
      stem: "fib",
      profileJson: "/tmp/run/fib.profile.json",
      speedscope: "/tmp/run/fib.speedscope.json",
      cpuprofile: "/tmp/run/fib.cpuprofile",
      folded: "/tmp/run/fib.folded",
    });
    assert.strictEqual(artifactPaths("/t", "/w/app.ospml").stem, "app");
  });

  test("stderrTail keeps the last non-empty lines", () => {
    const lines = Array.from({ length: 20 }, (_, i) => `line ${i}`).join("\n");
    const tail = stderrTail(lines);
    assert.strictEqual(tail.split("\n").length, 12);
    assert.ok(tail.endsWith("line 19"));
    assert.strictEqual(stderrTail("a\n\n \nb\n", 5), "a\nb");
    assert.strictEqual(stderrTail(""), "");
  });
});

suite("profile-run summary parsing", () => {
  const SPEC_EXAMPLE = JSON.stringify({
    version: 1,
    program: "/abs/fib.osp",
    wallSeconds: 1.2,
    cpuSeconds: 0.9,
    sampleCount: 1234,
    rateHz: 997,
    droppedSamples: 0,
    fibers: [{ id: 0, label: "main", samples: 100, oncpuSamples: 90 }],
    hotFunctions: [
      { name: "fib", file: "/abs/fib.osp", line: 3, selfPct: 42.0, totalPct: 61.0, selfSamples: 420, totalSamples: 610, kind: "user" },
    ],
    hotLines: [{ file: "/abs/fib.osp", line: 5, pct: 12.0, samples: 120 }],
  });

  test("parses the spec example and formats its header", () => {
    const parsed = parseSummary(SPEC_EXAMPLE);
    assert.ok(parsed.ok);
    assert.strictEqual(parsed.value.hotFunctions[0].name, "fib");
    assert.strictEqual(parsed.value.hotLines[0].line, 5);
    assert.strictEqual(onCpuPct(parsed.value.fibers[0]), 90);
    assert.strictEqual(
      formatSummaryHeader(parsed.value),
      "1234 samples · 997Hz · 1.2s wall · 0.9s CPU · 1 fiber",
    );
  });

  test("rejects malformed documents with a field-level error", () => {
    assert.ok(!parseSummary("{oops").ok);
    assert.ok(!parseSummary("[1,2]").ok);
    const missing = parseSummary('{"version":1,"wallSeconds":1,"cpuSeconds":1,"sampleCount":9}');
    assert.ok(!missing.ok && missing.error.includes('"rateHz"'));
  });

  test("normalizes junk: filters nameless functions, fileless lines, defaults arrays", () => {
    const parsed = parseSummary(
      '{"version":1,"wallSeconds":1,"cpuSeconds":1,"sampleCount":9,"rateHz":997,' +
        '"fibers":"nope","hotFunctions":[{"name":""},{"name":"ok"},7],' +
        '"hotLines":[{"file":"","line":1},{"file":"/f.osp","line":0},{"file":"/f.osp","line":2,"pct":1,"samples":1}]}',
    );
    assert.ok(parsed.ok);
    assert.deepStrictEqual(parsed.value.fibers, []);
    assert.deepStrictEqual(parsed.value.hotFunctions.map((f) => f.name), ["ok"]);
    assert.deepStrictEqual(parsed.value.hotLines.map((l) => l.line), [2]);
    assert.strictEqual(parsed.value.droppedSamples, 0);
    assert.strictEqual(parsed.value.program, "");
  });

  test("header pluralizes fibers, reports drops, and widens long times", () => {
    const parsed = parseSummary(
      '{"version":1,"wallSeconds":120,"cpuSeconds":0.5,"sampleCount":10,"rateHz":99,"droppedSamples":3,' +
        '"fibers":[{"id":0,"label":"a","samples":1,"oncpuSamples":1},{"id":1,"label":"b","samples":0,"oncpuSamples":0}]}',
    );
    assert.ok(parsed.ok);
    assert.strictEqual(
      formatSummaryHeader(parsed.value),
      "10 samples · 99Hz · 120s wall · 0.5s CPU · 2 fibers · 3 dropped",
    );
    assert.strictEqual(onCpuPct(parsed.value.fibers[1]), 0);
  });
});

const OK_SPEEDSCOPE =
  '{"shared":{"frames":[{"name":"main","file":"SRC","line":1},{"name":"fib","file":"SRC","line":1}]},' +
  '"profiles":[{"type":"sampled","name":"main","unit":"seconds","startValue":0,"endValue":1,' +
  '"samples":[[0],[0,1],[0,1]],"weights":[0.4,0.3,0.3]}]}';

suite("profile-run loadArtifacts", () => {
  let dir: string;

  suiteSetup(async () => {
    dir = await fs.promises.mkdtemp(path.join(os.tmpdir(), "osprey-artifacts-"));
  });

  test("reports missing artifacts", () => {
    const loaded = loadArtifacts(artifactPaths(dir, "/w/fib.osp"));
    assert.ok(!loaded.ok && loaded.error.includes("profiler output missing"));
  });

  test("propagates summary and speedscope validation errors", async () => {
    const paths = artifactPaths(dir, "/w/fib.osp");
    await fs.promises.writeFile(paths.profileJson, "{broken");
    await fs.promises.writeFile(paths.speedscope, OK_SPEEDSCOPE);
    const badSummary = loadArtifacts(paths);
    assert.ok(!badSummary.ok && badSummary.error.includes("not valid JSON"));

    await fs.promises.writeFile(
      paths.profileJson,
      '{"version":1,"wallSeconds":1,"cpuSeconds":1,"sampleCount":3,"rateHz":997}',
    );
    await fs.promises.writeFile(paths.speedscope, '{"profiles":[]}');
    const badScope = loadArtifacts(paths);
    assert.ok(!badScope.ok && badScope.error.includes("shared.frames"));
  });

  test("loads a complete run into a model + summary", async () => {
    const paths = artifactPaths(dir, "/w/fib.osp");
    await fs.promises.writeFile(
      paths.profileJson,
      '{"version":1,"wallSeconds":1,"cpuSeconds":1,"sampleCount":3,"rateHz":997}',
    );
    await fs.promises.writeFile(paths.speedscope, OK_SPEEDSCOPE);
    const loaded = loadArtifacts(paths);
    assert.ok(loaded.ok);
    assert.strictEqual(loaded.value.model.fibers.length, 1);
    assert.strictEqual(loaded.value.summary.rateHz, 997);
  });
});

// A stub `osprey` CLI: writes both artifacts into its cwd (the per-run temp
// dir) exactly as `--run --profile` does, interpolating the source path.
const OK_STUB = `#!/bin/sh
src="$1"
base=$(basename "$src")
stem="\${base%.*}"
cat > "$stem.profile.json" <<EOF
{"version":1,"program":"$src","wallSeconds":1.2,"cpuSeconds":0.9,"sampleCount":1234,"rateHz":997,"droppedSamples":0,"fibers":[{"id":0,"label":"main","samples":100,"oncpuSamples":90}],"hotFunctions":[{"name":"fib","file":"$src","line":1,"selfPct":42.0,"totalPct":61.0,"selfSamples":420,"totalSamples":610,"kind":"user"}],"hotLines":[{"file":"$src","line":1,"pct":12.0,"samples":120},{"file":"$src","line":99,"pct":6.0,"samples":60}]}
EOF
cat > "$stem.speedscope.json" <<EOF
${OK_SPEEDSCOPE.split("SRC").join("$src")}
EOF
echo "profiled $src"
`;

const FAIL_STUB = `#!/bin/sh
echo "sampler exploded" >&2
exit 3
`;

// A profiled program that hangs: only cancellation can end this run.
const HANG_STUB = `#!/bin/sh
sleep 30
`;

suite("profile-run command flow", function () {
  this.timeout(20000);
  const context = { subscriptions: [] as vscode.Disposable[] };
  let sourcePath: string;
  let hangScript: string;
  let okHeat: ReturnType<typeof registerProfilerCommands>;

  suiteSetup(async () => {
    const dir = await fs.promises.mkdtemp(path.join(os.tmpdir(), "osprey-profrun-"));
    sourcePath = path.join(dir, "fib.osp");
    await fs.promises.writeFile(sourcePath, "fn fib(n) = n\nprint(fib(30))\n");
    const okScript = path.join(dir, "osprey-ok.sh");
    const failScript = path.join(dir, "osprey-fail.sh");
    hangScript = path.join(dir, "osprey-hang.sh");
    await fs.promises.writeFile(okScript, OK_STUB, { mode: 0o755 });
    await fs.promises.writeFile(failScript, FAIL_STUB, { mode: 0o755 });
    await fs.promises.writeFile(hangScript, HANG_STUB, { mode: 0o755 });
    okHeat = registerProfilerCommands(
      context as unknown as vscode.ExtensionContext,
      () => okScript,
      { profile: "ospreyProfilerTest.run", openLast: "ospreyProfilerTest.openLast" },
    );
    registerProfilerCommands(
      context as unknown as vscode.ExtensionContext,
      () => failScript,
      { profile: "ospreyProfilerTest.failRun", openLast: "ospreyProfilerTest.failOpen" },
    );
  });

  suiteTeardown(async () => {
    context.subscriptions.forEach((disposable) => disposable.dispose());
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
  });

  test("the real commands refuse politely without an osprey editor", async () => {
    await vscode.extensions.getExtension("nimblesite.osprey")?.activate();
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    assert.strictEqual(await vscode.commands.executeCommand(PROFILER_COMMANDS.profile), false);
    assert.strictEqual(await vscode.commands.executeCommand(PROFILER_COMMANDS.openLast), false);
  });

  test("a successful run opens the flame panel, heat marks, and openLast", async () => {
    const document = await vscode.workspace.openTextDocument(sourcePath);
    await vscode.window.showTextDocument(document);
    const opened = await vscode.commands.executeCommand("ospreyProfilerTest.run");
    assert.strictEqual(opened, true);
    assert.strictEqual(okHeat.current()?.sampleCount, 1234);
    assert.strictEqual(okHeat.current()?.hotLines[0].file, sourcePath);
    assert.strictEqual(await vscode.commands.executeCommand("ospreyProfilerTest.openLast"), true);
  });

  test("a failing run surfaces the stderr tail and opens nothing", async () => {
    const document = await vscode.workspace.openTextDocument(sourcePath);
    await vscode.window.showTextDocument(document);
    assert.strictEqual(await vscode.commands.executeCommand("ospreyProfilerTest.failRun"), false);
    assert.strictEqual(await vscode.commands.executeCommand("ospreyProfilerTest.failOpen"), false);
  });

  test("runWithProfilingProgress supplies a live token and passes the result through", async () => {
    let token: vscode.CancellationToken | undefined;
    const result = await runWithProfilingProgress((t) => {
      token = t;
      return Promise.resolve({ stdout: "ok", stderr: "", exitCode: 0 });
    });
    assert.deepStrictEqual(result, { stdout: "ok", stderr: "", exitCode: 0 });
    assert.strictEqual(token?.isCancellationRequested, false);
  });

  test("cancelling mid-run stops a hung profiled program promptly, no flame panel", async () => {
    const document = await vscode.workspace.openTextDocument(sourcePath);
    await vscode.window.showTextDocument(document);
    const lines: string[] = [];
    const output = {
      append: (text: string): void => void lines.push(text),
      appendLine: (line: string): void => void lines.push(line),
      show: (): void => undefined,
    } as unknown as vscode.OutputChannel;
    // Stand-in for the notification's Cancel button: cancels 100ms in.
    const cancelSoon: ProgressRunner = (exec) => {
      const source = new vscode.CancellationTokenSource();
      setTimeout(() => source.cancel(), 100);
      return exec(source.token);
    };
    const heat = new HeatDecorationManager(() => false);
    const started = Date.now();
    let remembered = false;
    const opened = await profileActiveFile(
      hangScript,
      output,
      heat,
      () => {
        remembered = true;
      },
      cancelSoon,
    );
    heat.dispose();
    assert.strictEqual(opened, false);
    assert.strictEqual(remembered, false);
    assert.ok(Date.now() - started < 10000, "cancel must kill the process tree promptly");
    assert.ok(lines.some((line) => line.includes("Profiling cancelled.")));
  });
});

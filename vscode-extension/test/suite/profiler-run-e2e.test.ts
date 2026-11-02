// [PROF-VSCODE-FLAME] "Osprey: Profile Current File" end to end: the real
// compiler profiles a real program, and Osprey's own UI — the command, its
// flame panel, heat marks, output and "Open Last Profile" — shows the result.
import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import { HeatDecorationManager } from "../../client/src/profiler/heat-decorations";
import { OSPREY_PALETTE } from "../../client/src/profiler/palette";
import { flamePanel } from "../../client/src/profiler/profiler-panel";
import { profileActiveFile, PROFILER_COMMANDS, type ProfileArtifacts, type ProgressRunner } from "../../client/src/profiler/profile-run";
import { resolveBuiltOsprey } from "./osprey-test-env";
import { BUILD_MS, EXTENSION_ID } from "./profiler-kit";

// The same workload scripts/test_profiler.sh profiles: a spawned worker fiber
// beside the main fiber, both burning CPU in `fib`.
const PROGRAM = `fn add(a, b) = match b {
    0 => a
    1 => satAdd(a, 1)
    _ => satAdd(add(a, intDiv(b, 2)), satSub(b, intDiv(b, 2)))
}
fn sub(a, b) = match b {
    0 => a
    _ => satSub(sub(a, satSub(b, 1)), 1)
}
fn fib(n) = match n {
    0 => 0
    1 => 1
    _ => add(fib(sub(n, 1)), fib(sub(n, 2)))
}
let worker = spawn fib(33)
let local = fib(35)
print("\${add(local, await(worker))}")
`;
const PROGRAM_OUTPUT = "12752043";

/** Runs the profiler with no notification, so a test never waits on UI chrome. */
const direct: ProgressRunner = (exec) => exec(new vscode.CancellationTokenSource().token);

interface Recorded { readonly output: vscode.OutputChannel; readonly text: () => string }

function recordingOutput(): Recorded {
  const chunks: string[] = [];
  const output = {
    append: (text: string): void => void chunks.push(text),
    appendLine: (line: string): void => void chunks.push(`${line}\n`),
    show: (): void => undefined,
  } as unknown as vscode.OutputChannel;
  return { output, text: () => chunks.join("") };
}

async function openEditor(file: string): Promise<void> {
  await vscode.window.showTextDocument(await vscode.workspace.openTextDocument(file));
}

suite("Profile Current File — real compiler, Osprey UI", function () {
  this.timeout(BUILD_MS);
  let dir = "";
  let source = "";
  let savedHz: string | undefined;

  suiteSetup(async () => {
    await vscode.extensions.getExtension(EXTENSION_ID)?.activate();
    dir = await fs.promises.realpath(await fs.promises.mkdtemp(path.join(os.tmpdir(), "osprey-profile-e2e-")));
    source = path.join(dir, "profdemo.osp");
    await fs.promises.writeFile(source, PROGRAM);
    // A high rate keeps short machines well clear of an empty profile.
    savedHz = process.env.OSPREY_PROFILE_HZ;
    process.env.OSPREY_PROFILE_HZ = "8000";
  });

  suiteTeardown(async () => {
    if (savedHz === undefined) delete process.env.OSPREY_PROFILE_HZ;
    else process.env.OSPREY_PROFILE_HZ = savedHz;
    flamePanel.dispose();
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await fs.promises.rm(dir, { recursive: true, force: true });
  });

  test("the command profiles the open file into one flame panel that Open Last restores", async () => {
    flamePanel.dispose();
    await openEditor(source);
    assert.strictEqual(await vscode.commands.executeCommand(PROFILER_COMMANDS.profile), true, "the run opens a panel");
    assert.deepStrictEqual(flamePanel.describe(), { viewType: "ospreyProfiler", title: "Profile: profdemo.osp" });
    const html = flamePanel.html() ?? "";
    assert.ok(html.startsWith("<!DOCTYPE html>"), "the panel renders a document");
    assert.match(html, /\d+ samples · \d+Hz · [\d.]+s wall · [\d.]+s CPU · \d+ fibers?/, "the header summarises the run");
    assert.ok(html.includes(`--prof-critical: ${OSPREY_PALETTE.critical}`), "Osprey's palette themes the panel");
    assert.ok(html.includes('<canvas id="flame"></canvas>'), "the flame graph canvas is present");
    assert.ok(html.includes('data-fiber="0">'), "the main fiber has a chip");
    assert.ok(html.includes('data-fiber="1">'), "the spawned worker fiber has its own chip");
    assert.ok(html.includes(`"name":"fib","file":${JSON.stringify(source)}`), "fib is a hot function located in the profiled file");
    assert.ok(html.includes('"name":"fib"') && html.includes('"leftHeavy"'), "the model embeds the program's frames");

    flamePanel.dispose();
    assert.ok(!flamePanel.isOpen(), "the panel can be closed");
    assert.strictEqual(await vscode.commands.executeCommand(PROFILER_COMMANDS.openLast), true, "Open Last reopens it");
    assert.deepStrictEqual(flamePanel.describe(), { viewType: "ospreyProfiler", title: "Profile: profdemo.osp" });
    assert.ok(flamePanel.html()?.includes(`"name":"fib","file":${JSON.stringify(source)}`), "the reopened panel shows the same run");
  });

  test("a run streams the program's output, keeps every export, and marks the hot lines", async () => {
    const compiler = resolveBuiltOsprey();
    assert.ok(compiler, "build the Osprey compiler before the profiler suites");
    await openEditor(source);
    const recorded = recordingOutput();
    const heat = new HeatDecorationManager();
    let kept: { artifacts: ProfileArtifacts; summary: { sampleCount: number } } | undefined;
    try {
      assert.strictEqual(await profileActiveFile(compiler, recorded.output, heat, (run) => { kept = run; }, direct), true);
      const text = recorded.text();
      assert.ok(text.includes(`Profiling ${source} → `), "the log names the profiled file");
      assert.ok(text.includes(PROGRAM_OUTPUT), "the program's own output reaches the log");
      assert.ok(text.includes("Profiler process exited with code 0"), "the log records a clean exit");
      assert.match(text, /\d+ samples · \d+Hz/, "the log carries the summary header");
      assert.ok(text.includes("Profile artifacts kept in "), "the log says where the exports are");
      assert.ok(kept, "the run is remembered for Open Last");
      for (const key of ["profileJson", "speedscope", "cpuprofile", "folded"] as const) {
        const file = kept.artifacts[key];
        assert.ok(fs.statSync(file).size > 0, `${key} export is written: ${file}`);
      }
      const summary = heat.current();
      assert.ok(summary && summary.sampleCount > 0, "the heat manager holds the run's samples");
      assert.strictEqual(summary.sampleCount, kept.summary.sampleCount, "heat and Open Last share one run");
      assert.ok(summary.hotLines.length > 0, "the program has hot lines");
      assert.ok(summary.hotLines.every((line) => line.file === source), "every hot line belongs to the profiled file");
      assert.ok(summary.hotFunctions.some((fn) => fn.name === "fib"), "fib is a hot function");
    } finally {
      heat.dispose();
    }
  });

  test("a run that writes no profile reports why, even with a silent stderr and a ragged stdout", async () => {
    const stub = path.join(dir, "osprey-silent.sh");
    await fs.promises.writeFile(stub, "#!/bin/sh\nprintf 'half a line'\nexit 1\n", { mode: 0o755 });
    await openEditor(source);
    const recorded = recordingOutput();
    const heat = new HeatDecorationManager();
    let remembered = false;
    try {
      assert.strictEqual(await profileActiveFile(stub, recorded.output, heat, () => { remembered = true; }, direct), false);
      const text = recorded.text();
      assert.ok(text.includes("half a line\n"), "a stdout chunk without a newline is terminated in the log");
      assert.ok(text.includes("Profiler process exited with code 1"), "the exit code is logged");
      assert.ok(text.includes("profiler output missing: expected "), "the missing exports are named");
      assert.ok(text.includes("profdemo.profile.json") && text.includes("profdemo.speedscope.json"), "both expected exports are listed");
      assert.strictEqual(remembered, false, "a failed run is not remembered");
      assert.strictEqual(heat.current(), undefined, "a failed run marks no lines");
    } finally {
      heat.dispose();
    }
  });
});

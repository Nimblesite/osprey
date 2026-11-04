// Tests for [PROF-VSCODE-SHARED]: CPU profiling of a program launched through
// the Osprey debugger, plus the results panel's builders, ported from
// SharpLsp's debug-launched CPU suite. "Run & Profile CPU (Current File)"
// builds the open file, runs it under lldb-dap, and the shared UI attaches a
// live session to the PID the adapter reports [PROF-ATTACH-ENDPOINT].
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import * as vscode from "vscode";
import { State } from "vscode-languageclient/node";
import { delay } from "@nimblesite/lsp-profiler-ui/out/src/timeouts";
import { appliedProfileDecorations, clearProfileDecorations, type ProfileResult } from "@nimblesite/lsp-profiler-ui/out/src/profiler-decorations";
import { buildFlamegraphHtml, disposeFlamegraphPanel, flamegraphPanelOpen } from "@nimblesite/lsp-profiler-ui/out/src/profiler-flamegraph-html";
import { assertHardened, assertHtml, pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { memoryStatusText, processesViewState, profilerStatusText, recordedOperations } from "@nimblesite/lsp-profiler-ui";
import type { ProfileReport } from "@nimblesite/lsp-profiler-ui/out/src/wire-generated";
import { OSPREY_PALETTE } from "../../client/src/profiler/palette";
import { resolveRequiredLldbDap } from "./osprey-test-env";
import {
  activateWithServer,
  ACTIVATION_MS,
  BUILD_MS,
  execute,
  FIXTURE_SOURCE,
  HOT_FUNCTION,
  HOT_LINES,
  lspClient,
  openSource,
  profilerStore,
  uiCommand,
  waitFor,
  waitForListing,
} from "./profiler-kit";

const SAMPLE_WINDOW_MS = 3_000;
const STEP_MS = 90_000;

/** The debuggee: the live fixture copied out of the repo, so its debug build lands in a scratch dir. */
let program = "";

/** The adapter's PID for the running debug session. */
async function debuggeePid(): Promise<number> {
  return waitFor(() => profilerStore().getDebuggeeProcessId(vscode.debug.activeDebugSession?.id ?? "") ?? 0,
    (pid) => pid > 0, STEP_MS, "lldb-dap reports the debuggee PID");
}

/** Launch the open file through the real "Run & Profile CPU" command; resolves once profiling. */
async function runAndProfileCurrentFile(): Promise<void> {
  await openSource(program);
  await vscode.commands.executeCommand(uiCommand("profileCurrentFileCpu"));
  await pollUntilResult({ fn: () => profilerStore().profiler.value.cpu, until: (state) => state === "active",
    timeoutMs: STEP_MS, description: "Run & Profile CPU to start profiling the launched program" });
}

async function endSession(): Promise<void> {
  if (profilerStore().profiler.value.cpu !== "idle") {
    await vscode.commands.executeCommand(uiCommand("profileStop"));
  }
  if (vscode.debug.activeDebugSession !== undefined) {
    await vscode.debug.stopDebugging();
    await waitFor(() => vscode.debug.activeDebugSession, (session) => session === undefined, STEP_MS, "the debug session ends");
  }
  clearProfileDecorations();
  disposeFlamegraphPanel();
}

function hasProgramHotFunction(report: ProfileReport): boolean {
  return report.hot_functions.some((fn) => fn.name === HOT_FUNCTION && fn.file === program);
}

suite("CPU profiling — debug-launched end-to-end", () => {
  let scratch = "";
  let priorLldbDap: string | undefined;
  suiteSetup(async function () {
    this.timeout(ACTIVATION_MS + BUILD_MS);
    await activateWithServer();
    const config = vscode.workspace.getConfiguration("osprey");
    priorLldbDap = config.inspect<string>("debug.lldbDapPath")?.globalValue;
    await config.update("debug.lldbDapPath", resolveRequiredLldbDap(), vscode.ConfigurationTarget.Global);
    scratch = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "osprey-cpu-launch-")));
    program = path.join(scratch, "launch_spin.osp");
    fs.copyFileSync(FIXTURE_SOURCE, program);
  });
  teardown(async function () {
    this.timeout(STEP_MS);
    await endSession();
  });
  suiteTeardown(async () => {
    await vscode.workspace.getConfiguration("osprey").update("debug.lldbDapPath", priorLldbDap, vscode.ConfigurationTarget.Global);
    fs.rmSync(scratch, { recursive: true, force: true });
  });

  test("a debug-launched program is profiled over the wire and its own code is attributed", async function () {
    this.timeout(STEP_MS * 2);
    await openSource(program);
    assert.ok(await vscode.debug.startDebugging(undefined, { type: "osprey", request: "launch", name: "Debug-launched CPU E2E", program }), "the debug session starts");
    const pid = await debuggeePid();
    // The adapter reports the PID before `main` opens the endpoint.
    await waitForListing(pid);
    const started = await execute<{ session_id: string }>("cpu.start", { pid, sample_rate: 100 });
    assert.ok(started.session_id.length > 0, "the attach must mint a session");
    await delay(SAMPLE_WINDOW_MS);
    const report = await execute<ProfileReport>("cpu.stop", { session_id: started.session_id });
    assert.ok(report.total_samples > 0, "the sampler collects real ticks from the debuggee");
    assert.ok(hasProgramHotFunction(report), `the debuggee's own spin is attributed: ${JSON.stringify(report.hot_functions.slice(0, 5))}`);
    assert.ok(report.hot_lines.some((line) => line.file === program && line.line >= HOT_LINES.first && line.line <= HOT_LINES.last), "hot lines land inside spin");
    for (const artifact of [report.artifacts.cpuprofile_path, report.artifacts.flamegraph_path]) {
      assert.ok(artifact !== undefined && fs.existsSync(artifact), `artifact written: ${String(artifact)}`);
    }
  });

  test("one-click 'Run & Profile CPU' launches, profiles live, and paints the heat map on stop", async function () {
    this.timeout(STEP_MS * 2);
    const opsBefore = recordedOperations().length;
    await runAndProfileCurrentFile();
    assert.ok(profilerStatusText() !== undefined, "the status bar shows the live profile");
    assert.equal(memoryStatusText(), undefined, "an Osprey debug session never offers memory tracking it has no backend for");
    assert.ok(processesViewState().message?.includes("Profiling PID") === true, `the panel shows the live profile: ${String(processesViewState().message)}`);
    await delay(SAMPLE_WINDOW_MS);
    await vscode.commands.executeCommand(uiCommand("profileStop"));
    assert.equal(profilerStore().profiler.value.cpu, "idle", "stop clears the reactive store state");
    assert.equal(processesViewState().message, undefined, "stop clears the panel chrome");
    assert.ok(appliedProfileDecorations().some((entry) => entry.file === program), "stop paints the heat map on the debuggee source");
    const ops = recordedOperations().slice(opsBefore);
    for (const title of ["Osprey: Starting CPU profiler", "Osprey: Stopping profiler"]) {
      const begin = ops.indexOf(`begin:${title}`);
      assert.ok(begin !== -1, `"${title}" shows progress, ops: ${ops.join(" | ")}`);
      assert.ok(ops.indexOf(`end:${title}`) > begin, `"${title}" progress closes on completion`);
    }
    assert.equal(profilerStatusText(), undefined, "the status bar clears after stop");
  });

  test("run → profile → view: the results panel opens on stop with no click, and stays reachable after closing", async function () {
    this.timeout(STEP_MS * 2);
    await runAndProfileCurrentFile();
    const toasts: { message: string; actions: string[] }[] = [];
    const win = vscode.window as { showInformationMessage: typeof vscode.window.showInformationMessage };
    const originalShow = win.showInformationMessage;
    win.showInformationMessage = async (message: string, ...items: unknown[]) => {
      toasts.push({ message, actions: items.filter((item): item is string => typeof item === "string") });
      return undefined;
    };
    disposeFlamegraphPanel();
    try {
      await delay(SAMPLE_WINDOW_MS);
      await vscode.commands.executeCommand(uiCommand("profileStop"));
    } finally {
      win.showInformationMessage = originalShow;
    }
    await pollUntilResult({ fn: () => flamegraphPanelOpen(), until: (open) => open, timeoutMs: 5_000, description: "the results panel opens on stop without any click" });
    const completion = toasts.find((toast) => /Profile complete/i.test(toast.message));
    assert.ok(completion, `a "Profile complete" notification is shown: ${JSON.stringify(toasts)}`);
    assert.ok(completion.actions.length > 0, 'the notification offers a way to reach the raw trace');
    disposeFlamegraphPanel();
    assert.equal(flamegraphPanelOpen(), false, "baseline: panel closed before re-entry");
    await vscode.commands.executeCommand(uiCommand("profileShowResults"));
    await pollUntilResult({ fn: () => flamegraphPanelOpen(), until: (open) => open, timeoutMs: 5_000, description: "Show Profile Results re-opens the last profile" });
  });

  test("live progress survives a language client restart — the profile never goes silently dead", async function () {
    this.timeout(ACTIVATION_MS + STEP_MS * 2);
    const client = lspClient();
    let stopped = false;
    const watch = client.onDidChangeState((change) => {
      if (change.newState === State.Stopped) stopped = true;
    });
    try {
      await client.restart();
      await pollUntilResult({ fn: () => ({ restarted: stopped, running: profilerStore().client.value?.isRunning() === true }),
        until: (seen) => seen.restarted && seen.running, timeoutMs: ACTIVATION_MS, description: "the language client restarts and runs again" });
    } finally {
      watch.dispose();
    }
    await runAndProfileCurrentFile();
    await pollUntilResult({ fn: () => profilerStatusText() ?? "", until: (text) => /[1-9][\d.]* ?K? samples/.test(text),
      timeoutMs: 30_000, description: "the live sample count reaches the status bar after the restart" });
    await vscode.commands.executeCommand(uiCommand("profileStop"));
    assert.equal(profilerStore().profiler.value.cpu, "idle", "stop clears the session");
  });

  test("flamegraph webview HTML renders the dashboard from a profile result in Osprey's palette", async () => {
    const result: ProfileResult = {
      sessionId: "s-test", duration: 3, totalSamples: 600, outputFile: "/tmp/profile.speedscope.json",
      hotFunctions: [{ name: HOT_FUNCTION, file: program, line: HOT_LINES.first, samples: 540, percentage: 90, selfPercentage: 85 }],
      hotLines: [{ file: program, line: HOT_LINES.first + 2, samples: 500, percentage: 83 }],
    };
    const html = await buildFlamegraphHtml(result);
    assert.ok(html.includes(HOT_FUNCTION), "the hot functions are embedded");
    assert.ok(html.toLowerCase().includes(OSPREY_PALETTE.critical), "Osprey's heat palette is used");
    assert.ok(html.includes("navigateToSource"), "rows navigate to source");
    assert.ok(html.includes("fn-body"), "the hot-functions table renders");
    assert.ok(html.includes(String(result.totalSamples)), "the summary shows the real sample count");
  });

  test("the flamegraph's Speedscope link uses a working import path, not a dead file:// URL", async () => {
    const html = await buildFlamegraphHtml({ sessionId: "s-test", duration: 3, totalSamples: 600, outputFile: "/tmp/profile.speedscope.json", hotFunctions: [], hotLines: [] });
    assertHtml(html, { openSpeedscope: "the Speedscope link posts an openSpeedscope message" });
    assertHtml(html, { "speedscope.app/#profileURL=file://": "never the always-failing speedscope file:// URL" }, false);
  });

  test("flamegraph HTML escapes untrusted frame names/paths and nonce-gates its inline script", async () => {
    const hostile = "</script><img src=x onerror=alert(1)>";
    const html = await buildFlamegraphHtml({
      sessionId: "s-test", duration: 3, totalSamples: 600, outputFile: "/tmp/profile.speedscope.json",
      hotFunctions: [
        { name: hostile, file: hostile, line: 1, samples: 540, percentage: 90, selfPercentage: 85 },
        { name: "main", file: program, line: 1, samples: 60, percentage: 10, selfPercentage: 10 },
      ],
      hotLines: [{ file: hostile, line: 2, samples: 500, percentage: 83 }],
    });
    assertHardened(html, "the results panel");
    assertHtml(html, {
      "escapeHtml(fn.name)": "frame names are escaped before innerHTML",
      "escapeHtml(basename(fn.file))": "frame file paths are escaped",
      '<script nonce="': "the inline script carries the CSP nonce",
    });
  });
});

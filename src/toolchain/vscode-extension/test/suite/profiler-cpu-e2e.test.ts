// Tests for [PROF-VSCODE-SHARED], [PROF-LSP-RUNTIME] and [PROF-LSP-HOST]. Live
// CPU profiling, real end-to-end, ported from SharpLsp's CPU and LSP-level CPU
// suites: `osprey lsp` attaches to a running Osprey program through its
// endpoint, the shared capture contract runs on the live report, one session's
// whole life travels the wire, the library's one-click panel flow drives it,
// and the results land as heat-map decorations, a results panel and artifacts
// on disk. The offline `--profile` capture keeps its own contract run in
// profiler-shared.test.ts.
import { registerCpuCaptureContract } from "@nimblesite/lsp-profiler-ui/out/src/testing/cpu-capture-contract";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { toProfileResult } from "@nimblesite/lsp-profiler-ui/out/src/wire-adapters";
import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as vscode from "vscode";
import {
  appliedProfileDecorations,
  flamegraphPanelOpen,
  profilerStatusText,
  recordedOperations,
} from "@nimblesite/lsp-profiler-ui";
import type { ProfileReport, ProfileSessionList, StartProfileResult } from "@nimblesite/lsp-profiler-ui/out/src/wire-generated";
import {
  BUILD_MS,
  execute,
  executeError,
  FIXTURE_SOURCE,
  HOT_FUNCTION,
  HOT_LINES,
  openSource,
  PROCESS_START_MS,
  profileFromRowFor,
  profilingSuite,
  sampleCpu,
  Target,
  uiCommand,
} from "./profiler-kit";

const SAMPLE_WINDOW_MS = 3_000;
const STOP_MS = 60_000;

const inSpin = (file: string, line: number): boolean =>
  file === FIXTURE_SOURCE && line >= HOT_LINES.first && line <= HOT_LINES.last;

async function listed(sessionId: string): Promise<ProfileSessionList["sessions"][number] | undefined> {
  return (await execute<ProfileSessionList>("cpu.list", {})).sessions.find((s) => s.session_id === sessionId);
}

suite("CPU profiling — live end-to-end", () => {
  const live = profilingSuite("cpu_e2e");
  registerCpuCaptureContract(async () => toProfileResult((await sampleCpu(live.target.pid, SAMPLE_WINDOW_MS)).report));

  test("raw pipeline: spin is hot in its own source lines, and the speedscope, .cpuprofile and flame graph exports parse", async function () {
    this.timeout(STOP_MS + SAMPLE_WINDOW_MS);
    const { started, report } = await sampleCpu(live.target.pid, SAMPLE_WINDOW_MS);
    assert.equal(started.runtime, "osprey");
    assert.ok(report.total_samples > 100, `samples: ${report.total_samples}`);
    const spin = report.hot_functions.find((fn) => fn.name === HOT_FUNCTION);
    assert.ok(spin, `spin must be hot: ${report.hot_functions.map((f) => f.name).join(", ")}`);
    assert.equal(spin.file, FIXTURE_SOURCE, "the hot function is located in the fixture source");
    assert.ok(report.hot_lines.some((line) => inSpin(line.file, line.line)), "hot lines resolve inside spin");
    for (const key of ["speedscope_path", "cpuprofile_path", "flamegraph_path"] as const) {
      const file = report.artifacts[key];
      assert.ok(file !== undefined && fs.existsSync(file), `${key} must be written: ${file}`);
    }
    const speedscope = JSON.parse(fs.readFileSync(report.artifacts.speedscope_path ?? "", "utf8")) as { profiles: unknown[] };
    assert.ok(speedscope.profiles.length > 0, "speedscope file parses with profiles");
    const cpuprofile = JSON.parse(fs.readFileSync(report.artifacts.cpuprofile_path ?? "", "utf8")) as { nodes: unknown[] };
    assert.ok(cpuprofile.nodes.length > 0, ".cpuprofile parses with nodes");
  });

  test("one session's life on the wire: identity on start, live counts, a snapshot that keeps the session, a final report, and refusals after stop", async function () {
    this.timeout(STOP_MS * 2);
    const started = await execute<StartProfileResult>("cpu.start", { pid: live.target.pid, include_native: false });
    assert.ok(started.session_id.length > 0, "a session id");
    assert.equal(started.pid, live.target.pid, "the session names its PID");
    assert.ok(started.runtime_version.length > 0, "a runtime version from the endpoint");
    assert.match(started.started_at, /^\d{4}-\d{2}-\d{2}T/, "an RFC 3339 start time");
    await new Promise((resolve) => setTimeout(resolve, 2_500));
    const now = await listed(started.session_id);
    assert.ok(now && now.sample_count > 0 && now.duration_secs > 0, `the live session counts: ${JSON.stringify(now)}`);
    const snapshot = await execute<ProfileReport>("cpu.snapshot", { session_id: started.session_id });
    assert.equal(snapshot.session_id, started.session_id, "the report names its session");
    assert.ok(snapshot.user_samples >= 0 && snapshot.user_samples <= snapshot.total_samples, "user samples are a subset");
    assert.ok(snapshot.sample_rate >= 1, `a positive sample rate: ${snapshot.sample_rate}`);
    assert.ok(await listed(started.session_id), "the snapshot did not end the session");
    const stopped = await execute<ProfileReport>("cpu.stop", { session_id: started.session_id });
    assert.ok(stopped.total_samples >= snapshot.total_samples, "samples never shrink on stop");
    assert.equal(stopped.artifacts.export_error, undefined, "no export error");
    assert.ok(stopped.hot_lines.some((line) => inSpin(line.file, line.line)), "the report samples inside spin");
    assert.equal(await listed(started.session_id), undefined, "a stopped session is no longer active");
    assert.ok(((await executeError("cpu.stop", { session_id: started.session_id }))?.code ?? 0) !== 0, "a second stop fails");
    assert.ok(((await executeError("cpu.snapshot", { session_id: started.session_id }))?.code ?? 0) !== 0, "a snapshot after stop fails");
    assert.ok(live.target.alive(), "attaching is not invasive: the program keeps running");
  });

  test("a duration on start stops the session by itself and keeps its report for the next stop", async function () {
    this.timeout(STOP_MS * 2);
    const started = await execute<{ session_id: string }>("cpu.start", { pid: live.target.pid, duration_secs: 2 });
    await pollUntilResult({ fn: async () => listed(started.session_id), until: (entry) => entry === undefined,
      timeoutMs: STOP_MS, description: "the timed session ends by itself" });
    const kept = await execute<ProfileReport>("cpu.stop", { session_id: started.session_id });
    assert.equal(kept.session_id, started.session_id, "the kept report belongs to the session");
    assert.ok(kept.total_samples > 0, "the timed session sampled the program");
    assert.equal(kept.duration_secs, 2, "the report records exactly the requested two seconds");
    assert.ok(kept.hot_lines.some((line) => inSpin(line.file, line.line)), "the timed report contains spin, not just runtime frames");
  });

  test("panel one-click flow: Profile CPU on a row → live status bar → Stop paints the heat map and opens the results panel", async function () {
    this.timeout(STOP_MS + SAMPLE_WINDOW_MS);
    await openSource(FIXTURE_SOURCE);
    await profileFromRowFor(live.target.pid, SAMPLE_WINDOW_MS, STOP_MS);
    assert.equal(profilerStatusText(), undefined, "the status bar hides after stop");
    const applied = appliedProfileDecorations();
    assert.ok(applied.some((entry) => entry.file === FIXTURE_SOURCE), `heat map painted on the fixture source: ${JSON.stringify(applied.map((e) => e.file))}`);
    assert.ok(flamegraphPanelOpen(), "the results panel opens on stop with no click");
    const operations = recordedOperations();
    assert.ok(operations.some((op) => op.startsWith("begin:Osprey: Starting CPU profiler")), `the start narrates progress: ${operations.join(" | ")}`);
    assert.ok(operations.some((op) => op.startsWith("end:Osprey: Stopping profiler")), "the stop narrates progress and closes it");
  });

  test("Show Profile Results re-opens the last results after the panel is closed", async function () {
    this.timeout(PROCESS_START_MS);
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    assert.equal(flamegraphPanelOpen(), false, "the panel closed");
    await vscode.commands.executeCommand(uiCommand("profileShowResults"));
    assert.ok(flamegraphPanelOpen(), "results stay reachable after closing the panel");
  });

  test("attaching to an exited program fails with a distinct, classified cause", async function () {
    this.timeout(BUILD_MS + PROCESS_START_MS);
    const victim = await Target.start("cpu_exited");
    victim.stop();
    await pollUntilResult({ fn: () => victim.alive(), until: (alive) => !alive, timeoutMs: PROCESS_START_MS, description: "the program exits" });
    const error = await executeError("cpu.start", { pid: victim.pid });
    assert.equal(error?.code, -32001, `PROCESS_NOT_FOUND: ${error?.message}`);
  });

  test("a live process with no endpoint is UNSUPPORTED_RUNTIME, not an Osprey program", async function () {
    this.timeout(PROCESS_START_MS);
    const error = await executeError("cpu.start", { pid: process.pid });
    assert.equal(error?.code, -32002, `UNSUPPORTED_RUNTIME: ${error?.message}`);
  });
});

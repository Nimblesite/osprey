// Tests for [PROF-VSCODE-SHARED] and [PROF-LSP-RUNTIME]. The Processes panel
// over the real `osprey lsp` and live Osprey programs, ported from SharpLsp's
// processes suite: the server lists every Osprey program with its runtime
// facts, the panel renders a row per program with the runtime's advertised
// name, a stable id and a tooltip carrying every resolved detail; Osprey has no
// memory backend, so no row offers or suggests memory tracking; the CPU launch
// stays pinned; filter, sort and group re-project the same list; copy, reveal
// and kill act on a row; and the welcome's context key follows the fetch.
import * as assert from "node:assert/strict";
import * as path from "node:path";
import * as vscode from "vscode";
import { contextKeyValues, processesViewState } from "@nimblesite/lsp-profiler-ui";
import type { ProcessList } from "@nimblesite/lsp-profiler-ui/out/src/wire-generated";
import { pollUntilResult } from "@nimblesite/lsp-profiler-ui/out/src/testing/webview-contract-helpers";
import { extensionRoot } from "./osprey-test-env";
import {
  activateWithServer,
  ACTIVATION_MS,
  BUILD_MS,
  COMMAND_MS,
  execute,
  executeError,
  FIXTURE_DIR,
  LSP_RESPONSE_MS,
  panelRows,
  PROCESS_START_MS,
  processesProvider,
  profilerStore,
  profilingSuite,
  rowOf,
  Target,
  tooltipText,
  uiCommand,
  waitFor,
} from "./profiler-kit";

async function refreshed(): Promise<void> {
  await vscode.commands.executeCommand(uiCommand("refreshProcesses"));
}

async function listedRow(pid: number): Promise<vscode.TreeItem> {
  const row = await waitFor(() => rowOf(pid), (item) => item !== undefined, LSP_RESPONSE_MS, `row for PID ${pid}`);
  assert.ok(row);
  return row;
}

suite("Processes panel — the server lists live Osprey programs", () => {
  const live = profilingSuite("procs_target");

  test("the wire listing carries the target with its runtime facts", async function () {
    this.timeout(LSP_RESPONSE_MS);
    const { target } = live;
    const listing = await execute<ProcessList>("processes", { workspace_roots: [] });
    const entry = listing.processes.find((proc) => proc.pid === target.pid);
    assert.ok(entry, `PID ${target.pid} listed: ${listing.processes.map((p) => p.pid).join(", ")}`);
    assert.equal(entry.runtime, "osprey");
    assert.ok(entry.name.includes("procs_target"), `name: ${entry.name}`);
    assert.equal(entry.attachable, true, "a live Osprey program answers hello on its endpoint");
    assert.ok(entry.runtime_version, "the runtime version comes from the endpoint's hello");
    assert.equal(entry.entry, "procs_target", "the entry is the program's own name");
    assert.equal(entry.executable, target.binary, "the executable is the compiled program");
    assert.equal(entry.ppid, process.pid, "spawned by the extension host");
  });

  test("the extension host itself is not an Osprey program and is never listed", async function () {
    this.timeout(LSP_RESPONSE_MS);
    const listing = await execute<ProcessList>("processes", { workspace_roots: [] });
    assert.ok(!listing.processes.some((proc) => proc.pid === process.pid), "only programs with an endpoint are listed");
  });

  test("a program under a workspace root is marked in_workspace; outside every root it is not", async function () {
    this.timeout(LSP_RESPONSE_MS);
    const { target } = live;
    const inside = await execute<ProcessList>("processes", { workspace_roots: [FIXTURE_DIR] });
    const outside = await execute<ProcessList>("processes", { workspace_roots: [path.join(extensionRoot, "syntaxes")] });
    assert.equal(inside.processes.find((proc) => proc.pid === target.pid)?.in_workspace, true);
    assert.equal(outside.processes.find((proc) => proc.pid === target.pid)?.in_workspace, false);
  });

  test("a listing request without workspace roots is INVALID_PARAMS", async function () {
    this.timeout(LSP_RESPONSE_MS);
    assert.equal((await executeError("processes", {}))?.code, -32602);
  });

  test("refresh loads the store and flips the welcome key to loaded", async function () {
    this.timeout(LSP_RESPONSE_MS);
    const { target } = live;
    await refreshed();
    const state = profilerStore().processes.value;
    assert.equal(state.fetch, "loaded");
    assert.ok(state.list.some((proc) => proc.pid === target.pid), "the target is in the store");
    assert.equal(contextKeyValues().get("osprey.profiler.processesState"), "loaded");
  });

  test("the row has a stable id, the runtime's advertised name, and every resolved detail", async function () {
    this.timeout(LSP_RESPONSE_MS);
    const { target } = live;
    await refreshed();
    const row = await listedRow(target.pid);
    assert.equal(row.id, `process:${target.pid}`);
    const tip = tooltipText(row);
    for (const needle of [`PID ${target.pid}`, `Executable: ${target.binary}`, "Osprey: ", "CPU: ", "Memory: ", "Runtime: "]) {
      assert.ok(tip.includes(needle), `tooltip carries "${needle}": ${tip}`);
    }
  });

  test("a runtime with no memory backend offers and suggests no memory tracking", async function () {
    this.timeout(LSP_RESPONSE_MS);
    const { target } = live;
    await refreshed();
    const row = await listedRow(target.pid);
    assert.equal(row.contextValue, "process", "a plain profilable row, never the Track-Memory contextValue");
    assert.ok(!tooltipText(row).includes("Memory tracking"), `no memory suggestion: ${tooltipText(row)}`);
    assert.equal(profilerStore().memoryOffered(), false, "the server advertised no memory runtime");
  });

  test("a click on the row opens nothing: an Osprey entry is a native binary, not source", async function () {
    this.timeout(COMMAND_MS);
    const { target } = live;
    await refreshed();
    const row = await listedRow(target.pid);
    assert.equal(row.command, undefined, "no open-on-click for a binary entry");
    const before = vscode.window.activeTextEditor?.document.uri.toString();
    await vscode.commands.executeCommand(uiCommand("revealProcessScript"), row);
    assert.equal(vscode.window.activeTextEditor?.document.uri.toString(), before);
  });

  test("only the CPU current-file launch stays pinned above the rows", async function () {
    this.timeout(LSP_RESPONSE_MS);
    await refreshed();
    const launches = panelRows().filter((row) => row.contextValue === "launchAction");
    assert.deepEqual(launches.map((row) => row.id), [`launchAction:${uiCommand("profileCurrentFileCpu")}`]);
    assert.equal(panelRows()[0]?.id, launches[0]?.id, "the launch is the first row");
  });

  test("copy PID puts the row's PID on the clipboard", async function () {
    this.timeout(COMMAND_MS);
    const { target } = live;
    const row = await listedRow(target.pid);
    await vscode.env.clipboard.writeText("");
    await vscode.commands.executeCommand(uiCommand("copyProcessPid"), row);
    const copied = await pollUntilResult({ fn: async () => vscode.env.clipboard.readText(),
      until: (text) => text !== "", timeoutMs: COMMAND_MS, description: "the clipboard holds the PID" });
    assert.equal(copied, String(target.pid));
  });

  test("the panel is idle chrome when nothing is profiling", () => {
    assert.deepEqual(processesViewState(), { message: undefined, badge: undefined });
  });
});

suite("Processes panel — filter, sort and group re-project the same list", () => {
  let first: Target | undefined;
  let second: Target | undefined;
  suiteSetup(async function () {
    this.timeout(ACTIVATION_MS + BUILD_MS * 2 + PROCESS_START_MS * 2);
    await activateWithServer();
    first = await Target.start("view_alpha");
    second = await Target.start("view_beta");
    await refreshed();
  });
  suiteTeardown(() => {
    processesProvider().setFilter("");
    first?.stop();
    second?.stop();
  });

  test("a PID filter narrows the rows to that process", async function () {
    this.timeout(LSP_RESPONSE_MS);
    assert.ok(first && second);
    await listedRow(first.pid);
    processesProvider().setFilter(String(first.pid));
    assert.ok(rowOf(first.pid), "the matching row stays");
    assert.equal(rowOf(second.pid), undefined, "the other row is filtered out");
  });

  test("a name filter matches case-insensitively", () => {
    assert.ok(first && second);
    processesProvider().setFilter("VIEW_BETA");
    assert.ok(rowOf(second.pid), "the second program matches its name in any case");
    assert.equal(rowOf(first.pid), undefined);
  });

  test("a filter that hides every process shows an honest placeholder, never an empty list", () => {
    processesProvider().setFilter("no-such-process-anywhere");
    const message = panelRows().find((row) => row.contextValue === "processesMessage");
    assert.ok(message, "a placeholder row is shown");
    assert.ok(String(message.label).includes('No process matches "no-such-process-anywhere"'), `placeholder names the filter: ${String(message.label)}`);
    assert.ok(panelRows().some((row) => row.contextValue === "launchAction"), "the launch survives the filter");
  });

  test("clearing the filter restores both rows", () => {
    assert.ok(first && second);
    processesProvider().setFilter("");
    assert.ok(rowOf(first.pid) && rowOf(second.pid));
  });

  test("the panel opens grouped by runtime version, both programs under one header with a count", () => {
    assert.ok(first && second);
    assert.equal(profilerStore().processes.value.groupMode, "version", "the default grouping");
    const provider = processesProvider();
    const groups = provider.getChildren().filter((row) => row.contextValue === "processGroup");
    assert.ok(groups.length > 0, "group headers render");
    const ids = [`process:${first.pid}`, `process:${second.pid}`];
    const holding = groups.filter((group) => provider.getChildren(group).some((row) => ids.includes(row.id ?? "")));
    assert.equal(holding.length, 1, "both programs of this build share one version group");
    assert.ok(Number(holding[0]?.description) >= 2, `the count badge counts members: ${holding[0]?.description}`);
  });

  test("group cycles through every mode and back to none", async () => {
    const store = profilerStore();
    const seen: string[] = [store.processes.value.groupMode];
    for (let i = 0; i < 4; i++) {
      await vscode.commands.executeCommand(uiCommand("groupProcesses"));
      seen.push(store.processes.value.groupMode);
    }
    assert.deepEqual(seen, ["version", "interpreter", "user", "parent", "none"]);
    assert.ok(!panelRows().some((row) => row.contextValue === "processGroup"), "flat again");
  });

  test("sort cycles through every mode in order and wraps", async () => {
    const store = profilerStore();
    const seen: string[] = [store.processes.value.sortMode];
    for (let i = 0; i < 6; i++) {
      await vscode.commands.executeCommand(uiCommand("sortProcesses"));
      seen.push(store.processes.value.sortMode);
    }
    assert.deepEqual(seen, ["cpu", "memory", "pid", "name", "runtime", "version", "cpu"]);
  });

  test("PID sort orders the rows by ascending PID", async () => {
    assert.ok(first && second);
    const store = profilerStore();
    const sortMode = (): string => store.processes.value.sortMode;
    while (sortMode() !== "pid") {
      await vscode.commands.executeCommand(uiCommand("sortProcesses"));
    }
    const pids = panelRows().filter((row) => row.id?.startsWith("process:") === true)
      .map((row) => Number(row.id?.slice("process:".length)));
    assert.deepEqual(pids, [...pids].sort((a, b) => a - b), `ascending: ${pids.join(", ")}`);
    assert.ok(pids.includes(first.pid) && pids.includes(second.pid), "both live programs are listed");
    while (sortMode() !== "cpu") {
      await vscode.commands.executeCommand(uiCommand("sortProcesses"));
    }
  });
});

suite("Processes panel — kill", () => {
  suiteSetup(async function () {
    this.timeout(ACTIVATION_MS);
    await activateWithServer();
  });

  test("kill terminates a recognised Osprey program and it leaves the listing", async function () {
    this.timeout(BUILD_MS + PROCESS_START_MS + LSP_RESPONSE_MS);
    const victim = await Target.start("kill_victim");
    try {
      const result = await execute<{ pid: number; killed: boolean }>("kill", { pid: victim.pid });
      assert.deepEqual(result, { pid: victim.pid, killed: true });
      await waitFor(() => victim.alive(), (alive) => !alive, LSP_RESPONSE_MS, "the victim exits");
      const listing = await execute<ProcessList>("processes", { workspace_roots: [] });
      assert.ok(!listing.processes.some((proc) => proc.pid === victim.pid), "gone from the listing");
    } finally {
      victim.stop();
    }
  });

  test("kill refuses a process no runtime recognises — it can never hit the editor", async function () {
    this.timeout(LSP_RESPONSE_MS);
    const error = await executeError("kill", { pid: process.pid });
    assert.equal(error?.code, -32002, error?.message ?? "no error");
  });

  test("kill refuses a PID that does not exist", async function () {
    this.timeout(LSP_RESPONSE_MS);
    const error = await executeError("kill", { pid: 999_999 });
    assert.equal(error?.code, -32001, error?.message ?? "no error");
  });

  test("the kill command without a row does nothing and raises nothing", async function () {
    this.timeout(COMMAND_MS);
    await vscode.commands.executeCommand(uiCommand("killProcess"));
    assert.equal(profilerStore().profiler.value.cpu, "idle");
  });
});

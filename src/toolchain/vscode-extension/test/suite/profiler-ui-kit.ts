// Tests for [PROF-VSCODE-SHARED]. The fixtures and test doubles the shared
// UI's component suites use, ported from SharpLsp's profiler UI helpers and
// bound to Osprey's live extension: a detached host is Osprey's own with only
// the client and debuggee map swapped, because `createProfilerStore` installs
// its host's identity process-wide.
import * as assert from "node:assert/strict";
import { signal, type ReadonlySignal, type Signal } from "@preact/signals-core";
import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";
import type { LaunchMetric, ProfilerHost, ProfilerStore } from "@nimblesite/lsp-profiler-ui";
import type { ProcessInfo } from "@nimblesite/lsp-profiler-ui/out/src/process-explorer-rows";
import type { ProcessInfo as WireProcess } from "@nimblesite/lsp-profiler-ui/out/src/wire-generated";
import { numberField, rawField } from "@nimblesite/lsp-profiler-ui/out/src/unknown-shape";
import { profilerStore } from "./profiler-kit";

/** Write to a signal a consumer only reads (test seams). */
export function seedSignal<T>(target: ReadonlySignal<T>, value: T): void {
  (target as Signal<T>).value = value;
}

/** A language client stand-in exposing only `members`. */
export function fakeLanguageClient(members: Record<string, unknown>): LanguageClient {
  return members as unknown as LanguageClient;
}

/** A detached copy of Osprey's host: no client, no debuggee. */
export function fakeHost(overrides: Partial<ProfilerHost> = {}): ProfilerHost {
  return {
    ...profilerStore().host,
    client: signal<LanguageClient | undefined>(undefined),
    sessionIdToPid: signal<ReadonlyMap<string, number>>(new Map()),
    ...overrides,
  };
}

/** The launch configuration Osprey's host builds for `metric`, or `undefined`. */
export function launchConfigOf(metric: LaunchMetric, program: string): vscode.DebugConfiguration | undefined {
  return profilerStore().host.launchConfig(metric, program);
}

/** The wire form of a UI process, for test doubles that stand in for the server. */
export function toWireProcess(process: ProcessInfo): WireProcess {
  return {
    pid: process.pid,
    ppid: process.ppid,
    name: process.name,
    runtime: process.runtime,
    ...(process.runtimeVersion === null ? {} : { runtime_version: process.runtimeVersion }),
    ...(process.interpreterPath === null ? {} : { executable: process.interpreterPath }),
    ...(process.script === null ? {} : { entry: process.script }),
    cpu_percent: process.cpuPercent,
    memory_bytes: process.memoryBytes,
    runtime_secs: process.runtimeSecs,
    ...(process.user === null ? {} : { user: process.user }),
    requires_elevation: process.requiresElevation,
    in_workspace: process.inWorkspace,
    ...(process.launcher === null ? {} : { launcher: process.launcher }),
    attachable: process.debuggable,
    ...(process.undebuggableReason === null ? {} : { unattachable_reason: process.undebuggableReason }),
  };
}

/** Turn the Processes panel's grouping off: it opens grouped by runtime version. */
export function ungroup(store: ProfilerStore): void {
  while (store.processes.value.groupMode !== "none") {
    store.cycleProcessGroup();
  }
}

const MB = 1024 * 1024;

/** An attachable Osprey program row; each caller overrides what it exercises. */
export function processFixture(pid: number, overrides: Partial<ProcessInfo> = {}): ProcessInfo {
  return {
    pid,
    ppid: 1,
    name: "spin",
    interpreterPath: "/work/spin",
    script: "spin",
    runtime: "osprey",
    runtimeVersion: "0.0.0-dev",
    cpuPercent: 0,
    memoryBytes: MB,
    runtimeSecs: 1,
    user: null,
    requiresElevation: false,
    inWorkspace: false,
    launcher: null,
    debuggable: true,
    undebuggableReason: null,
    ...overrides,
  };
}

/** The `experimental.lspProfiler` block `osprey lsp` advertises: CPU only, no memory backend. */
export const OSPREY_CAPABILITIES = {
  runtimes: [{ id: "osprey", display_name: "Osprey", source_entries: false,
    cpu: { cooperative: false, line_level: true, live_progress: true, native_frames: true } }],
} as const;

/** A client double serving `processes` and recording every executeCommand it is sent. */
export function processClient(processes: readonly ProcessInfo[], requests: { command: string; arguments: readonly unknown[] }[] = [],
  capabilities: unknown = OSPREY_CAPABILITIES): LanguageClient {
  return fakeLanguageClient({
    isRunning: () => true,
    initializeResult: { capabilities: { experimental: { lspProfiler: capabilities } } },
    onDidChangeState: (): vscode.Disposable => ({ dispose: () => undefined }),
    sendRequest: async (_method: string, param?: { command: string; arguments: readonly unknown[] }) => {
      if (param !== undefined) requests.push(param);
      return param?.command === "osprey.lspProfiler.processes" ? { processes: processes.map(toWireProcess) } : undefined;
    },
  });
}

/** Read the PID a process row carries (the argument passed to inline commands). */
export function rowPid(item: vscode.TreeItem): number | undefined {
  return numberField(rawField(item, "process"), "pid");
}

/** The notifications shown while a body ran, by severity, with the actions each offered. */
export interface Toasts {
  readonly info: { message: string; actions: string[] }[];
  readonly warning: string[];
  readonly error: string[];
}

type ToastWindow = Record<"showInformationMessage" | "showWarningMessage" | "showErrorMessage", unknown>;

/** Swap every notification for a recorder while `body` runs. */
export async function captureToasts(body: () => PromiseLike<unknown>): Promise<Toasts> {
  const toasts: Toasts = { info: [], warning: [], error: [] };
  const window = vscode.window as unknown as ToastWindow;
  const original: ToastWindow = {
    showInformationMessage: window.showInformationMessage,
    showWarningMessage: window.showWarningMessage,
    showErrorMessage: window.showErrorMessage,
  };
  window.showInformationMessage = async (message: string, ...items: unknown[]) => {
    toasts.info.push({ message, actions: items.filter((i): i is string => typeof i === "string") });
    return undefined;
  };
  window.showWarningMessage = async (message: string) => void toasts.warning.push(message);
  window.showErrorMessage = async (message: string) => void toasts.error.push(message);
  try {
    await body();
  } finally {
    Object.assign(window, original);
  }
  return toasts;
}

/** What one applied decoration must show: its line, a colour, text it carries. */
export interface PaintedExpectation {
  readonly line?: number;
  readonly color?: string;
  readonly includes?: string;
}

/** Assert a decoration was applied and shows exactly what `expect` names. */
export function assertPainted(painted: { line: number; color: string; contentText: string } | undefined, expect: PaintedExpectation, what: string): void {
  assert.ok(painted !== undefined, `${what}: a decoration must be applied`);
  if (expect.line !== undefined) assert.equal(painted.line, expect.line, `${what}: it lands on its line`);
  if (expect.color !== undefined) assert.equal(painted.color, expect.color, `${what}: it wears its colour`);
  if (expect.includes !== undefined) assert.ok(painted.contentText.includes(expect.includes), painted.contentText);
}

/** An Osprey source with twelve numbered lines, for decorations to land on. */
export const DECORATED_SOURCE = Array.from({ length: 12 }, (_, index) => `// line ${index + 1}`).join("\n");

// Implements [PROF-VSCODE-SHARED]. Osprey's `ProfilerHost` for
// @nimblesite/lsp-profiler-ui: identity, palette, the language client, and the
// debuggee PIDs lldb-dap reports. The Processes panel, live CPU sessions, heat
// decorations and results panel all live in the library; `osprey lsp` serves
// them through lspkit's profiler service [PROF-LSP-HOST].

import * as path from "path";
import * as vscode from "vscode";
import type { LanguageClient } from "vscode-languageclient/node";
import {
  registerProfilerUi,
  type LaunchMetric,
  type ProfilerHost,
  type ProfilerUi,
} from "@nimblesite/lsp-profiler-ui";
import { OSPREY_PALETTE } from "./palette";
import { signal, type Signal } from "@preact/signals-core";

const DEBUG_TYPE = "osprey";

/** What `activate` keeps: the registered UI and the client seam to fill once the server starts. */
export interface OspreyProfiler {
  readonly ui: ProfilerUi;
  /** Set once the language client is running; the library reacts to the change. */
  setClient(client: LanguageClient | undefined): void;
}

/** Register the shared profiler UI with Osprey's identity. */
export function registerProfiler(
  context: vscode.ExtensionContext,
  log: vscode.OutputChannel,
): OspreyProfiler {
  const client = signal<LanguageClient | undefined>(undefined);
  const write = (message: string) => log.appendLine(`[profiler] ${message}`);
  const host: ProfilerHost = {
    idPrefix: "osprey",
    brand: "Osprey",
    palette: OSPREY_PALETTE,
    configSection: "osprey",
    debugType: DEBUG_TYPE,
    languageIds: ["osprey", "osprey-ml"],
    languageNames: ["Osprey"],
    client,
    logger: { trace: write, debug: write, info: write, warn: write, error: write },
    sessionIdToPid: trackDebuggeePids(context),
    launchConfig: profilingLaunch,
  };
  const ui = registerProfilerUi(context, host);
  return {
    ui,
    setClient(next) {
      client.value = next;
    },
  };
}

/**
 * lldb-dap reports the debuggee's PID in the DAP `process` event; record it
 * per session so the library can profile the program being debugged.
 */
function trackDebuggeePids(
  context: vscode.ExtensionContext,
): Signal<ReadonlyMap<string, number>> {
  const pids = signal<ReadonlyMap<string, number>>(new Map());
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterTrackerFactory(DEBUG_TYPE, {
      createDebugAdapterTracker: (session) => ({
        onDidSendMessage: (message: unknown) => {
          const pid = processEventPid(message);
          if (pid !== undefined) pids.value = new Map([...pids.value, [session.id, pid]]);
        },
        onWillStopSession: () => {
          pids.value = new Map([...pids.value].filter(([id]) => id !== session.id));
        },
      }),
    }),
  );
  return pids;
}

/** The `systemProcessId` of a DAP `process` event, or `undefined` for any other message. */
export function processEventPid(message: unknown): number | undefined {
  if (typeof message !== "object" || message === null) return undefined;
  if (Reflect.get(message, "type") !== "event" || Reflect.get(message, "event") !== "process") {
    return undefined;
  }
  const body: unknown = Reflect.get(message, "body");
  const pid: unknown = typeof body === "object" && body !== null ? Reflect.get(body, "systemProcessId") : undefined;
  return typeof pid === "number" && pid > 0 ? pid : undefined;
}

/**
 * "Run & Profile CPU" debug-launches `program` through the Osprey adapter,
 * which builds it and runs it under lldb-dap; the library attaches a CPU
 * session to the PID the adapter reports. Osprey has no memory backend
 * [PROF-LSP-RUNTIME], so a memory launch is not offered.
 */
export function profilingLaunch(
  metric: LaunchMetric,
  program: string,
): vscode.DebugConfiguration | undefined {
  return metric === "cpu"
    ? {
        type: DEBUG_TYPE,
        request: "launch",
        name: "Run & Profile CPU (Current File)",
        program,
        cwd: path.dirname(program),
        profileOnLaunch: true,
      }
    : undefined;
}

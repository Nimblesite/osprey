// The Osprey Debug panel: an activity-bar TreeView that, during a debug
// session, surfaces what is happening inside the Osprey program — the call
// stack, the current frame's locals, the program/binary under debug — and
// leaves first-class room for the planned CPU and memory profiling views.
//
// The data model and tree-building are PURE functions (no `vscode`), so they
// unit-test directly; only `registerOspreyDebugPanel` and the TreeDataProvider
// touch the editor. The DAP reads mirror @nimblesite/lspkit-debug's client.

import {
  debug,
  EventEmitter,
  ExtensionContext,
  ThemeIcon,
  TreeDataProvider,
  TreeItem,
  TreeItemCollapsibleState,
  window,
  type DebugSession,
} from "vscode";
import { pathBasename } from "./profiler/flame-html";

/** A session that can issue DAP requests (the bit we need from DebugSession). */
export interface DapRequester {
  customRequest(command: string, args?: unknown): Thenable<unknown>;
}

/** Where the debuggee is in its lifecycle. */
export type DebugState = "inactive" | "running" | "stopped";

/** The program/binary under debug. */
export interface ProgramInfo {
  sourceProgram?: string;
  debugBinary?: string;
}

/** A stack frame as the panel renders it. */
export interface FrameInfo {
  id: number;
  name: string;
  path?: string;
  line: number;
  column: number;
}

/** A variable as the panel renders it. */
export interface VarInfo {
  name: string;
  value: string;
  type?: string;
}

/** Everything the panel renders at one moment. */
export interface DebugSnapshot {
  state: DebugState;
  program?: ProgramInfo;
  frames: FrameInfo[];
  topVariables: VarInfo[];
}

/** The empty (no session) snapshot. */
export const EMPTY_SNAPSHOT: DebugSnapshot = {
  state: "inactive",
  frames: [],
  topVariables: [],
};

/** A node in the Osprey Debug tree. */
export interface DebugNode {
  kind: "section" | "info" | "frame" | "variable" | "placeholder";
  label: string;
  description?: string;
  icon?: string;
  children?: DebugNode[];
}

/** "Program" section: source, compiled binary, and live state. */
export function buildProgramNodes(snapshot: DebugSnapshot): DebugNode[] {
  const stateLabel: Record<DebugState, string> = {
    inactive: "No active session",
    running: "Running",
    stopped: "Paused",
  };
  const nodes: DebugNode[] = [
    { kind: "info", label: "State", description: stateLabel[snapshot.state], icon: "pulse" },
  ];
  if (snapshot.program?.sourceProgram) {
    nodes.push({
      kind: "info",
      label: "Source",
      description: pathBasename(snapshot.program.sourceProgram),
      icon: "file-code",
    });
  }
  if (snapshot.program?.debugBinary) {
    nodes.push({
      kind: "info",
      label: "Debug binary",
      description: pathBasename(snapshot.program.debugBinary),
      icon: "file-binary",
    });
  }
  return nodes;
}

/** "Call Stack" section, one node per frame (top frame first). */
export function buildStackNodes(frames: FrameInfo[]): DebugNode[] {
  if (frames.length === 0) {
    return [{ kind: "placeholder", label: "Run to a breakpoint to inspect the stack", icon: "debug-stackframe" }];
  }
  return frames.map((frame) => ({
    kind: "frame",
    label: frame.name,
    description: frame.path ? `${pathBasename(frame.path)}:${frame.line}` : `line ${frame.line}`,
    icon: "debug-stackframe",
  }));
}

/** "Variables" section: the current frame's locals/parameters. */
export function buildVariableNodes(variables: VarInfo[]): DebugNode[] {
  if (variables.length === 0) {
    return [{ kind: "placeholder", label: "No locals in scope", icon: "symbol-variable" }];
  }
  return variables.map((variable) => ({
    kind: "variable",
    label: variable.name,
    description: variable.type ? `${variable.value} : ${variable.type}` : variable.value,
    icon: "symbol-variable",
  }));
}

/**
 * Profiling sections — intentional placeholders that reserve the CPU and memory
 * profiling surfaces ([DEBUGGER-RUNTIME]). They map onto the LspKit profiling
 * launch flags (profileOnLaunch / memoryTrackOnLaunch) and become live views
 * once the runtime emits sampling/allocation streams.
 */
export function buildProfilingNodes(): DebugNode[] {
  return [
    {
      kind: "section",
      label: "Performance Profiling",
      icon: "dashboard",
      children: [
        { kind: "placeholder", label: "CPU sampling", description: "run ⌘⇧P → Osprey: Profile Current File", icon: "watch" },
      ],
    },
    {
      kind: "section",
      label: "Memory",
      icon: "server",
      children: [
        { kind: "placeholder", label: "Heap / allocations", description: "planned — memoryTrackOnLaunch", icon: "database" },
      ],
    },
  ];
}

/** Assemble the full tree (top-level sections) for a snapshot. Pure. */
export function buildTree(snapshot: DebugSnapshot): DebugNode[] {
  return [
    { kind: "section", label: "Program", icon: "rocket", children: buildProgramNodes(snapshot) },
    { kind: "section", label: "Call Stack", icon: "list-tree", children: buildStackNodes(snapshot.frames) },
    { kind: "section", label: "Variables", icon: "symbol-namespace", children: buildVariableNodes(snapshot.topVariables) },
    ...buildProfilingNodes(),
  ];
}

interface ThreadsResponse {
  threads?: { id: number }[];
}
interface StackResponse {
  stackFrames?: FrameInfo[];
}
interface ScopesResponse {
  scopes?: { variablesReference: number }[];
}
interface VariablesResponse {
  variables?: VarInfo[];
}

/** Read a fresh snapshot from a stopped session. Pure but for the DAP reads. */
export async function readDebugSnapshot(
  session: DapRequester,
  program: ProgramInfo | undefined,
): Promise<DebugSnapshot> {
  const threads = ((await session.customRequest("threads")) as ThreadsResponse).threads ?? [];
  for (const thread of threads) {
    const stack = (await session.customRequest("stackTrace", {
      threadId: thread.id,
      startFrame: 0,
      levels: 20,
    })) as StackResponse;
    const frames = stack.stackFrames ?? [];
    if (frames.length > 0) {
      const topVariables = await readTopVariables(session, frames[0].id);
      return { state: "stopped", program, frames, topVariables };
    }
  }
  return { state: "running", program, frames: [], topVariables: [] };
}

async function readTopVariables(session: DapRequester, frameId: number): Promise<VarInfo[]> {
  const { scopes } = (await session.customRequest("scopes", { frameId })) as ScopesResponse;
  const groups = await Promise.all(
    (scopes ?? [])
      .filter((scope) => scope.variablesReference > 0)
      .map((scope) => session.customRequest("variables", { variablesReference: scope.variablesReference })),
  );
  return groups.flatMap((group) => (group as VariablesResponse).variables ?? []);
}

/** The TreeDataProvider backing the Osprey Debug view. */
export class OspreyDebugTreeProvider implements TreeDataProvider<DebugNode> {
  private snapshot: DebugSnapshot = EMPTY_SNAPSHOT;
  private readonly emitter = new EventEmitter<DebugNode | undefined>();
  public readonly onDidChangeTreeData = this.emitter.event;

  /** Replace the snapshot and refresh the view. */
  public update(snapshot: DebugSnapshot): void {
    this.snapshot = snapshot;
    this.emitter.fire(undefined);
  }

  /** The current snapshot (for tests/inspection). */
  public current(): DebugSnapshot {
    return this.snapshot;
  }

  public getTreeItem(node: DebugNode): TreeItem {
    const collapsible =
      node.children && node.children.length > 0
        ? TreeItemCollapsibleState.Expanded
        : TreeItemCollapsibleState.None;
    const item = new TreeItem(node.label, collapsible);
    item.description = node.description;
    item.contextValue = node.kind;
    if (node.icon) {
      item.iconPath = new ThemeIcon(node.icon);
    }
    return item;
  }

  public getChildren(node?: DebugNode): DebugNode[] {
    return node ? node.children ?? [] : buildTree(this.snapshot);
  }
}

/** Pull the program identity out of a session's launch configuration. */
export function programInfoFromConfig(config: Record<string, unknown>): ProgramInfo {
  return {
    sourceProgram:
      (config.sourceProgram as string | undefined) ?? (config.program as string | undefined),
    debugBinary: config.program as string | undefined,
  };
}

/** A DAP message as the tracker sees it (only the fields we branch on). */
export interface TrackedMessage {
  type?: string;
  event?: string;
}

/**
 * Drive a panel from one session's DAP message stream: refresh the snapshot on
 * each `stopped`, mark running on `continued`. Pure of VS Code registration, so
 * it unit-tests by feeding messages and a fake requester. Returns the message
 * handler the tracker installs.
 */
export function makePanelMessageHandler(
  session: DapRequester,
  provider: OspreyDebugTreeProvider,
  program: ProgramInfo,
): (message: TrackedMessage) => void {
  return (message) => {
    if (message.type !== "event") {
      return;
    }
    if (message.event === "stopped") {
      void readDebugSnapshot(session, program).then((snapshot) => provider.update(snapshot));
    } else if (message.event === "continued") {
      provider.update({ state: "running", program, frames: [], topVariables: [] });
    }
  };
}

/** Register the Osprey Debug panel and keep it in step with debug sessions. */
export function registerOspreyDebugPanel(context: ExtensionContext): OspreyDebugTreeProvider {
  const provider = new OspreyDebugTreeProvider();
  context.subscriptions.push(
    debug.registerDebugAdapterTrackerFactory("osprey", {
      createDebugAdapterTracker(session: DebugSession) {
        const program = programInfoFromConfig(session.configuration as Record<string, unknown>);
        provider.update({ state: "running", program, frames: [], topVariables: [] });
        return { onDidSendMessage: makePanelMessageHandler(session, provider, program) };
      },
    }),
    debug.onDidTerminateDebugSession(() => provider.update(EMPTY_SNAPSHOT)),
    window.registerTreeDataProvider("ospreyDebugView", provider),
  );
  return provider;
}

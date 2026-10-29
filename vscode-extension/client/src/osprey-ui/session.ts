// Implements [UI-STORE], [UI-EVENTS] and [UI-PROTOCOL]: one serialized host
// session owns the opaque runtime state, keyed regions, host-held slices and
// in-flight completions. Capability requests are started only from a validated
// envelope, and a completion from a superseded session is never delivered.

import {
  applyEnvelope,
  completionFrame,
  dispatchFrame,
  editFrame,
  parseEnvelope,
  parseUiEvent,
  record,
  startFrame,
  type CompletionOutcome,
  type NativeRequest,
  type Regions,
} from "./protocol";

interface IssuedSnapshot {
  readonly state: string;
  readonly regions: Regions;
}

const MAX_ISSUED_SNAPSHOTS = 32;
const issuedKey = (region: unknown, revision: unknown): string => JSON.stringify([region, revision]);

function rememberIssued(history: Map<string, IssuedSnapshot>, snapshot: IssuedSnapshot): void {
  for (const [key, region] of snapshot.regions) {
    const id = issuedKey(key, region.revision);
    if (!history.has(id)) history.set(id, snapshot);
  }
  while (history.size > MAX_ISSUED_SNAPSHOTS) {
    const oldest = history.keys().next().value;
    if (oldest !== undefined) history.delete(oldest);
  }
}

interface CommandResult {
  readonly stdout: string;
  readonly exitCode: number;
}

export interface SessionDeps {
  run(frame: string, slices: ReadonlyMap<string, string>): Promise<CommandResult>;
  render(regions: Regions, diagnostics: readonly string[], theme: unknown, slices: ReadonlyMap<string, string>): string;
  send(message: unknown): void;
  execute(request: NativeRequest): Promise<unknown>;
  now(): number;
  report(error: Error): void;
}

export interface WorkspaceSession {
  start(context: Record<string, unknown>): Promise<void>;
  message(raw: unknown): void;
  dispose(): void;
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Create the generic host loop around one Osprey workspace session. */
export function createWorkspaceSession(deps: SessionDeps): WorkspaceSession {
  let state = "";
  let regions: Regions = new Map();
  let slices = new Map<string, string>();
  let diagnostics: readonly string[] = [];
  let theme: unknown = null;
  let chain: Promise<void> = Promise.resolve();
  let generation = 0;
  let disposed = false;
  const issued = new Map<string, IssuedSnapshot>();

  const current = (expected: number): boolean => !disposed && expected === generation;

  const fail = (error: unknown): void => {
    if (disposed) return;
    const message = messageOf(error);
    diagnostics = [message];
    deps.report(error instanceof Error ? error : new Error(message));
    deps.send({ type: "fatal", message });
  };

  const publish = (): void => {
    if (disposed) return;
    try {
      deps.send({ type: "render", html: deps.render(regions, diagnostics, theme, slices), diagnostics });
    } catch (error) {
      fail(error);
    }
  };

  const apply = (result: CommandResult, expected: number, edit?: number): void => {
    if (!current(expected)) return;
    if (result.exitCode !== 0) throw new Error(`The Osprey workspace exited with code ${result.exitCode}${result.stdout ? `: ${result.stdout.trim()}` : ""}`);
    const envelope = parseEnvelope(result.stdout);
    state = envelope.state;
    regions = applyEnvelope(regions, envelope);
    for (const slice of envelope.slices) slices.set(slice.key, slice.text);
    diagnostics = envelope.diagnostics;
    if (envelope.theme !== null) theme = envelope.theme;
    rememberIssued(issued, { state, regions });
    if (edit !== undefined) deps.send(envelope.diagnostics.length === 0
      ? { type: "edit-result", edit, accepted: true }
      : { type: "edit-result", edit, accepted: false, error: envelope.diagnostics.join("\n") });
    publish();
    for (const request of envelope.requests) void complete(request, expected);
  };

  const enqueueFrame = (build: () => string, expected: number, edit?: number): Promise<void> => {
    const step = async (): Promise<void> => {
      if (!current(expected)) return;
      const frame = build();
      if (!current(expected)) return;
      apply(await deps.run(frame + "\n", slices), expected, edit);
    };
    chain = chain.then(step, step).catch(fail);
    return chain;
  };

  const complete = async (request: NativeRequest, expected: number): Promise<void> => {
    let outcome: CompletionOutcome;
    try {
      outcome = { ok: true as const, result: await deps.execute(request) };
    } catch (error) {
      outcome = { ok: false as const, error: messageOf(error) };
    }
    if (!current(expected)) return;
    await enqueueFrame(() => completionFrame(state, deps.now(), request.ticket, outcome), expected);
  };

  const start = async (context: Record<string, unknown>): Promise<void> => {
    if (disposed) throw new Error("The package workspace session is disposed");
    generation += 1;
    state = "";
    regions = new Map();
    slices = new Map();
    diagnostics = [];
    theme = null;
    issued.clear();
    publish();
    await enqueueFrame(() => startFrame(context), generation);
  };

  const edited = (raw: Record<string, unknown>): void => {
    if (typeof raw.edit !== "number" || !Number.isSafeInteger(raw.edit) || raw.edit < 1) return;
    const now = deps.now();
    const snapshot = parseUiEvent(raw, regions, now) ? { state, regions } : issued.get(issuedKey(raw.region, raw.revision));
    const event = snapshot ? parseUiEvent(raw, snapshot.regions, now) : undefined;
    if (!event || !snapshot) {
      deps.send({ type: "edit-result", edit: raw.edit, accepted: false,
        error: typeof raw.value === "string" ? "The original text control is no longer available; the edit was not applied."
          : "The original control is no longer available; the edit was not applied." });
      return;
    }
    void enqueueFrame(() => editFrame(state, snapshot.state, event), generation, raw.edit);
  };

  const message = (raw: unknown): void => {
    if (disposed || !record(raw) || raw.type !== "event") return;
    if ("edit" in raw) { edited(raw); return; }
    const event = parseUiEvent(raw, regions, deps.now());
    if (!event) return;
    void enqueueFrame(() => dispatchFrame(state, event), generation);
  };

  const dispose = (): void => {
    disposed = true;
    generation += 1;
    state = "";
    regions = new Map();
    slices = new Map();
    issued.clear();
  };

  return { start, message, dispose };
}

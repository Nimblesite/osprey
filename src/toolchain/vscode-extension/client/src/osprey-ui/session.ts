// Implements [NATIVE-SHM-DELIVERY] and [NATIVE-SHM-ASK]: one serialized host session owns the
// live application instance, the host-held slices, and the capability requests each frame asks
// for. Every call's frame is published to the webview before its asks are answered; a completion
// returns as event kind 8 and draws the application's next frame.

import { diagnostics, readAsks, requests, sliceWrites, type Ask, type Completion } from "./asks";
import { readFrame, type Shown } from "./webview/rows";
import { EVENT } from "./webview/events";
import type { FrameResult, NativeRuntime, NativeTouch, OpenOptions } from "./wasi-runner";

export interface FrameMessage {
  readonly type: "frame";
  readonly shapes: ArrayBuffer;
  readonly views: ArrayBuffer;
  readonly text: ArrayBuffer;
  readonly assets: Readonly<Record<string, string>>;
  readonly diagnostics: readonly string[];
}

export interface BusyMessage {
  readonly type: "busy";
  readonly active: boolean;
}

export interface FatalMessage {
  readonly type: "fatal";
  readonly message: string;
}

export interface SessionDeps {
  readonly runtime: NativeRuntime;
  send(message: FrameMessage | BusyMessage | FatalMessage): void;
  execute(ask: Ask, slices: ReadonlyMap<string, string>): Promise<Completion>;
  assets?(shown: Shown): Readonly<Record<string, string>>;
  report(error: Error): void;
}

export interface WorkspaceSession {
  start(options: OpenOptions): Promise<void>;
  message(raw: unknown): void;
  appearance(dark: boolean): void;
  dispose(): void;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function integer(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/** A webview event as the typed host call; an unknown or malformed event is ignored. */
export function parseTouch(raw: unknown): NativeTouch | undefined {
  if (!record(raw) || raw.type !== "event" || !integer(raw.kind) || raw.kind > 11) return undefined;
  const ordinal = raw.ordinal === undefined ? 0 : raw.ordinal;
  if (!integer(ordinal)) return undefined;
  const words = raw.words === undefined ? "" : raw.words;
  const detail = raw.detail === undefined ? "" : raw.detail;
  const x = raw.x === undefined ? 0 : raw.x;
  const y = raw.y === undefined ? 0 : raw.y;
  const down = raw.down === undefined ? false : raw.down;
  const shift = raw.shift === undefined ? false : raw.shift;
  if (typeof words !== "string" || typeof detail !== "string") return undefined;
  if (typeof x !== "number" || !Number.isFinite(x) || typeof y !== "number" || !Number.isFinite(y)) return undefined;
  if (typeof down !== "boolean" || typeof shift !== "boolean") return undefined;
  return { kind: raw.kind, ordinal, words, detail, x, y, down, shift };
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

/** Create the generic host loop around one live Osprey application. */
export function createWorkspaceSession(deps: SessionDeps): WorkspaceSession {
  let slices = new Map<string, string>();
  let chain: Promise<void> = Promise.resolve();
  let queued = 0;
  let disposed = false;
  let started = false;
  let generation = 0;

  const current = (expected: number): boolean => !disposed && expected === generation;

  const fail = (error: unknown): void => {
    if (disposed) return;
    const message = messageOf(error);
    deps.report(error instanceof Error ? error : new Error(message));
    deps.send({ type: "fatal", message });
  };

  const enqueue = (call: () => Promise<FrameResult>, expected: number): Promise<void> => {
    queued += 1;
    if (queued === 1) deps.send({ type: "busy", active: true });
    const step = async (): Promise<void> => {
      if (!current(expected)) return;
      const result = await call();
      if (!current(expected)) return;
      publish(result, expected);
    };
    chain = chain.then(step, step).catch(fail).finally(() => {
      queued -= 1;
      if (queued === 0 && !disposed) deps.send({ type: "busy", active: false });
    });
    return chain;
  };

  const assetsOf = (runs: FrameResult["runs"]): Readonly<Record<string, string>> => {
    if (deps.assets === undefined) return {};
    try {
      return deps.assets(readFrame({ shapes: runs.shapes, views: runs.views, text: runs.text }));
    } catch {
      return {};
    }
  };

  const publish = (result: FrameResult, expected: number): void => {
    if (!current(expected)) return;
    const asked = readAsks(result.runs.asks, result.runs.text);
    for (const write of sliceWrites(asked)) slices.set(write.key, write.text);
    deps.send({ type: "frame", shapes: result.runs.shapes, views: result.runs.views, text: result.runs.text,
      assets: assetsOf(result.runs), diagnostics: diagnostics(asked) });
    for (const ask of requests(asked)) void complete(ask, expected);
  };

  const complete = async (ask: Ask, expected: number): Promise<void> => {
    let outcome: Completion;
    try {
      outcome = await deps.execute(ask, slices);
    } catch (error) {
      outcome = { detail: messageOf(error) };
      await enqueue(() => deps.runtime.event(touch(EVENT.completed, 0, ask.ticket, outcome.detail, false)), expected);
      return;
    }
    if (!current(expected)) return;
    await enqueue(() => deps.runtime.event(touch(EVENT.completed, 0, ask.ticket, outcome.detail, true)), expected);
  };

  const touch = (kind: number, ordinal: number, words: string, detail: string, down: boolean): NativeTouch =>
    ({ kind, ordinal, words, detail, x: 0, y: 0, down, shift: false });

  const start = async (options: OpenOptions): Promise<void> => {
    if (disposed) throw new Error("The native session is disposed");
    if (started) return;
    started = true;
    generation += 1;
    slices = new Map();
    await enqueue(() => deps.runtime.open(options), generation);
  };

  const message = (raw: unknown): void => {
    if (disposed) return;
    const parsed = parseTouch(raw);
    if (parsed === undefined) {
      deps.send({ type: "busy", active: queued > 0 });
      return;
    }
    void enqueue(() => deps.runtime.event(parsed), generation);
  };

  const appearance = (dark: boolean): void => {
    if (disposed || !started) return;
    void enqueue(() => deps.runtime.event(touch(EVENT.appearance, 0, "", "", dark)), generation);
  };

  const dispose = (): void => {
    if (disposed) return;
    disposed = true;
    generation += 1;
    slices = new Map();
    deps.runtime.dispose();
  };

  return { start, message, appearance, dispose };
}

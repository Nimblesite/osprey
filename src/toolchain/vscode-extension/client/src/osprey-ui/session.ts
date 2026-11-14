// Implements [NATIVE-SHM-DELIVERY] and [NATIVE-SHM-ASK]: one serialized host session owns the
// live application instance, the host-held slices, and the capability requests each frame asks
// for. Every call's frame is published to the webview before its asks are answered; a completion
// returns as event kind 8 and draws the application's next frame.

import { diagnostics, readAsks, requests, sliceWrites, type Ask, type Completion } from "./asks";
import { RegionStore, sourcesOf, toldFrame, type Regions, type ToldFrame } from "./nodes";
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

/** A frame the Node renderer told as component markup; the runs ride along for its scenes. */
export interface RenderMessage extends Omit<FrameMessage, "type"> {
  readonly type: "render";
  readonly html: string;
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
  send(message: RenderMessage | FrameMessage | BusyMessage | FatalMessage): void;
  execute(ask: Ask, slices: ReadonlyMap<string, string>): Promise<Completion>;
  /** Draws a told frame's regions and theme as the markup the webview shows. */
  draw(frame: ToldFrame, regions: Regions, slices: ReadonlyMap<string, string>): string;
  /** Resolves the asset sources a frame names inside the host's approved root. */
  assets?(sources: readonly string[]): Readonly<Record<string, string>>;
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

/** A webview event as the typed host call; an unknown or malformed event is ignored. Completions,
 * phases, frame ticks and appearance changes are the host's alone: the webview cannot forge them. */
export function parseTouch(raw: unknown): NativeTouch | undefined {
  if (!record(raw) || raw.type !== "event" || !integer(raw.kind) || raw.kind > EVENT.lasso) return undefined;
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
  const regions = new RegionStore();
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

  /** The asset sources a frame names: a told frame's media and boards, or a view row's images,
   * and every scene's atlas entries. The resolver skips a source outside the approved root. */
  const assetsOf = (shown: Shown, frame: ToldFrame | undefined): Readonly<Record<string, string>> => {
    if (deps.assets === undefined) return {};
    const sources = new Set<string>(frame === undefined
      ? shown.controls.flatMap((control) => (control.kind === "image" && control.words !== "" ? [control.words] : []))
      : sourcesOf(frame));
    for (const scene of shown.scenes.values()) {
      for (const source of scene.atlas) if (source !== "") sources.add(source);
    }
    try {
      return deps.assets([...sources]);
    } catch {
      return {};
    }
  };

  /** Publish one call's frame: a told frame as renderer markup with its runs beside it, and a
   * frame of plain view rows for the webview's row view ([NATIVE-SHM-NODES]). */
  const publish = (result: FrameResult, expected: number): void => {
    if (!current(expected)) return;
    const asked = readAsks(result.runs.asks, result.runs.text);
    for (const write of sliceWrites(asked)) keep(write.key, write.text);
    let shown: Shown;
    try {
      shown = readFrame({ shapes: result.runs.shapes, views: result.runs.views, text: result.runs.text });
    } catch (error) {
      fail(error);
      return;
    }
    const notice = diagnostics(asked);
    const told = toldFrame(shown);
    const assets = assetsOf(shown, told);
    try {
      if (told === undefined) {
        deps.send({ type: "frame", shapes: result.runs.shapes, views: result.runs.views,
          text: result.runs.text, assets, diagnostics: notice });
      } else {
        const html = deps.draw(told, regions.adopt(told), slices);
        deps.send({ type: "render", html, shapes: result.runs.shapes, views: result.runs.views,
          text: result.runs.text, assets, diagnostics: notice });
      }
    } catch (error) {
      fail(error);
      return;
    }
    for (const ask of requests(asked)) void complete(ask, expected);
  };

  const keep = (key: string, text: string): void => {
    slices.set(key, text);
    deps.runtime.hold(key, text);
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
    for (const write of outcome.slices ?? []) keep(write.key, write.text);
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
    regions.clear();
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
    regions.clear();
    deps.runtime.dispose();
  };

  return { start, message, appearance, dispose };
}

// A native session around a scripted runtime: every open, event, hold and send is recorded, and
// a test may gate, fail or reshape any runtime answer ([NATIVE-SHM-DELIVERY] [NATIVE-SHM-ASK]).
import { Ask, type Completion } from "../../client/src/osprey-ui/asks";
import { createWorkspaceSession, type BusyMessage, type FatalMessage, type FrameMessage, type RenderMessage, type SessionDeps } from "../../client/src/osprey-ui/session";
import type { FrameResult, NativeRuntime, NativeTouch, OpenOptions } from "../../client/src/osprey-ui/wasi-runner";
import { frame, type AskSpec, type ControlSpec } from "./native-rows-kit";

export type Runs = FrameResult["runs"];
export type Sent = FrameMessage | RenderMessage | BusyMessage | FatalMessage;

/** One frame of plain view rows reading `words`, with `asks`. */
export const runs = (asks: readonly AskSpec[], words = "Ready"): Runs => {
  const built = frame({ views: [{ kind: 2, words }], asks });
  return { shapes: built.shapes, views: built.views, text: built.text, asks: built.asks };
};

/** The runs of a frame built from `views` and `asks`. */
export const told = (views: readonly ControlSpec[], asks: readonly AskSpec[] = []): Runs => {
  const built = frame({ views, asks });
  return { shapes: built.shapes, views: built.views, text: built.text, asks: built.asks };
};

/** Lets every queued microtask and one turn of the event loop run. */
export const settle = (): Promise<void> => new Promise((resolve) => setImmediate(resolve));

export const settleAll = async (turns = 4): Promise<void> => {
  for (let turn = 0; turn < turns; turn += 1) await settle();
};

export const open: OpenOptions = { now: 1, offset: 0, platform: "vscode", home: "/home", dark: true, arguments: ["a", "b"] };

/** A frame that tells one region `key` holding a `kind` node with `text`. */
export const region = (key: string, text: string, kind = "text"): ControlSpec[] => [
  { kind: 34, hint: "region" },
  { kind: 33, parent: 0, hint: "key", words: key },
  { kind: 34, parent: 0, hint: "node" },
  { kind: 33, parent: 2, hint: "kind", words: kind },
  { kind: 33, parent: 2, hint: "text", words: text },
];

/** A theme record whose one token is `name`, then one region. */
export const themed = (name: string, key = "root", text = "Ready"): ControlSpec[] => [
  { kind: 34, hint: "theme" },
  { kind: 35, parent: 0, hint: "tokens" },
  { kind: 34, parent: 1, hint: "" },
  { kind: 33, parent: 2, hint: "name", words: name },
  ...region(key, text).map((spec) => ({ ...spec, parent: spec.parent === undefined ? undefined : spec.parent + 4 })),
];

export interface HarnessOptions {
  readonly asks?: readonly AskSpec[];
  readonly open?: Runs;
  /** The runs the `call`-th event (from 1) answers with. */
  readonly reply?: Runs | ((touch: NativeTouch, call: number) => Runs);
  /** Replaces the runtime's event answer entirely: a gate, a throw or a status. */
  readonly event?: (touch: NativeTouch, call: number) => Promise<FrameResult>;
  readonly answer?: (ask: Ask, slices: ReadonlyMap<string, string>) => Promise<Completion>;
  readonly draw?: SessionDeps["draw"];
}

/** A session and the records of everything it did. */
export function harness(options: HarnessOptions = {}) {
  const events: NativeTouch[] = [];
  const opens: OpenOptions[] = [];
  const sent: Sent[] = [];
  const executed: Ask[] = [];
  const held: ReadonlyMap<string, string>[] = [];
  const holds: [string, string][] = [];
  const reports: Error[] = [];
  const drawn: { key: string; revision: number }[][] = [];
  let calls = 0;
  let disposed = 0;
  const answer = (touch: NativeTouch, call: number): Runs =>
    typeof options.reply === "function" ? options.reply(touch, call) : options.reply ?? runs([]);
  const runtime: NativeRuntime = {
    async open(request) { opens.push(request); return { status: 0, revision: 1, runs: options.open ?? runs(options.asks ?? []) }; },
    async event(touch) {
      events.push(touch);
      calls += 1;
      return options.event === undefined ? { status: 0, revision: calls + 1, runs: answer(touch, calls) } : options.event(touch, calls);
    },
    hold(key, text) { holds.push([key, text]); },
    dispose() { disposed += 1; },
  };
  const deps: SessionDeps = {
    runtime,
    send: (message) => { sent.push(message); },
    execute: async (ask, slices) => {
      executed.push(ask);
      held.push(new Map(slices));
      return options.answer === undefined ? { detail: JSON.stringify({ ok: ask.ticket }) } : options.answer(ask, slices);
    },
    draw: (frameTold, regions, slices) => {
      drawn.push([...regions].map(([key, snapshot]) => ({ key, revision: snapshot.revision })));
      return options.draw === undefined ? '<div class="osp-root"></div>' : options.draw(frameTold, regions, slices);
    },
    assets: (sources) => Object.fromEntries(sources.map((source) => [source, `asset:${source}`])),
    report: (error) => { reports.push(error); },
  };
  return {
    session: createWorkspaceSession(deps), events, opens, sent, executed, held, holds, reports, drawn,
    disposals: () => disposed,
    busy: () => sent.filter((message): message is BusyMessage => message.type === "busy").map((message) => message.active),
    frames: () => sent.filter((message): message is FrameMessage | RenderMessage => message.type === "frame" || message.type === "render"),
    fatal: () => sent.filter((message): message is FatalMessage => message.type === "fatal").map((message) => message.message),
    completions: () => events.filter((event) => event.kind === 8),
  };
}

/** A promise and the function that settles it. */
export function gate<T>(): { readonly promise: Promise<T>; release(value: T): void; refuse(reason: unknown): void } {
  let release: (value: T) => void = () => undefined;
  let refuse: (reason: unknown) => void = () => undefined;
  const promise = new Promise<T>((resolve, reject) => { release = resolve; refuse = reject; });
  return { promise, release: (value) => release(value), refuse: (reason) => refuse(reason) };
}

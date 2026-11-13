// [NATIVE-SHM-DELIVERY] [NATIVE-SHM-ASK]: one serialized session owns the live instance, publishes
// every call's frame before answering its asks, and returns each completion as event kind 8.
import * as assert from "assert";
import { Ask, type Completion } from "../../client/src/osprey-ui/asks";
import { createWorkspaceSession, parseTouch, type FrameMessage, type SessionDeps } from "../../client/src/osprey-ui/session";
import type { FrameResult, NativeRuntime, NativeTouch, OpenOptions } from "../../client/src/osprey-ui/wasi-runner";
import { frame, type AskSpec } from "./native-rows-kit";

const runs = (asks: readonly AskSpec[], words = "Ready"): FrameResult["runs"] => {
  const built = frame({ views: [{ kind: 2, words }], asks });
  return { shapes: built.shapes, views: built.views, text: built.text, asks: built.asks };
};

const settle = (): Promise<void> => new Promise((resolve) => setImmediate(resolve));

function harness(options: { readonly asks?: AskSpec[]; readonly open?: FrameResult["runs"]; readonly answer?: (ask: Ask) => Promise<Completion> } = {}) {
  const events: NativeTouch[] = [];
  const opens: OpenOptions[] = [];
  const sent: (FrameMessage | { type: "busy"; active: boolean } | { type: "fatal"; message: string })[] = [];
  const executed: Ask[] = [];
  const held: ReadonlyMap<string, string>[] = [];
  let calls = 0;
  const runtime: NativeRuntime = {
    async open(request) { opens.push(request); return { status: 0, revision: 1, runs: options.open ?? runs(options.asks ?? []) }; },
    async event(touch) { events.push(touch); calls += 1; return { status: 0, revision: calls + 1, runs: runs([]) }; },
    dispose() { events.push({ kind: -1, ordinal: 0, words: "", detail: "", x: 0, y: 0, down: false, shift: false }); },
  };
  const deps: SessionDeps = {
    runtime,
    send: (message) => { sent.push(message as FrameMessage); },
    execute: async (ask, slices) => {
      executed.push(ask);
      held.push(slices);
      return options.answer === undefined ? { detail: JSON.stringify({ ok: ask.ticket }) } : options.answer(ask);
    },
    assets: (shown) => Object.fromEntries(shown.controls.filter((control) => control.kind === "image").map((control) => [control.words, `asset:${control.words}`])),
    report: () => undefined,
  };
  return { session: createWorkspaceSession(deps), events, opens, sent, executed, held };
}

const open: OpenOptions = { now: 1, offset: 0, platform: "vscode", home: "/home", dark: true, arguments: ["a", "b"] };

suite("native session", () => {
  test("start runs the live instance once with its typed context and publishes the frame", async () => {
    const { session, opens, sent } = harness();
    await session.start(open);
    assert.deepStrictEqual(opens, [open]);
    assert.deepStrictEqual(sent[0], { type: "busy", active: true });
    const published = sent.find((message): message is FrameMessage => message.type === "frame");
    assert.ok(published);
    assert.deepStrictEqual(published.diagnostics, []);
    assert.ok(published.views.byteLength > 0 && published.text.byteLength > 0);
    assert.strictEqual(sent[sent.length - 1]?.type, "busy");
  });

  test("a second start is ignored rather than opening another instance", async () => {
    const { session, opens } = harness();
    await session.start(open);
    await session.start(open);
    assert.strictEqual(opens.length, 1);
  });

  test("slice writes are kept for the host and handed to the request that reads them", async () => {
    const { session, executed, held } = harness({ asks: [
      { code: 100, ticket: "export", fields: [[1, 0, "<math>∧</math>"]] },
      { code: 15, ticket: "save", fields: [[1, 0, "proof.html"], [2, 0, "export"]] },
    ] });
    await session.start(open);
    await settle();
    assert.strictEqual(executed.length, 1);
    assert.strictEqual(held[0]?.get("export"), "<math>∧</math>");
  });

  test("every capability request is answered with a completion event carrying its ticket and detail", async () => {
    const { session, executed, events } = harness({ asks: [
      { code: 11, ticket: "sdk/open", fields: [[1, 0, "vscode"], [2, 0, "document.open"]] },
      { code: 13, ticket: "pick", fields: [[1, 0, "ospml"], [2, 10, ""], [3, 0, "document"]] },
    ], answer: async (ask) => ({ detail: ask.kind === "choose-file" ? "/tmp/chosen.ospml" : "{\"opened\":true}" }) });
    await session.start(open);
    await settle();
    await settle();
    assert.deepStrictEqual(executed.map((ask) => ask.kind), ["host-call", "choose-file"]);
    const completions = events.filter((event) => event.kind === 8);
    assert.deepStrictEqual(completions.map((event) => [event.words, event.detail, event.down]), [
      ["sdk/open", "{\"opened\":true}", true],
      ["pick", "/tmp/chosen.ospml", true],
    ]);
  });

  test("a failed capability completes as a refusal with the reason and still draws a frame", async () => {
    const { session, events, sent } = harness({ asks: [{ code: 10, ticket: "fetch", fields: [] }],
      answer: async () => { throw new Error("Unsupported native capability: http"); } });
    await session.start(open);
    await settle();
    await settle();
    const refused = events.find((event) => event.kind === 8);
    assert.deepStrictEqual([refused?.words, refused?.detail, refused?.down], ["fetch", "Unsupported native capability: http", false]);
    assert.ok(sent.filter((message) => message.type === "frame").length >= 2, "the refusal's frame is published");
  });

  test("a diagnostic ask shows the application's refusal reason", async () => {
    const { session, sent } = harness({ asks: [{ code: 101, ticket: "That control is not available" }] });
    await session.start(open);
    const published = sent.find((message): message is FrameMessage => message.type === "frame");
    assert.deepStrictEqual(published?.diagnostics, ["That control is not available"]);
  });

  test("image rows are resolved through the host's approved assets only", async () => {
    const built = frame({ views: [{ kind: 12, words: "media/logo.png" }, { kind: 2, words: "Text" }] });
    const { session, sent } = harness({ open: { shapes: built.shapes, views: built.views, text: built.text, asks: built.asks } });
    await session.start(open);
    const published = sent.find((message): message is FrameMessage => message.type === "frame");
    assert.deepStrictEqual(published?.assets, { "media/logo.png": "asset:media/logo.png" });
  });

  test("typed events are delivered in order and malformed events never reach the instance", async () => {
    const { session, events } = harness();
    await session.start(open);
    for (const malformed of [null, "event", { type: "event" }, { type: "event", kind: 99 },
      { type: "event", kind: 0, ordinal: -1 }, { type: "event", kind: 0, x: Number.NaN },
      { type: "event", kind: 0, words: 7 }, { type: "nope" }]) session.message(malformed);
    await settle();
    assert.strictEqual(events.length, 0);
    session.message({ type: "event", kind: 0, ordinal: 4, words: "text" });
    session.message({ type: "event", kind: 2, ordinal: 1, x: 0.5, y: 1.5, down: true, shift: true });
    await settle();
    await settle();
    assert.deepStrictEqual(events.map((event) => [event.kind, event.ordinal]), [[0, 4], [2, 1]]);
    assert.deepStrictEqual([events[1]?.x, events[1]?.y, events[1]?.down, events[1]?.shift], [0.5, 1.5, true, true]);
  });

  test("an appearance change reaches the instance after it opened, never before", async () => {
    const { session, events } = harness();
    session.appearance(true);
    await settle();
    assert.strictEqual(events.length, 0);
    await session.start(open);
    session.appearance(false);
    await settle();
    await settle();
    assert.deepStrictEqual(events.filter((event) => event.kind === 11).map((event) => event.down), [false]);
  });

  test("a frame whose asks are malformed is refused whole and reports a fatal reason", async () => {
    const built = frame({ views: [{ kind: 0 }] });
    const asks = new ArrayBuffer(32);
    const view = new DataView(asks);
    view.setBigInt64(0, 0n, true);
    view.setBigInt64(8, 999n, true);
    const { session, sent, events } = harness({ open: { shapes: built.shapes, views: built.views, text: built.text, asks } });
    await session.start(open);
    await settle();
    const fatal = sent.find((message) => message.type === "fatal");
    assert.ok(fatal && fatal.type === "fatal" && fatal.message.includes("unknown capability code"), JSON.stringify(sent));
    assert.ok(!sent.some((message) => message.type === "frame"), "no bad frame reaches the webview");
    assert.deepStrictEqual(events, []);
  });

  test("dispose terminates the instance and drops later calls", async () => {
    const { session, events, sent } = harness();
    await session.start(open);
    const before = sent.length;
    session.dispose();
    session.message({ type: "event", kind: 0, ordinal: 0 });
    session.appearance(true);
    await settle();
    assert.strictEqual(events[events.length - 1]?.kind, -1, "dispose reached the runtime");
    assert.strictEqual(events.filter((event) => event.kind >= 0).length, 0);
    assert.strictEqual(sent.length, before);
  });

  test("parseTouch defaults only fields the shared event leaves out", () => {
    assert.deepStrictEqual(parseTouch({ type: "event", kind: 1, ordinal: 2 }), {
      kind: 1, ordinal: 2, words: "", detail: "", x: 0, y: 0, down: false, shift: false });
    assert.strictEqual(parseTouch({ type: "event", kind: 12 }), undefined);
    assert.strictEqual(parseTouch({ type: "event", kind: 0, ordinal: 1.5 }), undefined);
  });
});

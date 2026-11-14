// [NATIVE-SHM-DELIVERY] [NATIVE-SHM-ASK] [NATIVE-SHM-EVENT]: the serialized session's guarantees. An
// edit is a typed control event; its acknowledgment is the frame its call draws; a capability's
// answer is a completion only the host can issue; disposal quarantines everything still in flight.
import * as assert from "assert";
import type { FrameResult } from "../../client/src/osprey-ui/wasi-runner";
import { gate, harness, open, region, runs, settle, settleAll, told } from "./native-session-kit";

const call = (ticket: string) => ({ code: 11, ticket, fields: [[1, 0, "vscode"], [2, 0, "workspace.snapshot"]] as const });
const edit = (ordinal: number, words: string) => ({ type: "event", kind: 0, ordinal, words });
const answered = (revision: number, words = "Ready"): FrameResult => ({ status: 0, revision, runs: runs([], words) });

suite("package workspace session", () => {
  test("a foreign executor result is delivered untouched even when it names a result member", async () => {
    const detail = "{\"result\":{\"result\":1},\"ok\":true,\"error\":\"not mine\"}";
    const { session, completions } = harness({ asks: [call("t")], answer: async () => ({ detail }) });
    await session.start(open);
    await settleAll();
    assert.deepStrictEqual(completions().map((event) => [event.words, event.detail, event.down]), [["t", detail, true]]);
  });

  test("a foreign executor result is delivered untouched even when it names a result member (extra: text that is not JSON is not reshaped)", async () => {
    const details = ["plain words", "{\"unterminated\":", "  padded  "];
    const { session, completions } = harness({ asks: details.map((_, index) => call(`t${index}`)), answer: async (ask) => ({ detail: details[Number(ask.ticket.slice(1))] ?? "" }) });
    await session.start(open);
    await settleAll(8);
    assert.deepStrictEqual(completions().map((event) => event.detail).sort(), [...details].sort());
  });

  test("a forged or stale control event never reaches the runtime", async () => {
    const { session, events } = harness();
    await session.start(open);
    for (const forged of [{ type: "event", kind: 12 }, { type: "events", kind: 0 }, { type: "event", kind: 0, ordinal: "1" }, { kind: 0 }]) session.message(forged);
    await settleAll();
    session.dispose();
    session.message(edit(0, "stale"));
    await settleAll();
    assert.deepStrictEqual(events, []);
  });

  test("a forged or stale control event never reaches the runtime (extra: unknown members are dropped, not forwarded)", async () => {
    const { session, events } = harness();
    await session.start(open);
    session.message({ ...edit(2, "p"), revision: 99, ticket: "forged", now: -1 });
    await settleAll();
    assert.deepStrictEqual(events, [{ kind: 0, ordinal: 2, words: "p", detail: "", x: 0, y: 0, down: false, shift: false }]);
  });

  test("a refused action preserves the actual queued busy state without executing", async () => {
    const pending = gate<FrameResult>();
    const { session, busy, events } = harness({ event: () => pending.promise });
    await session.start(open);
    session.message(edit(0, "slow"));
    session.message({ type: "event", kind: "refused" });
    assert.deepStrictEqual(busy(), [true, false, true, true], "the refusal repeats the real, still-busy state");
    pending.release(answered(2));
    await settleAll();
    assert.deepStrictEqual(busy().at(-1), false);
    assert.strictEqual(events.length, 1);
  });

  test("a refused action preserves the actual queued busy state without executing (extra: a refusal while idle reports idle)", async () => {
    const { session, busy, events, executed } = harness();
    await session.start(open);
    session.message(null);
    session.message({ type: "event", kind: 0, x: "left" });
    assert.deepStrictEqual(busy(), [true, false, false, false]);
    assert.deepStrictEqual([events, executed], [[], []]);
  });

  test("a rejected host capability is returned to Osprey as an explicit failed completion", async () => {
    const { session, completions, fatal } = harness({ asks: [call("denied")], answer: async () => { throw new Error("Unsupported host call: workspace.snapshot"); } });
    await session.start(open);
    await settleAll();
    assert.deepStrictEqual(completions().map((event) => [event.words, event.detail, event.down]), [["denied", "Unsupported host call: workspace.snapshot", false]]);
    assert.deepStrictEqual(fatal(), [], "a refused capability is the application's to handle, not a fatal host error");
  });

  test("a rejected host capability is returned to Osprey as an explicit failed completion (extra: a rejection that is not an Error keeps its words)", async () => {
    const { session, completions } = harness({ asks: [call("a"), call("b")], answer: (ask) => Promise.reject(ask.ticket === "a" ? "refused by policy" : 42) });
    await session.start(open);
    await settleAll(8);
    assert.deepStrictEqual(completions().map((event) => [event.words, event.detail, event.down]).sort(), [["a", "refused by policy", false], ["b", "42", false]]);
  });

  test("an edit from an unknown revision is explicitly refused without dispatch", async () => {
    const { session, events, busy } = harness();
    await session.start(open);
    for (const unknown of [edit(-1, "p"), edit(1.5, "p"), { ...edit(0, "p"), words: ["p"] }]) session.message(unknown);
    await settleAll();
    assert.deepStrictEqual(events, []);
    assert.deepStrictEqual(busy().slice(2), [false, false, false], "each refusal is answered explicitly");
  });

  test("an edit from an unknown revision is explicitly refused without dispatch (extra: an ordinal beyond exact integers is refused)", async () => {
    const { session, events } = harness();
    await session.start(open);
    session.message(edit(Number.MAX_SAFE_INTEGER + 2, "p"));
    session.message(edit(Number.MAX_SAFE_INTEGER, "p"));
    await settleAll();
    assert.deepStrictEqual(events.map((event) => event.ordinal), [Number.MAX_SAFE_INTEGER]);
  });

  test("an edit uses its issued snapshot and receives acknowledgment before the render", async () => {
    const order: string[] = [];
    const { session, sent } = harness({ event: async (touch, at) => { order.push(`runtime ${touch.words}`); return answered(at + 1, `after ${touch.words}`); } });
    await session.start(open);
    const before = sent.length;
    session.message(edit(3, "p → q"));
    await settleAll();
    const published = sent.slice(before).filter((message) => message.type === "frame");
    assert.deepStrictEqual(order, ["runtime p → q"]);
    assert.strictEqual(published.length, 1, "the edit's own frame acknowledges it");
    assert.ok(Buffer.from(published[0]?.type === "frame" ? published[0].text : new ArrayBuffer(0)).toString().includes("after p → q"));
  });

  test("an edit uses its issued snapshot and receives acknowledgment before the render (extra: consecutive edits are acknowledged in order)", async () => {
    const { session, frames } = harness({ event: async (touch, at) => answered(at + 1, `after ${touch.words}`) });
    await session.start(open);
    for (const words of ["p", "p ∧", "p ∧ q"]) session.message(edit(1, words));
    await settleAll(8);
    const texts = frames().slice(1).map((message) => Buffer.from(message.text).toString().replace(/\0/g, ""));
    assert.deepStrictEqual(texts.map((text) => text.match(/after [^\0]*/)?.[0]), ["after p", "after p ∧", "after p ∧ q"]);
  });

  test("busy remains set until every queued runtime frame has finished", async () => {
    const first = gate<FrameResult>();
    const second = gate<FrameResult>();
    const { session, busy } = harness({ event: (_touch, at) => (at === 1 ? first.promise : second.promise) });
    await session.start(open);
    session.message(edit(0, "a"));
    session.message(edit(0, "b"));
    first.release(answered(2));
    await settleAll();
    assert.deepStrictEqual(busy(), [true, false, true], "one finished call leaves the other queued");
    second.release(answered(3));
    await settleAll();
    assert.deepStrictEqual(busy(), [true, false, true, false]);
  });

  test("busy remains set until every queued runtime frame has finished (extra: a later burst is busy again)", async () => {
    const { session, busy } = harness();
    await session.start(open);
    for (let burst = 0; burst < 2; burst += 1) {
      for (let at = 0; at < 3; at += 1) session.message(edit(at, "x"));
      await settleAll(8);
    }
    assert.deepStrictEqual(busy(), [true, false, true, false, true, false]);
  });

  test("capability executions receive host-held slices and their deliveries ride the next frame", async () => {
    const order: string[] = [];
    const { session, held, holds, events } = harness({
      asks: [{ code: 100, ticket: "draft", fields: [[1, 0, "∀p"]] }, { code: 15, ticket: "save", fields: [[1, 0, "proof.txt"], [2, 0, "draft"]] }],
      answer: async () => { order.push("executed"); return { detail: "", slices: [{ key: "receipt", text: "saved é" }] }; },
    });
    await session.start(open);
    await settleAll();
    assert.strictEqual(held[0]?.get("draft"), "∀p");
    assert.deepStrictEqual(holds, [["draft", "∀p"], ["receipt", "saved é"]]);
    assert.deepStrictEqual(events.map((event) => [event.kind, event.words]), [[8, "save"]], "the delivery is held before the completion that announces it");
    assert.deepStrictEqual(order, ["executed"]);
  });

  test("capability executions receive host-held slices and their deliveries ride the next frame (extra: every delivered slice is held in order)", async () => {
    const delivered = [{ key: "a", text: "1" }, { key: "b", text: "" }, { key: "a", text: "3 →" }];
    const { session, holds } = harness({ asks: [{ code: 6, ticket: "read", fields: [[1, 0, "k"]] }], answer: async () => ({ detail: "", slices: delivered }) });
    await session.start(open);
    await settleAll();
    assert.deepStrictEqual(holds, delivered.map((write) => [write.key, write.text]));
  });

  test("disposal quarantines a pending runtime response before it can render or execute capabilities", async () => {
    const pending = gate<FrameResult>();
    const { session, executed, frames } = harness({ event: () => pending.promise });
    await session.start(open);
    const drawn = frames().length;
    session.message(edit(0, "x"));
    await settle();
    session.dispose();
    pending.release({ status: 0, revision: 2, runs: runs([call("late")]) });
    await settleAll();
    assert.deepStrictEqual([frames().length, executed], [drawn, []]);
  });

  test("disposal quarantines a pending runtime response before it can render or execute capabilities (extra: a pending open is quarantined too)", async () => {
    const { session, executed, frames, reports } = harness({ asks: [call("never")] });
    const started = session.start(open);
    session.dispose();
    await started;
    await settleAll();
    assert.deepStrictEqual([frames(), executed, reports], [[], [], []]);
  });

  test("expired edits are refused and a live edit cannot substitute its prior state", async () => {
    const { session, events } = harness();
    await session.start(open);
    session.message(edit(1, "p"));
    session.message(edit(1, "q"));
    await settleAll();
    session.dispose();
    session.message(edit(1, "expired"));
    await settleAll();
    assert.deepStrictEqual(events.map((event) => event.words), ["p", "q"], "each edit carries its own words, never its predecessor's");
  });

  test("expired edits are refused and a live edit cannot substitute its prior state (extra: an empty edit is delivered as empty)", async () => {
    const { session, events } = harness();
    await session.start(open);
    session.message(edit(1, "p"));
    session.message({ type: "event", kind: 0, ordinal: 1 });
    await settleAll();
    assert.deepStrictEqual(events.map((event) => event.words), ["p", ""]);
  });

  test("failed runtime commands report their exit code and available diagnostic output", async () => {
    const { session, fatal, reports } = harness({ event: async () => { throw new Error("native application exited with status 3: wasm trap: unreachable"); } });
    await session.start(open);
    session.message(edit(0, "x"));
    await settleAll();
    assert.deepStrictEqual(fatal(), ["native application exited with status 3: wasm trap: unreachable"]);
    assert.deepStrictEqual(reports.map((error) => error.message), fatal());
  });

  test("failed runtime commands report their exit code and available diagnostic output (extra: the next event still reaches the runtime)", async () => {
    const { session, events, fatal, frames } = harness({ event: async (touch, at) => { if (at === 1) throw new Error("status 1"); return answered(at + 1, touch.words); } });
    await session.start(open);
    session.message(edit(0, "fails"));
    session.message(edit(0, "recovers"));
    await settleAll(8);
    assert.deepStrictEqual([events.map((event) => event.words), fatal()], [["fails", "recovers"], ["status 1"]]);
    assert.strictEqual(frames().length, 2, "the open frame and the recovered frame");
  });

  test("invalid edit sequences cannot execute or acknowledge a forged edit", async () => {
    const { session, events, executed } = harness();
    await session.start(open);
    // Completions, phases, frame ticks and appearance changes are the host's to issue: a webview
    // message naming one is forged.
    for (const kind of [8, 9, 10, 11]) session.message({ type: "event", kind, words: "ticket", detail: "{\"ok\":true}", down: true });
    await settleAll();
    assert.deepStrictEqual([events, executed], [[], []]);
  });

  test("invalid edit sequences cannot execute or acknowledge a forged edit (extra: every webview event kind is still delivered)", async () => {
    const { session, events } = harness();
    await session.start(open);
    for (let kind = 0; kind <= 7; kind += 1) session.message({ type: "event", kind, ordinal: kind });
    await settleAll(12);
    assert.deepStrictEqual(events.map((event) => event.kind), [0, 1, 2, 3, 4, 5, 6, 7]);
  });

  test("retired file deliveries cannot enter a restarted session", async () => {
    const pending = gate<{ detail: string; slices: { key: string; text: string }[] }>();
    const { session, holds } = harness({ asks: [{ code: 13, ticket: "pick", fields: [[1, 0, "ospml"], [2, 10, ""]] }], answer: () => pending.promise });
    await session.start(open);
    await settle();
    session.dispose();
    pending.release({ detail: "/tmp/old.ospml", slices: [{ key: "file", text: "retired" }] });
    await settleAll();
    assert.deepStrictEqual(holds, []);
    await assert.rejects(session.start(open), /disposed/);
  });

  test("retired file deliveries cannot enter a restarted session (extra: a second start keeps the first session's slices)", async () => {
    const { session, holds, opens } = harness({ asks: [{ code: 100, ticket: "file", fields: [[1, 0, "kept"]] }] });
    await session.start(open);
    await session.start({ ...open, arguments: ["other"] });
    assert.deepStrictEqual([opens.length, holds], [1, [["file", "kept"]]]);
  });

  test("runtime and rendering rejections remain visible even when they are not Error objects", async () => {
    const { session, fatal, reports } = harness({ open: { ...runs([]) }, event: () => Promise.reject("trap as text"), draw: () => { throw new Error("unused"); } });
    await session.start(open);
    session.message(edit(0, "x"));
    await settleAll();
    assert.deepStrictEqual(fatal(), ["trap as text"]);
    assert.ok(reports[0] instanceof Error && reports[0].message === "trap as text");
  });

  test("runtime and rendering rejections remain visible even when they are not Error objects (extra: a renderer throwing a value is fatal)", async () => {
    const { session, fatal, reports, completions } = harness({ open: told(region("root", "x"), [call("never")]), draw: () => { throw { reason: "bad node" }; } });
    await session.start(open);
    await settleAll();
    assert.deepStrictEqual(fatal(), ["[object Object]"]);
    assert.strictEqual(reports.length, 1);
    assert.deepStrictEqual(completions(), [], "a frame that could not be drawn answers none of its asks");
  });

  test("runtime diagnostics explicitly refuse an edit before publishing the next screen", async () => {
    const { session, frames } = harness({ reply: () => runs([{ code: 101, ticket: "That proof line is not available" }], "Unchanged") });
    await session.start(open);
    session.message(edit(0, "forbidden"));
    await settleAll();
    assert.deepStrictEqual(frames().map((message) => message.diagnostics), [[], ["That proof line is not available"]]);
  });

  test("runtime diagnostics explicitly refuse an edit before publishing the next screen (extra: the refusal is cleared by the next accepted edit)", async () => {
    const { session, frames } = harness({ reply: (_touch, at) => runs(at === 1 ? [{ code: 101, ticket: "refused" }] : []) });
    await session.start(open);
    session.message(edit(0, "bad"));
    session.message(edit(0, "good"));
    await settleAll(8);
    assert.deepStrictEqual(frames().map((message) => message.diagnostics), [[], ["refused"], []]);
  });

  test("start applies keyed regions and executes only requests from the envelope", async () => {
    const { session, executed, holds, frames } = harness({ asks: [{ code: 100, ticket: "s", fields: [[1, 0, "t"]] }, { code: 101, ticket: "notice" }, call("only")] });
    await session.start(open);
    await settleAll();
    assert.deepStrictEqual(executed.map((ask) => ask.ticket), ["only"]);
    assert.deepStrictEqual(holds, [["s", "t"]]);
    assert.deepStrictEqual(frames()[0]?.diagnostics, ["notice"]);
  });

  test("start applies keyed regions and executes only requests from the envelope (extra: requests run in the order the frame asked)", async () => {
    const { session, executed } = harness({ asks: [call("first"), { code: 6, ticket: "second", fields: [[1, 0, "k"]] }, call("third")] });
    await session.start(open);
    await settleAll(8);
    assert.deepStrictEqual(executed.map((ask) => ask.ticket), ["first", "second", "third"]);
  });
});

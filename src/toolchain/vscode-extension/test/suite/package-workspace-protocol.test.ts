// [NATIVE-SHM-NODES] [NATIVE-SHM-ASK] [NATIVE-SHM-EVENT]: the wire boundary of the shared frame. The
// JSON envelope these cases first guarded became four checked runs; each case now guards the same
// promise there: valid frames keep their regions, asks and slices, and anything malformed is refused.
import * as assert from "assert";
import { readAsks, requests, sliceWrites, diagnostics, Ask } from "../../client/src/osprey-ui/asks";
import { RegionStore, toldFrame } from "../../client/src/osprey-ui/nodes";
import { parseTouch } from "../../client/src/osprey-ui/session";
import { readFrame } from "../../client/src/osprey-ui/webview/rows";
import { SCENE_SHAPE, SCENE_SHAPE_BYTES } from "../../client/src/osprey-ui/generated/native-abi";
import { frame, type AskSpec, type SceneSpec } from "./native-rows-kit";
import { gate, harness, open, region, runs, settle, settleAll, themed, told } from "./native-session-kit";

const asks: AskSpec[] = [
  { code: 100, ticket: "s", fields: [[1, 0, "t"]] },
  { code: 11, ticket: "ticket", fields: [[1, 0, "vscode"], [2, 0, "workspace.snapshot"]] },
  { code: 101, ticket: "notice" },
];

const read = (views = region("root", "Hi"), with_: AskSpec[] = asks) => {
  const built = frame({ views, asks: with_ });
  return { shown: readFrame(built), asked: readAsks(built.asks, built.text) };
};

const regionsOf = (store: RegionStore, views: ReturnType<typeof region>) =>
  [...store.adopt(toldFrame(readFrame(frame({ views })))!)].map(([key, cell]) => [key, cell.revision]);

/** A told tree as plain data: told records have no prototype. */
const plain = (value: unknown): unknown => JSON.parse(JSON.stringify(value));

const truncated = (spec: AskSpec[], cut: number): ArrayBuffer => frame({ asks: spec }).asks.slice(0, cut);

suite("package workspace protocol", () => {
  test("a final scene strip retains every point within its declared count", () => {
    for (const count of [0, 1, 2, 3, 8]) {
      const points = Array.from({ length: count }, (_, index) => ({ x: index + 0.5, y: -index }));
      const built = frame({ views: [{ kind: 13 }], scenes: [
        { code: 20, index: 0, x: 100, y: 100, width: 1 },
        { code: 2, index: count, width: 3, colour: 0xff00ffff, depth: 2 },
        ...points.map((point) => ({ code: 7, ...point })),
      ] });
      assert.deepStrictEqual(readFrame(built).scenes.get(0)?.shapes, [
        { shape: "strip", points, colour: 0xff00ffff, stroke: 3, depth: 2 },
      ], `strip with ${count} points`);
    }
  });

  test("scene paths retain cubic controls, gradient spaces, dashes and transforms", () => {
    const marker = { code: 20, index: 0, x: 100, y: 100, width: 1 };
    const built = frame({ views: [{ kind: 13 }], scenes: [marker,
      { code: 5, index: 2, x: 2, width: 1 }, { code: 8, index: 1, x: 2, y: 3 },
      { code: 8, index: 3, x: 4, y: 5, width: 6, height: 7, angle: 8 }, { code: 7, x: 9 },
      { code: 9, index: 0 }, { code: 10, index: 1, x: 45, y: 0.2, width: 0.8, colour: 1 },
      { code: 11, index: 1, x: 0.3, y: 0.4, width: 0.5, height: 0.6 },
      { code: 11, index: 0, x: 0.1, y: 0.2, width: 0.3, height: 0.4 },
    ] });
    const path = readFrame(built).scenes.get(0)?.shapes[0];
    assert.ok(path?.shape === "path");
    assert.deepStrictEqual(path.steps, [{ step: "line", to: { x: 2, y: 3 } },
      { step: "cubic", to: { x: 4, y: 5 }, first: { x: 6, y: 7 }, second: { x: 8, y: 9 } }]);
    assert.deepStrictEqual(path.outline, { outline: "fill" });
    assert.deepStrictEqual(path.paint, { paint: "linear", angle: 45,
      from: [{ space: "hsl", channels: [0.3, 0.4, 0.5, 0.6] }, 0.2],
      to: [{ space: "rgb", channels: [0.1, 0.2, 0.3, 0.4] }, 0.8], oklab: true });
    assert.deepStrictEqual(path.transform, { x: 2, y: 0, scale: 1 });
    for (const [x, y, scale] of [[0, 0, 1], [0, 2, 1], [0, 0, 2]]) {
      const result = readFrame(frame({ views: [{ kind: 13 }], scenes: [marker,
        { code: 5, index: 0, x, y, width: scale }, { code: 9, index: 1, x: 3, depth: 2, colour: 1 },
        { code: 7, x: 0.5 }, { code: 7, x: 2 }, { code: 10 }, { code: 11 },
      ] })).scenes.get(0)?.shapes[0];
      assert.ok(result?.shape === "path");
      assert.deepStrictEqual(result.outline, { outline: "stroke", width: 3, bevel: true, dashes: [0.5, 2] });
      assert.deepStrictEqual(result.transform, x || y || scale !== 1 ? { x, y, scale } : null);
    }
  });

  test("scene row counts and discriminants reject truncation before decoding payloads", () => {
    const marker = { code: 20, index: 0, x: 100, y: 100, width: 1 };
    const malformed: readonly [readonly SceneSpec[], RegExp][] = [
      [[{ code: 2, index: -1 }], /count that runs past/],
      [[{ code: 2, index: 2 }, { code: 7 }], /count that runs past/],
      [[{ code: 2, index: 1 }, { code: 9 }], /should have code 7/],
      [[{ code: 5, index: -1 }], /count that runs past/],
      [[{ code: 5, index: 0x80000000 }], /count that runs past/],
      [[{ code: 5, index: 1 }, { code: 8, index: 7 }], /unknown path step/],
      [[{ code: 5, index: 1 }, { code: 8, index: 3 }], /should have code 7/],
      [[{ code: 5 }, { code: 9, index: 3 }], /unknown outline/],
      [[{ code: 5 }, { code: 9 }, { code: 10, index: 2 }], /unknown paint/],
      [[{ code: 5 }, { code: 9 }, { code: 10 }, { code: 11, index: 2 }], /unknown tint/],
    ];
    for (const [scenes, error] of malformed) {
      assert.throws(() => readFrame(frame({ views: [{ kind: 13 }], scenes: [marker, ...scenes] })), error);
    }
    const complete = frame({ views: [{ kind: 13 }], scenes: [marker] });
    assert.throws(() => readFrame({ ...complete, shapes: complete.shapes.slice(0, -1) }), /scene run is not a whole number/);
    for (const value of [BigInt(Number.MAX_SAFE_INTEGER) + 1n, BigInt(Number.MIN_SAFE_INTEGER) - 1n]) {
      const changed = { ...complete, shapes: complete.shapes.slice(0) };
      new DataView(changed.shapes).setBigInt64(SCENE_SHAPE.index, value, true);
      assert.throws(() => readFrame(changed), /integer outside the range/);
    }
    for (const field of [SCENE_SHAPE.textOffset, SCENE_SHAPE.textLength]) {
      const text = frame({ views: [{ kind: 13 }], scenes: [marker, { code: 4, text: "∧" }] });
      new DataView(text.shapes).setBigInt64(SCENE_SHAPE_BYTES + field, -1n, true);
      assert.throws(() => readFrame(text), /text (offset|length) below zero/);
    }
  });

  test("scene sprites, glyphs and accessibility rows preserve content and hide decorative entries", () => {
    const access: SceneSpec[] = [0, 1, 2, 3, 4, 0].flatMap((role, index) => [
      { code: 22, index: role, x: 10, y: 20, width: 30, height: 40, angle: 0.5,
        colour: role === 3 ? 1 : 0, text: index === 5 ? "" : `label${role}` },
      { code: 7, x: 0, y: 1 },
    ]);
    const scene = readFrame(frame({ views: [{ kind: 13 }], scenes: [
      { code: 20, index: 0, x: 100, y: 100, width: 1 }, { code: 21, text: "atlas/sprite.png" },
      { code: 3, index: 0, x: 1, y: 2, width: 3, height: 4, angle: 5, colour: 0xffffffff, depth: 6 },
      { code: 4, text: "∧ café", x: 7, y: 8, width: 9, colour: 0xff00ffff, depth: 10 }, ...access,
    ] })).scenes.get(0);
    assert.deepStrictEqual(scene?.atlas, ["atlas/sprite.png"]);
    assert.deepStrictEqual(scene?.shapes, [
      { shape: "sprite", atlas: 0, x: 1, y: 2, w: 3, h: 4, angle: 5, tint: 0xffffffff, depth: 6 },
      { shape: "glyphs", text: "∧ café", x: 7, y: 8, size: 9, colour: 0xff00ffff, depth: 10 },
    ]);
    assert.deepStrictEqual(scene?.access, ["button", "slider", "button", "menuitem"].map((role, index) => ({
      role, label: `label${index}`, x: 10, y: 20, w: 30, h: 40, value: 0.5, min: 0, max: 1, selected: index === 3,
    })));
  });

  test("a valid envelope keeps its opaque state, keyed patches and requests", () => {
    const { shown, asked } = read();
    const told = toldFrame(shown);
    assert.deepStrictEqual(told?.regions.map((cell) => cell.key), ["root"]);
    assert.deepStrictEqual(plain(told?.regions[0]?.node), { kind: "text", text: "Hi" });
    assert.deepStrictEqual(requests(asked).map((ask) => [ask.ticket, ask.kind, ask.text(2)]), [["ticket", "host-call", "workspace.snapshot"]]);
    assert.deepStrictEqual(diagnostics(asked), ["notice"]);
    assert.deepStrictEqual(sliceWrites(asked), [{ key: "s", text: "t" }]);
  });

  test("a valid envelope keeps its opaque state, keyed patches and requests (extra: several regions and asks keep their order)", () => {
    const views = [...region("a", "first"), ...region("b", "second").map((spec) => ({ ...spec, parent: spec.parent === undefined ? undefined : spec.parent + 5 }))];
    const { shown, asked } = read(views, [{ code: 6, ticket: "one", fields: [[1, 0, "k"]] }, { code: 13, ticket: "two", fields: [[1, 0, "ospml"], [2, 10, ""]] }]);
    assert.deepStrictEqual(toldFrame(shown)?.regions.map((cell) => [cell.key, (cell.node as { text: string }).text]), [["a", "first"], ["b", "second"]]);
    assert.deepStrictEqual(requests(asked).map((ask) => [ask.ticket, ask.kind]), [["one", "store-read"], ["two", "choose-file"]]);
    assert.strictEqual(requests(asked)[1]?.number(2), 10);
    assert.deepStrictEqual(diagnostics(asked), []);
  });

  test("keyed patches replace only their region and removals drop theirs", () => {
    const store = new RegionStore();
    const both = [...region("root", "Hi"), ...region("details", "Detail").map((spec) => ({ ...spec, parent: spec.parent === undefined ? undefined : spec.parent + 5 }))];
    assert.deepStrictEqual(regionsOf(store, both), [["root", 0], ["details", 0]]);
    assert.deepStrictEqual(regionsOf(store, region("details", "Changed")), [["details", 1]]);
  });

  test("keyed patches replace only their region and removals drop theirs (extra: a returning region starts again at revision zero)", () => {
    const store = new RegionStore();
    assert.deepStrictEqual(regionsOf(store, region("root", "Hi")), [["root", 0]]);
    assert.deepStrictEqual(regionsOf(store, region("other", "x")), [["other", 0]]);
    assert.deepStrictEqual(regionsOf(store, region("root", "Hi")), [["root", 0]]);
    store.clear();
    assert.deepStrictEqual(regionsOf(store, region("root", "Again")), [["root", 0]]);
  });

  test("a wrong protocol version or malformed request is rejected, never guessed", () => {
    const text = frame({ asks }).text;
    assert.throws(() => readAsks(frame({ asks: [{ code: 999, ticket: "x" }] }).asks, text), /unknown capability code 999/);
    assert.throws(() => readAsks(frame({ asks: [{ code: 0, ticket: "x" }] }).asks, text), /unknown capability code 0/);
    assert.throws(() => readAsks(truncated(asks, 8), text), /whole number of rows/);
    const orphan = frame({ asks: [{ code: 11, ticket: "t", fields: [[1, 0, "x"]] }] });
    assert.throws(() => readAsks(orphan.asks.slice(orphan.asks.byteLength / 2), orphan.text), /field before any ask/);
  });

  test("a wrong protocol version or malformed request is rejected, never guessed (extra: a known request with malformed arguments refuses its read)", () => {
    const [headers] = readAsks(frame({ asks: [{ code: 10, ticket: "h", fields: [[3, 0, "Accept"]] }] }).asks, frame({ asks: [{ code: 10, ticket: "h", fields: [[3, 0, "Accept"]] }] }).text);
    assert.throws(() => headers?.pairs(3), /name and value pairs/);
    assert.throws(() => new Ask(77, "t", []).kind, /Unknown capability code 77/);
    assert.strictEqual(new Ask(26, "t", []).ticket, "t");
    assert.throws(() => new Ask(26, "t", []).kind, /Unknown capability code 26/);
  });

  test("host-held slices are validated and never silently discarded", () => {
    const writes: AskSpec[] = [{ code: 100, ticket: "a", fields: [[1, 0, "b"]] }, { code: 100, ticket: "é→", fields: [[1, 0, ""]] }, { code: 100, ticket: "a", fields: [[1, 0, "c"]] }];
    const built = frame({ asks: writes });
    assert.deepStrictEqual(sliceWrites(readAsks(built.asks, built.text)), [{ key: "a", text: "b" }, { key: "é→", text: "" }, { key: "a", text: "c" }]);
    assert.throws(() => readAsks(built.asks, new ArrayBuffer(0)), /text range outside the frame/);
  });

  test("host-held slices are validated and never silently discarded (extra: the last write of a key is what a request reads)", async () => {
    const { session, held, holds } = harness({ asks: [
      { code: 100, ticket: "doc", fields: [[1, 0, "first"]] }, { code: 100, ticket: "doc", fields: [[1, 0, "second ∧"]] },
      { code: 15, ticket: "save", fields: [[1, 0, "proof.txt"], [2, 0, "doc"]] },
    ] });
    await session.start(open);
    await settle();
    assert.deepStrictEqual(holds, [["doc", "first"], ["doc", "second ∧"]]);
    assert.strictEqual(held[0]?.get("doc"), "second ∧");
  });

  test("malformed envelopes fail explicitly at the wire boundary", () => {
    const good = frame({ views: region("root", "Hi") });
    assert.throws(() => readFrame({ ...good, views: good.views.slice(0, good.views.byteLength - 1) }), /whole number of rows/);
    assert.throws(() => readFrame({ ...good, text: new ArrayBuffer(0) }), /text range outside the frame/);
    assert.throws(() => readFrame(frame({ views: [{ kind: 999 }] })), /unknown kind 999/);
    assert.throws(() => readFrame(frame({ views: [{ kind: 34 }, { kind: 33, parent: 5, hint: "x" }] })), /parent that is not an earlier row/);
    assert.throws(() => readFrame(frame({ views: [{ kind: 33, hint: "x", weight: 9 }] })), /unknown type 9/);
  });

  test("malformed envelopes fail explicitly at the wire boundary (extra: a malformed frame from the runtime is fatal and never drawn)", async () => {
    const bad = frame({ views: [{ kind: 999 }] });
    const { session, sent, executed, fatal } = harness({ open: { ...bad }, asks: [] });
    await session.start(open);
    await settle();
    assert.ok(fatal()[0]?.includes("unknown kind 999"), JSON.stringify(sent));
    assert.ok(!sent.some((message) => message.type === "frame" || message.type === "render"));
    assert.deepStrictEqual(executed, []);
  });

  test("optional wire fields default safely and diagnostic output does not hide the envelope", async () => {
    assert.deepStrictEqual(parseTouch({ type: "event", kind: 0 }), { kind: 0, ordinal: 0, words: "", detail: "", x: 0, y: 0, down: false, shift: false });
    const { session, frames } = harness({ asks: [{ code: 101, ticket: "runtime log" }, { code: 101, ticket: "trailing diagnostic" }] });
    await session.start(open);
    const [published] = frames();
    assert.deepStrictEqual(published?.diagnostics, ["runtime log", "trailing diagnostic"]);
    assert.ok((published?.views.byteLength ?? 0) > 0, "the frame is published beside its diagnostics");
  });

  test("optional wire fields default safely and diagnostic output does not hide the envelope (extra: explicit fields are kept exactly)", () => {
    assert.deepStrictEqual(parseTouch({ type: "event", kind: 4, ordinal: 3, words: "é", detail: "→", x: -2.5, y: 1e6, down: true, shift: true }),
      { kind: 4, ordinal: 3, words: "é", detail: "→", x: -2.5, y: 1e6, down: true, shift: true });
    assert.deepStrictEqual(parseTouch({ type: "event", kind: 7, down: false }), { kind: 7, ordinal: 0, words: "", detail: "", x: 0, y: 0, down: false, shift: false });
    assert.strictEqual(parseTouch({ type: "event", kind: 11, down: false }), undefined, "an appearance change is the host's to tell");
  });

  test("malformed control addresses and messages cannot dispatch", async () => {
    const { session, events } = harness();
    await session.start(open);
    for (const message of [null, [], { type: "other" }, { type: "event", kind: "0" }, { type: "event", kind: -1 },
      { type: "event", kind: 0, ordinal: 0.5 }, { type: "event", kind: 0, ordinal: Number.MAX_SAFE_INTEGER + 1 },
      { type: "event", kind: 0, detail: 1 }, { type: "event", kind: 0, y: Number.POSITIVE_INFINITY }, { type: "event", kind: 0, down: "yes" },
    ]) session.message(message);
    await settleAll();
    assert.deepStrictEqual(events, []);
  });

  test("malformed control addresses and messages cannot dispatch (extra: a refused message between valid ones keeps their order)", async () => {
    const { session, events } = harness();
    await session.start(open);
    session.message({ type: "event", kind: 0, ordinal: 1, words: "a" });
    session.message({ type: "event", kind: 0, ordinal: -1, words: "forged" });
    session.message({ type: "event", kind: 1, ordinal: 2, words: "b" });
    await settleAll();
    assert.deepStrictEqual(events.map((event) => [event.kind, event.ordinal, event.words]), [[0, 1, "a"], [1, 2, "b"]]);
  });

  test("only a current region revision produces a control event", async () => {
    const { session, drawn, events } = harness({ open: told(region("root", "One")), reply: (_touch, call) => told(region("root", call < 2 ? "One" : "Two")) });
    await session.start(open);
    session.message({ type: "event", kind: 0, ordinal: 1, words: "query" });
    await settleAll();
    session.message({ type: "event", kind: 0, ordinal: 1, words: "query" });
    await settleAll();
    assert.deepStrictEqual(drawn, [[{ key: "root", revision: 0 }], [{ key: "root", revision: 0 }], [{ key: "root", revision: 1 }]]);
    assert.deepStrictEqual(events.map((event) => [event.ordinal, event.words]), [[1, "query"], [1, "query"]]);
  });

  test("only a current region revision produces a control event (extra: a region's revision advances once per change)", () => {
    const store = new RegionStore();
    const revisions = ["a", "a", "b", "b", "c", "a"].map((text) => regionsOf(store, region("root", text))[0]?.[1]);
    assert.deepStrictEqual(revisions, [0, 0, 1, 1, 2, 3]);
  });

  test("dispatch and completion frames keep the state opaque", async () => {
    const { session, events, completions } = harness({ asks: [{ code: 11, ticket: "t", fields: [[1, 0, "vscode"], [2, 0, "x"]] }],
      answer: async () => ({ detail: "{\"ok\":false,\"error\":\"no\"}" }) });
    await session.start(open);
    session.message({ type: "event", kind: 0, ordinal: 1, words: "x", detail: "{\"opaque\":1}" });
    await settleAll();
    assert.deepStrictEqual(completions().map((event) => [event.words, event.detail, event.down]), [["t", "{\"ok\":false,\"error\":\"no\"}", true]]);
    const dispatched = events.find((event) => event.kind === 0);
    assert.deepStrictEqual(dispatched, { kind: 0, ordinal: 1, words: "x", detail: "{\"opaque\":1}", x: 0, y: 0, down: false, shift: false });
  });

  test("dispatch and completion frames keep the state opaque (extra: unicode and empty results reach the runtime byte for byte)", async () => {
    const details = ["", "é→∀\n", "x".repeat(4096)];
    const { session, completions } = harness({ asks: details.map((_, index) => ({ code: 11, ticket: `t${index}`, fields: [[1, 0, "vscode"], [2, 0, "x"]] as const })),
      answer: async (ask) => ({ detail: details[Number(ask.ticket.slice(1))] ?? "missing" }) });
    await session.start(open);
    await settleAll(8);
    assert.deepStrictEqual(completions().map((event) => [event.words, event.detail]).sort(), details.map((detail, index) => [`t${index}`, detail]));
  });

  test("a completion from a superseded session never reaches the runtime", async () => {
    const pending = gate<{ detail: string }>();
    const { session, completions, executed } = harness({ asks: [{ code: 11, ticket: "old", fields: [[1, 0, "vscode"], [2, 0, "x"]] }], answer: () => pending.promise });
    await session.start(open);
    await settle();
    assert.strictEqual(executed.length, 1);
    session.dispose();
    pending.release({ detail: "late" });
    await settleAll();
    assert.deepStrictEqual(completions(), []);
  });

  test("a completion from a superseded session never reaches the runtime (extra: a late failure is not reported either)", async () => {
    const pending = gate<{ detail: string }>();
    const { session, completions, reports, fatal } = harness({ asks: [{ code: 11, ticket: "old", fields: [[1, 0, "vscode"], [2, 0, "x"]] }], answer: () => pending.promise });
    await session.start(open);
    await settle();
    session.dispose();
    pending.refuse(new Error("gone"));
    await settleAll();
    assert.deepStrictEqual([completions(), reports, fatal()], [[], [], []]);
  });

  test("disposal stops later events and completions", async () => {
    const { session, events, sent, disposals } = harness({ asks: [{ code: 11, ticket: "t", fields: [[1, 0, "vscode"], [2, 0, "x"]] }] });
    await session.start(open);
    await settleAll();
    session.dispose();
    const [reached, before] = [events.length, sent.length];
    session.message({ type: "event", kind: 0, ordinal: 0 });
    session.appearance(false);
    await settleAll();
    assert.deepStrictEqual([events.length, sent.length, disposals()], [reached, before, 1]);
    assert.deepStrictEqual(events.map((event) => event.kind), [8], "only the completion answered before disposal reached the runtime");
    await assert.rejects(session.start(open), /disposed/);
  });

  test("disposal stops later events and completions (extra: disposing twice releases the instance once)", async () => {
    const { session, disposals, sent } = harness();
    await session.start(open);
    session.dispose();
    session.dispose();
    assert.strictEqual(disposals(), 1);
    assert.ok(!sent.some((message) => message.type === "fatal"));
  });

  test("host-held slices survive frames and are passed to the runtime", async () => {
    const { session, held, holds } = harness({ asks: [{ code: 100, ticket: "keep", fields: [[1, 0, "kept ∧"]] }],
      reply: (_touch, call) => runs(call === 2 ? [{ code: 7, ticket: "write", fields: [[1, 0, "k"], [2, 0, "keep"]] }] : []) });
    await session.start(open);
    for (let at = 0; at < 2; at += 1) {
      session.message({ type: "event", kind: 0, ordinal: at });
      await settleAll();
    }
    assert.deepStrictEqual(holds, [["keep", "kept ∧"]]);
    assert.strictEqual(held[0]?.get("keep"), "kept ∧", "a slice written two frames earlier still reaches the request");
  });

  test("host-held slices survive frames and are passed to the runtime (extra: an adapter's delivered slice is held for later requests)", async () => {
    const { session, held, holds } = harness({ asks: [{ code: 6, ticket: "read", fields: [[1, 0, "k"]] }],
      answer: async (ask) => (ask.ticket === "read" ? { detail: "", slices: [{ key: "read", text: "stored é" }] } : { detail: "" }),
      reply: (_touch, call) => runs(call === 1 ? [{ code: 7, ticket: "again", fields: [[1, 0, "k"], [2, 0, "read"]] }] : []) });
    await session.start(open);
    await settleAll(8);
    assert.deepStrictEqual(holds, [["read", "stored é"]]);
    assert.strictEqual(held[1]?.get("read"), "stored é");
  });

  test("the application theme survives a dispatch that omits a replacement", async () => {
    const themes: unknown[] = [];
    const { session } = harness({ open: told(themed("canvas")), reply: (_touch, call) => (call === 1 ? runs([]) : told(themed("canvas"))),
      draw: (frameTold) => { themes.push(frameTold.theme); return "HTML"; } });
    await session.start(open);
    for (let at = 0; at < 2; at += 1) {
      session.message({ type: "event", kind: 0, ordinal: at });
      await settleAll();
    }
    // Every frame is whole: a frame of plain rows draws no components, so the renderer is not
    // asked to draw without the theme; the next told frame carries it again.
    assert.deepStrictEqual(plain(themes), [{ tokens: [{ name: "canvas" }] }, { tokens: [{ name: "canvas" }] }]);
  });

  test("the application theme survives a dispatch that omits a replacement (extra: a new theme replaces the old one whole)", async () => {
    const themes: unknown[] = [];
    const { session } = harness({ open: told(themed("canvas")), reply: () => told(themed("accent")),
      draw: (frameTold) => { themes.push(frameTold.theme); return "HTML"; } });
    await session.start(open);
    session.message({ type: "event", kind: 0, ordinal: 0 });
    await settleAll();
    assert.deepStrictEqual(plain(themes), [{ tokens: [{ name: "canvas" }] }, { tokens: [{ name: "accent" }] }]);
  });
});

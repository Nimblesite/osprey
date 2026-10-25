import * as assert from "assert";
import {
  applyEnvelope,
  completionFrame,
  dispatchFrame,
  parseEnvelope,
  parseUiEvent,
} from "../../client/src/osprey-ui/protocol";
import { createWorkspaceSession } from "../../client/src/osprey-ui/session";
import type { NativeRequest } from "../../client/src/osprey-ui/protocol";

const envelope = JSON.stringify({
  version: 1,
  state: "opaque",
  slices: [{ key: "s", text: "t" }],
  patches: [{ key: "root", revision: 2, node: { kind: "text", text: "Hi", role: "body", tone: "primary" } }],
  removed: [],
  requests: [{ ticket: "ticket", kind: "host-call", namespace: "vscode", operation: "workspace.snapshot", arguments: {} }],
  diagnostics: ["notice"],
  theme: { tokens: [] },
});

suite("package workspace protocol", () => {
  test("a valid envelope keeps its opaque state, keyed patches and requests", () => {
    const parsed = parseEnvelope(envelope + "\n");
    assert.strictEqual(parsed.state, "opaque");
    assert.deepStrictEqual(parsed.patches.map((patch) => patch.key), ["root"]);
    assert.deepStrictEqual(parsed.requests.map((request) => request.ticket), ["ticket"]);
    assert.deepStrictEqual(parsed.diagnostics, ["notice"]);
  });

  test("keyed patches replace only their region and removals drop theirs", () => {
    const first = parseEnvelope(envelope);
    const regions = applyEnvelope(new Map(), first);
    const next = applyEnvelope(regions, parseEnvelope(JSON.stringify({
      version: 1, state: "next", slices: [], diagnostics: [], theme: null,
      patches: [{ key: "details", revision: 1, node: { kind: "text", text: "Detail", role: "body", tone: "primary" } }],
      removed: ["root"], requests: [],
    })));
    assert.deepStrictEqual([...next.keys()], ["details"]);
  });

  test("a wrong protocol version or malformed request is rejected, never guessed", () => {
    for (const bad of [
      JSON.stringify({ version: 2, state: "x", patches: [], requests: [] }),
      JSON.stringify({ version: 1, state: "x", patches: [], requests: [{ ticket: "", kind: "http" }] }),
      JSON.stringify({ version: 1, state: "x", patches: [{ key: "root", revision: -1, node: {} }], requests: [] }),
      "not json",
    ]) {
      assert.throws(() => parseEnvelope(bad), bad);
    }
  });

  test("host-held slices are validated and never silently discarded", () => {
    assert.deepStrictEqual(parseEnvelope(JSON.stringify({ version: 1, state: "x", slices: [{ key: "a", text: "b" }], patches: [], requests: [] })).slices, [{ key: "a", text: "b" }]);
    for (const bad of [
      { version: 1, state: "x", slices: "no", patches: [], requests: [] },
      { version: 1, state: "x", slices: [{ key: "", text: "b" }], patches: [], requests: [] },
      { version: 1, state: "x", slices: [{ key: "a", text: "b" }, { key: "a", text: "c" }], patches: [], requests: [] },
    ]) {
      assert.throws(() => parseEnvelope(JSON.stringify(bad)), /slice/);
    }
  });

  test("malformed envelopes fail explicitly at the wire boundary", () => {
    for (const fields of [
      { state: 1 }, { diagnostics: [1] }, { removed: [false] },
      { patches: null }, { requests: null },
      { slices: Array.from({ length: 257 }, (_, index) => ({ key: String(index), text: "" })) },
      { slices: [null] }, { slices: [{ key: 1, text: "" }] },
      { slices: [{ key: "s", text: false }] }, { slices: [{ key: "s".repeat(257), text: "" }] },
      { slices: [{ key: "s", text: "x".repeat(8 * 1024 * 1024 + 1) }] },
    ]) assert.throws(() => parseEnvelope(JSON.stringify({ ...JSON.parse(envelope), ...fields })), /envelope|slice/);
    for (const source of ["null", "[]", "false"]) assert.throws(() => parseEnvelope(source), /not a JSON object/);
  });

  test("optional wire fields default safely and diagnostic output does not hide the envelope", () => {
    const parsed = parseEnvelope('runtime log\n' + JSON.stringify({ version: 1, state: "s", patches: [], requests: [] }) + '\ntrailing diagnostic\n');
    assert.deepStrictEqual(parsed, { version: 1, state: "s", patches: [], requests: [], slices: [], removed: [], diagnostics: [], theme: null });
    assert.deepStrictEqual(JSON.parse(completionFrame("s", 1, "ticket", { ok: true, result: { ready: true } })).event,
      { kind: "completed", now: 1, ticket: "ticket", ok: true, result: { ready: true } });
  });

  test("malformed control addresses and messages cannot dispatch", () => {
    const regions = applyEnvelope(new Map(), parseEnvelope(envelope));
    for (const message of [null, [], { type: "other" },
      { type: "event", region: 1, revision: 2, ordinal: 0, value: null },
      { type: "event", region: "root", revision: -1, ordinal: 0, value: null },
      { type: "event", region: "root", revision: 2, ordinal: 0.5, value: null },
      { type: "event", region: "missing", revision: 2, ordinal: 0, value: null },
    ]) assert.strictEqual(parseUiEvent(message, regions, 1), undefined);
  });

  test("only a current region revision produces a control event", () => {
    const regions = applyEnvelope(new Map(), parseEnvelope(envelope));
    const good = parseUiEvent({ type: "event", region: "root", revision: 2, ordinal: 1, value: "query" }, regions, 10);
    assert.deepStrictEqual(good, { region: "root", revision: 2, ordinal: 1, value: "query", now: 10 });
    assert.strictEqual(parseUiEvent({ type: "event", region: "root", revision: 1, ordinal: 1, value: "query" }, regions, 10), undefined);
    assert.strictEqual(parseUiEvent({ type: "event", region: "root", revision: 2, ordinal: 1, value: { forged: true } }, regions, 10), undefined);
  });

  test("dispatch and completion frames keep the state opaque", () => {
    const dispatch = JSON.parse(dispatchFrame("opaque", { region: "root", revision: 2, ordinal: 1, value: "x", now: 9 }));
    assert.deepStrictEqual(dispatch, {
      operation: "dispatch", state: "opaque",
      event: { kind: "ui", now: 9, region: "root", revision: 2, ordinal: 1, value: "x" },
    });
    const done = JSON.parse(completionFrame("opaque", 11, "t", { ok: false, error: "no" }));
    assert.deepStrictEqual(done.event, { kind: "completed", now: 11, ticket: "t", ok: false, error: "no" });
  });
});

function sessionEnvelope(state: string, revision: number, requests: NativeRequest[] = [], slices: { key: string; text: string }[] = []): string {
  return JSON.stringify({
    version: 1, state, slices, diagnostics: [], theme: null, removed: [], requests,
    patches: [{ key: "root", revision, node: { kind: "text", text: "Ready", role: "body", tone: "primary" } }],
  });
}

const settled = async (): Promise<void> => {
  await new Promise((resolve) => setImmediate(resolve));
  await new Promise((resolve) => setImmediate(resolve));
};

suite("package workspace session", () => {
  test("a completion from a superseded session never reaches the runtime", async () => {
    let release: (value: unknown) => void = () => undefined;
    const gate = new Promise<unknown>((resolve) => { release = resolve; });
    const frames: string[] = [];
    const request: NativeRequest = { ticket: "old", kind: "host-call", namespace: "vscode", operation: "workspace.snapshot", arguments: {} };
    let runs = 0;
    const session = createWorkspaceSession({
      run: async (frame) => {
        frames.push(frame);
        runs += 1;
        return { stdout: sessionEnvelope(`s${runs}`, runs, runs === 1 ? [request] : []), exitCode: 0 };
      },
      render: () => "HTML", send: () => undefined, execute: () => gate, now: () => 1, report: () => undefined,
    });
    await session.start({});
    await session.start({});
    release({ trusted: true });
    await settled();
    assert.ok(!frames.some((frame) => frame.includes('"completed"') && frame.includes('"ticket":"old"')), frames.join("\n"));
  });

  test("host-held slices survive frames and are passed to the runtime", async () => {
    const seen: string[][] = [];
    let runs = 0;
    const session = createWorkspaceSession({
      run: async (_frame, slices) => {
        runs += 1;
        seen.push([...slices.keys()]);
        return { stdout: sessionEnvelope(`s${runs}`, runs, [], [{ key: "echo", text: "hello" }]), exitCode: 0 };
      },
      render: () => "HTML", send: () => undefined, execute: async () => undefined, now: () => 1, report: () => undefined,
    });
    await session.start({});
    session.message({ type: "event", region: "root", revision: 1, ordinal: 0, value: "typed" });
    await settled();
    assert.deepStrictEqual(seen.at(-1), ["echo"]);
  });

  test("disposal stops later events and completions", async () => {
    const frames: string[] = [];
    const sent: unknown[] = [];
    let release: (value: unknown) => void = () => undefined;
    const gate = new Promise<unknown>((resolve) => { release = resolve; });
    const request: NativeRequest = { ticket: "late", kind: "host-call", namespace: "vscode", operation: "workspace.snapshot", arguments: {} };
    const session = createWorkspaceSession({
      run: async (frame) => { frames.push(frame); return { stdout: sessionEnvelope("s1", 1, [request]), exitCode: 0 }; },
      render: () => "HTML", send: (message) => { sent.push(message); }, execute: () => gate, now: () => 1, report: () => undefined,
    });
    await session.start({});
    const before = frames.length;
    session.dispose();
    session.message({ type: "event", region: "root", revision: 1, ordinal: 0, value: "after" });
    release({});
    await settled();
    assert.strictEqual(frames.length, before);
    assert.ok(!sent.some((message) => (message as { type?: string }).type === "fatal"));
  });
});

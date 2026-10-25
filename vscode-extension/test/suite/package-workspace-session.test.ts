import * as assert from "assert";
import { createWorkspaceSession, type SessionDeps } from "../../client/src/osprey-ui/session";
import type { NativeRequest } from "../../client/src/osprey-ui/protocol";

function envelope(state: string, revision: number, requests: NativeRequest[] = []): string {
  return JSON.stringify({
    version: 1, state, slices: [], diagnostics: [], theme: null, removed: [], requests,
    patches: [{ key: "root", revision, node: { kind: "text", text: "Ready", role: "body", tone: "primary" } }],
  });
}

function recordedSession(initialRevision = 0, overrides: Partial<SessionDeps> = {}) {
  const frames: Record<string, unknown>[] = [];
  const sent: Record<string, unknown>[] = [];
  const session = createWorkspaceSession({
    run: async (frame) => {
      frames.push(JSON.parse(frame));
      const revision = initialRevision + frames.length;
      return { stdout: envelope(`s${revision}`, revision), exitCode: 0 };
    },
    render: () => "HTML", send: (message) => { sent.push(message as Record<string, unknown>); },
    execute: async () => undefined, now: () => 1, report: () => undefined, ...overrides,
  });
  return { session, frames, sent };
}

suite("package workspace session", () => {
  test("failed runtime commands report their exit code and available diagnostic output", async () => {
    for (const stdout of ["", " runtime diagnostic "]) {
      const reported: string[] = [];
      const { session, sent } = recordedSession(0, {
        run: async () => ({ stdout, exitCode: 7 }), report: (error) => { reported.push(error.message); },
      });
      await session.start({});
      const message = `The Osprey workspace exited with code 7${stdout ? ": runtime diagnostic" : ""}`;
      assert.deepStrictEqual(reported, [message]);
      assert.deepStrictEqual(sent.at(-1), { type: "fatal", message });
      session.dispose();
    }
  });

  test("runtime and rendering rejections remain visible even when they are not Error objects", async () => {
    for (const failure of ["wire refused", new Error("wire refused")]) {
      const { session, sent } = recordedSession(0, { run: async () => { throw failure; } });
      await session.start({});
      assert.deepStrictEqual(sent.at(-1), { type: "fatal", message: "wire refused" });
      session.dispose();
    }
    const { session, sent } = recordedSession(0, { render: () => { throw new Error("render refused"); } });
    await session.start({});
    assert.deepStrictEqual(sent.at(-1), { type: "fatal", message: "render refused" });
    session.dispose();
    await assert.rejects(session.start({}), /session is disposed/);
  });

  test("invalid edit sequences cannot execute or acknowledge a forged edit", async () => {
    const { session, sent, frames } = recordedSession();
    await session.start({});
    const before = sent.length;
    for (const edit of ["1", null, 0, -1, 0.5, Number.MAX_SAFE_INTEGER + 1]) {
      session.message({ type: "event", region: "root", revision: 1, ordinal: 0, value: "draft", edit });
    }
    await new Promise((resolve) => setImmediate(resolve));
    assert.strictEqual(frames.length, 1);
    assert.strictEqual(sent.length, before);
    session.dispose();
  });

  test("a rejected host capability is returned to Osprey as an explicit failed completion", async () => {
    const frames: Record<string, unknown>[] = [];
    const request: NativeRequest = { ticket: "refused", kind: "host-call" };
    const { session } = recordedSession(0, {
      run: async (frame) => {
        frames.push(JSON.parse(frame));
        return { stdout: envelope("s", 1, frames.length === 1 ? [request] : []), exitCode: 0 };
      }, execute: async () => { throw "capability refused"; },
    });
    await session.start({});
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepStrictEqual(frames.at(-1)?.event,
      { kind: "completed", now: 1, ticket: "refused", ok: false, error: "capability refused" });
    session.dispose();
  });

  test("disposal quarantines a pending runtime response before it can render or execute capabilities", async () => {
    let release: (result: { stdout: string; exitCode: number }) => void = () => undefined;
    const pending = new Promise<{ stdout: string; exitCode: number }>((resolve) => { release = resolve; });
    let executed = 0;
    const { session, sent } = recordedSession(0, { run: () => pending, execute: async () => { executed++; } });
    const started = session.start({});
    await new Promise((resolve) => setImmediate(resolve));
    const before = sent.length;
    session.dispose();
    release({ stdout: envelope("late", 1, [{ ticket: "late", kind: "host-call" }]), exitCode: 0 });
    await started;
    assert.strictEqual(sent.length, before);
    assert.strictEqual(executed, 0);
  });

  test("runtime diagnostics explicitly refuse an edit before publishing the next screen", async () => {
    let runs = 0;
    const { session, sent } = recordedSession(0, { run: async () => {
      const parsed = JSON.parse(envelope("s", ++runs));
      return { stdout: JSON.stringify({ ...parsed, diagnostics: runs > 1 ? ["binding refused"] : [] }), exitCode: 0 };
    } });
    await session.start({});
    session.message({ type: "event", region: "root", revision: 1, ordinal: 0, value: "draft", edit: 1 });
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepStrictEqual(sent.slice(-2), [{ type: "edit-result", edit: 1, accepted: false, error: "binding refused" },
      { type: "render", html: "HTML", diagnostics: ["binding refused"] }]);
    session.dispose();
  });

  test("expired edits are refused and a live edit cannot substitute its prior state", async () => {
    const { session, frames, sent } = recordedSession();
    await session.start({});
    for (let revision = 1; revision <= 35; revision++) {
      session.message({ type: "event", region: "root", revision, ordinal: 0, value: null });
      await new Promise((resolve) => setImmediate(resolve));
    }
    session.message({ type: "event", region: "root", revision: 1, ordinal: 0, value: "expired", edit: 8 });
    assert.deepStrictEqual(sent.at(-1), { type: "edit-result", edit: 8, accepted: false,
      error: "The original text control is no longer available; the edit was not applied." });
    session.message({ type: "event", region: "root", revision: 36, ordinal: 0, value: "live", edit: 9, prior: "forged" });
    await new Promise((resolve) => setImmediate(resolve));
    assert.deepStrictEqual(frames.at(-1)?.event, { kind: "ui-edit", prior: "s36", region: "root", revision: 36, ordinal: 0, value: "live", now: 1 });
    session.dispose();
  });

  test("an edit uses its issued snapshot and receives acknowledgment before the render", async () => {
    const { session, frames, sent } = recordedSession();
    await session.start({});
    session.message({ type: "event", region: "root", revision: 1, ordinal: 0, value: null });
    await new Promise((resolve) => setImmediate(resolve));
    session.message({ type: "event", region: "root", revision: 1, ordinal: 0, value: " draft ", edit: 7 });
    await new Promise((resolve) => setImmediate(resolve));
    assert.strictEqual(frames.length, 3);
    assert.deepStrictEqual(frames[2], { operation: "dispatch", state: "s2", event: {
      kind: "ui-edit", prior: "s1", region: "root", revision: 1, ordinal: 0, value: " draft ", now: 1,
    } });
    assert.deepStrictEqual(sent.slice(-2), [{ type: "edit-result", edit: 7, accepted: true },
      { type: "render", html: "HTML", diagnostics: [] }]);
    session.dispose();
  });

  test("an edit from an unknown revision is explicitly refused without dispatch", async () => {
    const { session, frames, sent } = recordedSession(3);
    await session.start({});
    session.message({ type: "event", region: "root", revision: 3, ordinal: 0, value: "draft", edit: 8 });
    await new Promise((resolve) => setImmediate(resolve));
    assert.strictEqual(frames.length, 1);
    assert.deepStrictEqual(sent.at(-1), { type: "edit-result", edit: 8, accepted: false,
      error: "The original text control is no longer available; the edit was not applied." });
    session.dispose();
  });

  test("start applies keyed regions and executes only requests from the envelope", async () => {
    const frames: string[] = [];
    const sent: unknown[] = [];
    const executed: NativeRequest[] = [];
    const request: NativeRequest = { ticket: "snapshot", kind: "host-call", namespace: "vscode", operation: "workspace.snapshot", arguments: {} };
    const session = createWorkspaceSession({
      run: async (frame) => {
        frames.push(frame);
        return { stdout: frame.includes('"operation":"start"') ? envelope("s1", 1, [request]) : envelope("s2", 2), exitCode: 0 };
      },
      render: (_regions, _diagnostics, _theme) => "HTML",
      send: (message) => { sent.push(message); },
      execute: async (item) => { executed.push(item); return { trusted: true }; },
      now: () => 7,
      report: () => undefined,
    });
    await session.start({ platform: "vscode" });
    await new Promise((resolve) => setImmediate(resolve));
    assert.ok(sent.some((message) => (message as { html?: string }).html === "HTML"));
    assert.deepStrictEqual(executed.map((item) => item.ticket), ["snapshot"]);
    assert.ok(frames.some((frame) => frame.includes('"kind":"completed"') && frame.includes('"state":"s1"')));
    assert.ok(!frames.some((frame) => frame.includes('"operation":"dispatch"') && !frame.includes('"completed"')));
  });

  test("a forged or stale control event never reaches the runtime", async () => {
    const frames: string[] = [];
    const session = createWorkspaceSession({
      run: async (frame) => { frames.push(frame); return { stdout: envelope("s1", 4), exitCode: 0 }; },
      render: () => "HTML", send: () => undefined, execute: async () => undefined, now: () => 1, report: () => undefined,
    });
    await session.start({});
    const before = frames.length;
    session.message({ type: "event", region: "root", revision: 3, ordinal: 0, value: "forged" });
    session.message({ type: "event", region: "root", revision: 4, ordinal: 0, value: { forged: true } });
    await new Promise((resolve) => setImmediate(resolve));
    assert.strictEqual(frames.length, before);
    session.message({ type: "event", region: "root", revision: 4, ordinal: 0, value: "real" });
    await new Promise((resolve) => setImmediate(resolve));
    assert.ok(frames.some((frame) => frame.includes('"state":"s1"') && frame.includes('"value":"real"')));
  });
});

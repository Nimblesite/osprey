// Tests for [PROF-LSP-HOST] and [PROF-LSP-RUNTIME] over the real `osprey lsp`,
// ported from SharpLsp's LSP-level protocol suite: initialize advertises every
// registry operation once under Osprey's prefix with nothing from the UI
// namespace, params are exactly one typed object, every refusal leaves the
// message loop answering and nothing running, and the error taxonomy holds:
// a missing PID, a process no runtime recognises, the editor itself, and an
// Osprey program asked for the memory tracking it has no backend for.
import * as assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import type { ProcessList, ProfileSessionList } from "@nimblesite/lsp-profiler-ui/out/src/wire-generated";
import { execute, executeError, FIXTURE_DIR, lspClient, lspCommand, profilingSuite } from "./profiler-kit";

const STOP_MS = 60_000;

/** The JSON-RPC codes of lspkit's error taxonomy [PROF-ERRORS]. */
const CODE = { INVALID_PARAMS: -32602, PROCESS_NOT_FOUND: -32001, UNSUPPORTED_RUNTIME: -32002 } as const;

/** Every operation the wire registry names, under Osprey's prefix. */
const COMMANDS = [
  "capabilities", "processes", "kill",
  "cpu.start", "cpu.stop", "cpu.snapshot", "cpu.list", "cpu.cooperativeScript", "cpu.cooperativeAttach",
  "memory.start", "memory.snapshot", "memory.diff", "memory.references", "memory.objectsByType",
  "memory.gcCollect", "memory.stop", "memory.ingest",
].map(lspCommand);

/** `workspace/executeCommand` with exactly these `arguments`, as no library caller would send them. */
async function raw(command: string, args: unknown[]): Promise<unknown> {
  return lspClient().sendRequest("workspace/executeCommand", { command, arguments: args });
}

/** The error `raw` rejects with; fails the test when it resolves. */
async function rawError(command: string, args: unknown[]): Promise<{ code: number; message: string }> {
  try {
    await raw(command, args);
  } catch (error: unknown) {
    const failure = error as { code?: unknown; message?: unknown };
    return { code: typeof failure.code === "number" ? failure.code : 0, message: String(failure.message ?? error) };
  }
  assert.fail(`${command} ${JSON.stringify(args)} must be refused`);
}

async function noSessions(what: string): Promise<void> {
  assert.deepEqual((await execute<ProfileSessionList>("cpu.list", {})).sessions, [], what);
}

/** A running process no profiler runtime recognises: a plain `sleep`. */
function sleeper(): ChildProcess {
  const child = spawn("sleep", ["240"], { stdio: "ignore" });
  assert.ok(child.pid !== undefined, "the decoy process started");
  return child;
}

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

suite("LSP-level profiler protocol — a live Osprey program behind the same server", () => {
  const live = profilingSuite("lsp_protocol");

  test("initialize advertises every registry operation once, under the server prefix, with nothing from the UI namespace and experimental.lspProfiler set", () => {
    const capabilities = lspClient().initializeResult?.capabilities;
    assert.ok(capabilities, "initialize completed");
    const advertised = capabilities.executeCommandProvider?.commands ?? [];
    assert.equal(advertised.includes(lspCommand("cpu.progress")), false, "progress is a notification, not a command");
    for (const commandId of COMMANDS) {
      assert.equal(advertised.filter((entry) => entry === commandId).length, 1, `${commandId} is advertised exactly once`);
    }
    const profilerCommands = advertised.filter((entry) => entry.includes("lspProfiler"));
    assert.deepEqual([...profilerCommands].sort(), [...COMMANDS].sort(), "the advertised profiler commands are exactly the registry");
    for (const commandId of profilerCommands) {
      assert.match(commandId, /^osprey\.lspProfiler\.[A-Za-z]+(?:\.[A-Za-z]+)?$/, `the id composes from prefix, area and action: ${commandId}`);
    }
    assert.deepEqual(advertised.filter((entry) => entry.startsWith("osprey.profiler.")), [], "UI ids stay client-side");
    const runtimes = (capabilities.experimental as { lspProfiler?: { runtimes?: { id: string; display_name: string; memory?: unknown; cpu?: unknown }[] } } | undefined)?.lspProfiler?.runtimes;
    assert.deepEqual(runtimes?.map((runtime) => [runtime.id, runtime.display_name]), [["osprey", "Osprey"]], "the Osprey runtime is mounted");
    assert.ok(runtimes?.[0]?.cpu !== undefined, "Osprey samples CPU");
    assert.equal(runtimes?.[0]?.memory, undefined, "and advertises no memory backend");
    assert.ok(capabilities.textDocumentSync !== undefined, "the profiler never replaces the language server's own capabilities");
  });

  test("params are exactly one typed object: zero, two, a number, a string, null and a bad field are INVALID_PARAMS, and an unknown operation fails without wedging the loop", async function () {
    this.timeout(STOP_MS);
    const malformed: readonly (readonly [string, unknown[]])[] = [
      ["two arguments", [{}, {}]], ["a number", [42]], ["a string", ["pid=1"]], ["null", [null]],
    ];
    for (const [label, args] of malformed) {
      const failure = await rawError(lspCommand("cpu.start"), args);
      assert.equal(failure.code, CODE.INVALID_PARAMS, `${label}: ${JSON.stringify(failure)}`);
      assert.ok(failure.message.length > 0, `${label} explains itself`);
    }
    assert.equal((await rawError(lspCommand("cpu.list"), [{}, {}])).code, CODE.INVALID_PARAMS, "two valid objects are still two arguments");
    assert.equal((await rawError(lspCommand("cpu.start"), [{ pid: "not-a-pid" }])).code, CODE.INVALID_PARAMS, "pid must be a number");
    assert.equal((await rawError(lspCommand("cpu.start"), [])).code, CODE.INVALID_PARAMS, "absent params read as {}, which lacks the pid");
    await noSessions("no session from a refused call");
    assert.ok((await rawError(lspCommand("cpu.bogus"), [{}])).message.length > 0, "the unknown operation is reported");
    await noSessions("the message loop was never blocked");
  });

  test("all seventeen requests enforce their typed arguments with INVALID_PARAMS and leave the server responsive", async function () {
    this.timeout(STOP_MS * 2);
    const empty = new Set(["capabilities", "cpu.list"].map(lspCommand));
    for (const commandId of COMMANDS) {
      if (empty.has(commandId)) {
        const result = await raw(commandId, [{}]);
        assert.ok(result !== null && typeof result === "object", `${commandId} accepts its empty params object`);
      } else {
        assert.equal((await rawError(commandId, [{}])).code, CODE.INVALID_PARAMS, `${commandId} requires its typed fields`);
      }
      for (const args of [[{}, {}], [42], ["not-an-object"]]) {
        assert.equal((await rawError(commandId, args)).code, CODE.INVALID_PARAMS, `${commandId} rejects ${JSON.stringify(args)}`);
      }
      await noSessions(`${commandId} created no CPU session`);
    }
    const processes = await execute<ProcessList>("processes", { workspace_roots: [] });
    assert.ok(processes.processes.some((entry) => entry.pid === live.target.pid), "discovery still works after the sweep");
  });

  test("the error taxonomy: PROCESS_NOT_FOUND, UNSUPPORTED_RUNTIME, kill never reaches an unprofilable process, and an Osprey program refuses memory tracking", async function () {
    this.timeout(STOP_MS);
    const { target } = live;
    const decoy = sleeper();
    try {
      assert.equal((await executeError("kill", { pid: 2_147_483_647 }))?.code, CODE.PROCESS_NOT_FOUND, "kill of a missing PID");
      assert.equal((await executeError("cpu.start", { pid: 2_147_483_647 }))?.code, CODE.PROCESS_NOT_FOUND, "the same code for start");
      const decoyPid = decoy.pid ?? 0;
      assert.equal((await executeError("kill", { pid: decoyPid }))?.code, CODE.UNSUPPORTED_RUNTIME, "kill never reaches a process no runtime recognises");
      assert.equal(isAlive(decoyPid), true, "the unprofilable process is still alive");
      assert.equal((await executeError("cpu.start", { pid: process.pid }))?.code, CODE.UNSUPPORTED_RUNTIME, "the editor is not profiled");
      const memory = await executeError("memory.start", { pid: target.pid });
      assert.ok(memory !== undefined, "an Osprey program cannot be memory-tracked");
      assert.ok(memory.code !== 0 && memory.message.length > 0, `the refusal is classified and explained: ${JSON.stringify(memory)}`);
      assert.ok(target.alive(), "a refused memory start leaves the program running");
      const listed = (await execute<ProcessList>("processes", { workspace_roots: [] })).processes;
      const osprey = listed.find((entry) => entry.pid === target.pid);
      assert.ok(osprey, "the Osprey program is discovered");
      assert.equal(osprey.runtime, "osprey");
      assert.equal(osprey.attachable, true);
      assert.equal(osprey.unattachable_reason, undefined, "an attachable process carries no refusal reason");
      assert.ok(osprey.memory_bytes > 0 && osprey.runtime_secs >= 0 && osprey.cpu_percent >= 0, "resource readings are sane");
      assert.ok(!listed.some((entry) => entry.pid === decoyPid), "the decoy is never listed");
      const firstBlocked = listed.findIndex((entry) => !entry.attachable);
      for (const entry of firstBlocked < 0 ? [] : listed.slice(firstBlocked)) {
        assert.ok(!entry.attachable && (entry.unattachable_reason ?? "").length > 0, `blocked ${entry.pid} sorts last and says why`);
      }
      const scoped = await execute<ProcessList>("processes", { workspace_roots: [FIXTURE_DIR] });
      assert.equal(scoped.processes.find((entry) => entry.pid === target.pid)?.in_workspace, true, "membership follows the root without hiding the target");
      await noSessions("no refused start left a session");
    } finally {
      decoy.kill();
    }
  });
});

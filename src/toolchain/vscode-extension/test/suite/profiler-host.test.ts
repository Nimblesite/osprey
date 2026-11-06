// [PROF-VSCODE-SHARED] Osprey's ProfilerHost seams: the debuggee PID lldb-dap
// reports, and the one profiling launch Osprey offers.
import * as assert from "assert";
import { processEventPid, profilingLaunch } from "../../client/src/profiler/profiler-host";

const processEvent = (body: unknown): unknown => ({ type: "event", event: "process", body });

suite("profiler-host processEventPid", () => {
  test("reads the PID of a DAP process event", () => {
    assert.strictEqual(processEventPid(processEvent({ systemProcessId: 4242 })), 4242);
  });

  test("anything but a process event with a positive numeric PID has none", () => {
    for (const message of [
      undefined,
      null,
      "process",
      { type: "event", event: "output", body: { systemProcessId: 4242 } },
      { type: "response", event: "process", body: { systemProcessId: 4242 } },
      processEvent(undefined),
      processEvent(null),
      processEvent(4242),
      processEvent({}),
      processEvent({ systemProcessId: "4242" }),
      processEvent({ systemProcessId: 0 }),
      processEvent({ systemProcessId: -1 }),
    ]) {
      assert.strictEqual(processEventPid(message), undefined, `expected no PID: ${JSON.stringify(message)}`);
    }
  });
});

suite("profiler-host profilingLaunch", () => {
  test("a CPU launch debug-launches the program through the Osprey adapter", () => {
    assert.deepStrictEqual(profilingLaunch("cpu", "/w/app/fib.osp"), {
      type: "osprey",
      request: "launch",
      name: "Run & Profile CPU (Current File)",
      program: "/w/app/fib.osp",
      cwd: "/w/app",
      profileOnLaunch: true,
    });
  });

  test("a memory launch is not offered: Osprey has no memory backend", () => {
    assert.strictEqual(profilingLaunch("memory", "/w/app/fib.osp"), undefined);
  });
});

// Local package registry controller ([PACKAGE-LOCAL-REGISTRY]): starting,
// stopping and the "start it?" prompt are driven through injected fakes, and
// the health probe runs against a real loopback HTTP server.

import * as assert from "assert";
import * as fs from "fs";
import * as http from "http";
import * as os from "os";
import * as path from "path";
import { shipwrightPlatform } from "../../client/src/client-config";
import { fetchListingText } from "../../client/src/package-api";
import {
  failureOf,
  healthCheck,
  LocalRegistry,
  LocalRegistryDeps,
  loopbackOrigin,
  RegistryState,
  serverConfig,
  ServerProcess,
  spawnServer,
  START_ACTION,
} from "../../client/src/local-registry";

interface FakeServer extends ServerProcess {
  emit(text: string): void;
  exit(code: number | null): void;
  killed: boolean;
}

function fakeServer(): FakeServer {
  const output: ((text: string) => void)[] = [];
  const exits: ((code: number | null) => void)[] = [];
  const server: FakeServer = {
    killed: false,
    onOutput: (listener) => { output.push(listener); },
    onExit: (listener) => { exits.push(listener); },
    kill: () => { server.killed = true; },
    emit: (text) => output.forEach((listener) => listener(text)),
    exit: (code) => exits.forEach((listener) => listener(code)),
  };
  return server;
}

interface Recorded {
  deps: LocalRegistryDeps;
  configs: string[];
  starts: { executable: string; cwd: string }[];
  errors: string[];
  infos: string[];
  asked: string[];
  states: RegistryState[];
  logs: string[];
  server: FakeServer;
}

/** Fake deps: `health` answers each probe in turn and then repeats its last answer. */
function recorded(options: {
  registry?: string;
  health?: boolean[];
  missing?: string[];
  answer?: string;
  onStart?: (server: FakeServer) => void;
} = {}): Recorded {
  const health = [...(options.health ?? [false])];
  const server = fakeServer();
  const record: Recorded = { configs: [], starts: [], errors: [], infos: [], asked: [], states: [], logs: [], server } as unknown as Recorded;
  record.deps = {
    registry: () => options.registry ?? "http://127.0.0.1:8787",
    layout: () => ({ executable: "/ext/bin/osprey-registry", root: "/ext/bin/registry" }),
    exists: (file) => !(options.missing ?? []).includes(file),
    writeConfig: (text) => { record.configs.push(text); return "/storage/local-registry"; },
    start: (executable, cwd) => {
      record.starts.push({ executable, cwd });
      options.onStart?.(server);
      return server;
    },
    healthy: async () => (health.length > 1 ? health.shift() : health[0]) ?? false,
    log: (line) => { record.logs.push(line); },
    ask: async (message) => { record.asked.push(message); return options.answer; },
    error: (message) => { record.errors.push(message); },
    info: (message) => { record.infos.push(message); },
    pause: () => new Promise((resolve) => setImmediate(resolve)),
    changed: (state) => { record.states.push(state); },
  };
  return record;
}

suite("local package registry", () => {
  test("only loopback HTTP origins have a local server, with their address and port", () => {
    assert.deepStrictEqual(loopbackOrigin("http://127.0.0.1:8787"), { origin: "http://127.0.0.1:8787", address: "127.0.0.1", port: 8787 });
    assert.deepStrictEqual(loopbackOrigin("http://localhost:9000/"), { origin: "http://localhost:9000", address: "127.0.0.1", port: 9000 });
    assert.deepStrictEqual(loopbackOrigin("http://[::1]"), { origin: "http://[::1]", address: "::1", port: 80 });
    for (const remote of ["https://packages.example", "http://packages.example:8787", "https://127.0.0.1:8787", "not a url"]) {
      assert.strictEqual(loopbackOrigin(remote), undefined, remote);
    }
  });

  test("the server configuration names the root, address and port it is told", () => {
    assert.deepStrictEqual(JSON.parse(serverConfig("/ext/bin/registry", "127.0.0.1", 8787)), { root: "/ext/bin/registry", address: "127.0.0.1", port: 8787 });
  });

  test("a failed start reports the server's own error lines", () => {
    assert.strictEqual(failureOf("serving @osprey/re\nerror: cannot listen on 127.0.0.1:8787 (-1)\nstatus: 1\n"), "error: cannot listen on 127.0.0.1:8787 (-1)");
    assert.strictEqual(failureOf("status: 1\n"), "the server exited before it answered");
  });

  test("start writes the configuration, runs the bundled server and waits for health", async () => {
    const r = recorded({ health: [false, false, false, true] });
    assert.strictEqual(await new LocalRegistry(r.deps, 10_000, 1).start(), true);
    assert.deepStrictEqual(JSON.parse(r.configs[0]), { root: "/ext/bin/registry", address: "127.0.0.1", port: 8787 });
    assert.deepStrictEqual(r.starts, [{ executable: "/ext/bin/osprey-registry", cwd: "/storage/local-registry" }]);
    assert.deepStrictEqual(r.states, ["starting", "running"]);
    assert.deepStrictEqual(r.infos, ["Local package registry running at http://127.0.0.1:8787."]);
  });

  test("start does nothing but report when a registry already answers", async () => {
    const r = recorded({ health: [true] });
    const registry = new LocalRegistry(r.deps);
    assert.strictEqual(await registry.start(), true);
    assert.strictEqual(r.starts.length, 0);
    assert.strictEqual(registry.current, "external");
  });

  test("concurrent starts share one server", async () => {
    const r = recorded({ health: [false, false, true] });
    const registry = new LocalRegistry(r.deps, 10_000, 1);
    assert.deepStrictEqual(await Promise.all([registry.start(), registry.start()]), [true, true]);
    assert.strictEqual(r.starts.length, 1);
  });

  test("a missing server or snapshot is named and nothing is started", async () => {
    for (const missing of ["/ext/bin/osprey-registry", "/ext/bin/registry"]) {
      const r = recorded({ missing: [missing] });
      assert.strictEqual(await new LocalRegistry(r.deps).start(), false);
      assert.strictEqual(r.starts.length, 0);
      assert.ok(r.errors[0].includes(missing), r.errors[0]);
    }
  });

  test("a remote registry has no local server to start", async () => {
    const r = recorded({ registry: "https://packages.example" });
    assert.strictEqual(await new LocalRegistry(r.deps).start(), false);
    assert.ok(r.errors[0].includes("not a loopback HTTP origin"), r.errors[0]);
  });

  test("a server that exits before answering is reported with its error", async () => {
    // A child process reports output and exit on a later tick, after its listeners attach.
    const r = recorded({ health: [false], onStart: (server) => setImmediate(() => {
      server.emit("error: cannot listen on 127.0.0.1:8787 (-1)\n");
      server.exit(1);
    }) });
    const registry = new LocalRegistry(r.deps, 10_000, 1);
    assert.strictEqual(await registry.start(), false);
    assert.deepStrictEqual(r.errors, ["The local package registry did not start: error: cannot listen on 127.0.0.1:8787 (-1)."]);
    assert.strictEqual(registry.current, "stopped");
    assert.ok(r.logs.includes("error: cannot listen on 127.0.0.1:8787 (-1)"), r.logs.join("|"));
  });

  test("a server that never answers is killed after the timeout", async () => {
    const r = recorded({ health: [false] });
    assert.strictEqual(await new LocalRegistry(r.deps, 5, 1).start(), false);
    assert.strictEqual(r.server.killed, true);
    assert.ok(r.errors[0].includes("did not answer http://127.0.0.1:8787/v1/health"), r.errors[0]);
  });

  test("stop kills only the server this window started", async () => {
    const r = recorded({ health: [false, true] });
    const registry = new LocalRegistry(r.deps, 10_000, 1);
    assert.strictEqual(registry.stop(), false);
    await registry.start();
    assert.strictEqual(registry.stop(), true);
    assert.strictEqual(r.server.killed, true);
    assert.strictEqual(registry.current, "stopped");
    assert.strictEqual(registry.stop(), false);
  });

  test("the package manager asks to start a silent local registry and starts it on yes", async () => {
    const r = recorded({ health: [false, false, true], answer: START_ACTION });
    assert.strictEqual(await new LocalRegistry(r.deps, 10_000, 1).ensureRunning(), true);
    assert.deepStrictEqual(r.asked, ["The Osprey package registry at http://127.0.0.1:8787 is not running. Start the local registry?"]);
    assert.strictEqual(r.starts.length, 1);
  });

  test("declining the prompt opens nothing and starts nothing", async () => {
    const r = recorded({ health: [false] });
    assert.strictEqual(await new LocalRegistry(r.deps).ensureRunning(), false);
    assert.strictEqual(r.asked.length, 1);
    assert.strictEqual(r.starts.length, 0);
  });

  test("a running or remote registry is used without asking", async () => {
    for (const r of [recorded({ health: [true] }), recorded({ registry: "https://packages.example" })]) {
      assert.strictEqual(await new LocalRegistry(r.deps).ensureRunning(), true);
      assert.strictEqual(r.asked.length, 0);
    }
  });

  test("refresh reflects whether something answers", async () => {
    const r = recorded({ health: [true, false] });
    const registry = new LocalRegistry(r.deps);
    assert.strictEqual(await registry.refresh(), "external");
    assert.strictEqual(await registry.refresh(), "stopped");
  });

  test("the health probe accepts only 200 from /v1/health", async () => {
    const paths: string[] = [];
    const server = http.createServer((request, response) => {
      paths.push(request.url ?? "");
      response.statusCode = request.url === "/v1/health" ? 200 : 404;
      response.end("{}");
    });
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const origin = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
    try {
      assert.strictEqual(await healthCheck(origin), true);
      assert.deepStrictEqual(paths, ["/v1/health"]);
    } finally {
      server.closeAllConnections();
      await new Promise<void>((resolve) => server.close(() => resolve()));
    }
    assert.strictEqual(await healthCheck(origin, 500), false);
  });
});

// The bundled server over the bundled snapshot of packages/, on a free port:
// `make _vsix_bundle` places both, and `_vsix_verify` refuses a run without them.
suite("local package registry server", () => {
  const bundle = path.resolve(__dirname, "..", "..", "..", "bin", shipwrightPlatform());
  const executable = path.join(bundle, process.platform === "win32" ? "osprey-registry.exe" : "osprey-registry");

  test("the bundled server serves the repository's packages and stops", async function () {
    this.timeout(120_000);
    const port = await freePort();
    const storage = fs.mkdtempSync(path.join(os.tmpdir(), "osprey local registry "));
    const r = recorded();
    const deps: LocalRegistryDeps = {
      ...r.deps,
      registry: () => `http://127.0.0.1:${port}`,
      layout: () => ({ executable, root: path.join(bundle, "registry") }),
      exists: (file) => fs.existsSync(file),
      writeConfig: (text) => { fs.writeFileSync(path.join(storage, "registry-server.json"), text); return storage; },
      start: spawnServer,
      healthy: (origin) => healthCheck(origin),
      pause: (ms) => new Promise((resolve) => setTimeout(resolve, ms)),
    };
    const registry = new LocalRegistry(deps);
    try {
      assert.strictEqual(await registry.start(), true, r.errors.join("|"));
      const listing = JSON.parse(await fetchListingText(`http://127.0.0.1:${port}`)) as { packages: { key: string }[] };
      const keys = listing.packages.map((entry) => entry.key);
      for (const key of ["@osprey/native", "@osprey/re", "@osprey/torch"]) assert.ok(keys.includes(key), keys.join(","));
    } finally {
      registry.stop();
      fs.rmSync(storage, { recursive: true, force: true });
    }
    for (let i = 0; i < 40 && await healthCheck(`http://127.0.0.1:${port}`, 250); i++) await new Promise((resolve) => setTimeout(resolve, 50));
    assert.strictEqual(await healthCheck(`http://127.0.0.1:${port}`, 250), false);
  });
});

function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const probe = http.createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const port = (probe.address() as { port: number }).port;
      probe.close(() => resolve(port));
    });
  });
}

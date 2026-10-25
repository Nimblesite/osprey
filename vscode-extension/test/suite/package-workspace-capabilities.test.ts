import * as assert from "assert";
import * as fs from "fs";
import * as http from "http";
import * as os from "os";
import * as path from "path";
import { pathToFileURL } from "url";
import { createRequestExecutor, type CapabilityHost } from "../../client/src/osprey-ui/capabilities";
import type { NativeRequest } from "../../client/src/osprey-ui/protocol";

function temporary(): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), "osprey workspace "));
}

function hostFor(root: string, dirty: string[] = []): CapabilityHost {
  return {
    trusted: true,
    workspaceFolders: [root],
    configuredExecutable: process.execPath,
    bundledExecutable: "",
    dirtyFiles: () => dirty,
    workspaceSnapshot: () => ({ trusted: true }),
    openDocument: async () => ({ opened: true }),
    readConfiguration: (section) => ({ section }),
    revealView: async () => ({ revealed: true }),
  };
}

function processRequest(root: string, script: string): NativeRequest {
  return {
    ticket: "run:1", kind: "host-call", namespace: "vscode", operation: "process.input",
    timeout: 10000, limit: 1024 * 1024,
    arguments: { executable: "osprey-pkg", argv: [script], stdin: "hello from stdin\n", workspace: pathToFileURL(root).toString() },
  };
}

async function withRoot(action: (root: string) => Promise<void>): Promise<void> {
  const root = temporary();
  try { await action(root); } finally { fs.rmSync(root, { recursive: true, force: true }); }
}

function processFields(root: string, args: Record<string, unknown> = {}, fields: Record<string, unknown> = {}): NativeRequest {
  const request = processRequest(root, "unused");
  return { ...request, ...fields, arguments: { ...(request.arguments as object), ...args } };
}

function httpFixture(request: http.IncomingMessage, response: http.ServerResponse): void {
  if (request.url === "/stall") return;
  response.writeHead(200, { "content-type": "application/json" });
  if (request.url === "/interrupt") {
    response.write("partial");
    setImmediate(() => response.destroy());
  } else response.end(JSON.stringify({ ok: true }));
}

async function withServer(action: (url: string, headers: () => http.IncomingHttpHeaders) => Promise<void>): Promise<void> {
  let received: http.IncomingHttpHeaders = {};
  const server = http.createServer((request, response) => { received = request.headers; httpFixture(request, response); });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address === "object");
  try { await action(`http://127.0.0.1:${address.port}/`, () => received); } finally {
    server.closeAllConnections();
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
}

suite("package workspace capabilities", () => {
  test("untrusted, relative, missing and absent executables are refused before launch", async () => {
    await withRoot(async (root) => {
      const cases: [Partial<CapabilityHost>, RegExp][] = [
        [{ trusted: false }, /Trust this workspace/], [{ configuredExecutable: "relative" }, /absolute path/],
        [{ configuredExecutable: path.join(root, "missing") }, /does not exist/],
        [{ configuredExecutable: "", bundledExecutable: "" }, /not bundled or configured/],
      ];
      for (const [fields, expected] of cases) {
        await assert.rejects(createRequestExecutor({ ...hostFor(root), ...fields })(processFields(root)), expected);
      }
    });
  });

  test("malformed and non-directory workspace addresses cannot launch commands", async () => {
    await withRoot(async (root) => {
      const file = path.join(root, "file");
      fs.writeFileSync(file, "file");
      for (const [workspace, error] of [
        ["invalid", /not a URI/], ["https://example.test/", /plain file URI/],
        [pathToFileURL(path.join(root, "missing")).toString(), /does not exist/],
        [pathToFileURL(file).toString(), /not a directory/],
      ] as const) await assert.rejects(createRequestExecutor(hostFor(root))(processFields(root, { workspace })), error);
    });
  });

  test("process bounds reject malformed arguments, input and limits", async () => {
    await withRoot(async (root) => {
      for (const args of [{ argv: "bad" }, { argv: [42] }, { argv: Array(65).fill("x") },
        { argv: ["x".repeat(8193)] }, { stdin: 42 }, { stdin: "x".repeat(1024 * 1024 + 1) }]) {
        await assert.rejects(createRequestExecutor(hostFor(root))(processFields(root, args)), /Request field/);
      }
      for (const fields of [{ timeout: 0 }, { timeout: 0.5 }, { timeout: "1" }, { limit: 16 * 1024 * 1024 + 1 }]) {
        await assert.rejects(createRequestExecutor(hostFor(root))(processFields(root, {}, fields)), /between 1 and/);
      }
      await assert.rejects(createRequestExecutor(hostFor(root))({ ticket: "t", kind: "host-call", namespace: "vscode", operation: "process.input" }), /missing its arguments/);
    });
  });

  test("bundled executables and omitted process options preserve real output and numeric exits", async () => {
    await withRoot(async (root) => {
      const execute = createRequestExecutor({ ...hostFor(root), configuredExecutable: "", bundledExecutable: process.execPath });
      const result = await execute(processFields(root, { argv: undefined, stdin: 'process.stdout.write("default input")' },
        { timeout: undefined, limit: undefined }));
      assert.deepStrictEqual(result, { code: 0, stdout: "default input", stderr: "" });
      const failed = await execute(processFields(root, { argv: ["-e", 'process.stderr.write("boom");process.exit(3)'], stdin: undefined }));
      assert.deepStrictEqual(failed, { code: 3, stdout: "", stderr: "boom" });
      await assert.rejects(execute(processFields(root, { argv: ["-e", "setTimeout(()=>{},9999)"], stdin: undefined }, { timeout: 100 })), /Package command failed/);
    });
  });

  test("nested workspace projects run despite unrelated unsaved files with missing parents", async () => {
    await withRoot(async (root) => {
      const nested = path.join(root, "nested");
      fs.mkdirSync(nested);
      const execute = createRequestExecutor(hostFor(root, [path.join(root, "missing", "note.osp")]));
      const result = await execute(processFields(nested, { argv: ["-e", "process.stdout.write(process.cwd())"], stdin: undefined }));
      assert.deepStrictEqual(result, { code: 0, stdout: fs.realpathSync(nested), stderr: "" });
    });
  });

  test("malformed HTTP headers, methods and URLs fail before sending a request", async () => {
    await withRoot(async (root) => {
      const execute = createRequestExecutor(hostFor(root));
      const request = { ticket: "h", kind: "http", method: "GET", url: "http://127.0.0.1:1/" };
      for (const headers of ["bad", Array(65).fill({ name: "x", value: "y" }), [null],
        [{ name: 1, value: "v" }], [{ name: "X", value: 5 }], [{ name: "", value: "v" }],
        [{ name: "X", value: "x".repeat(8193) }]]) {
        await assert.rejects(execute({ ...request, headers }), /HTTP header/);
      }
      await assert.rejects(execute({ ...request, method: "POST" }), /Unsupported package HTTP method/);
      await assert.rejects(execute({ ...request, url: "" }), /non-empty string/);
      await assert.rejects(execute({ ...request, url: "x".repeat(8193) }), /too long/);
    });
  });


  test("process.input runs a bounded executable without a shell and returns raw stdout", async () => {
    const root = temporary();
    const script = path.join(root, "echo.cjs");
    fs.writeFileSync(script, "let s='';process.stdin.on('data',c=>s+=c);process.stdin.on('end',()=>process.stdout.write(s));");
    try {
      const result = await createRequestExecutor(hostFor(root))(processRequest(root, script)) as { code: number; stdout: string };
      assert.strictEqual(result.code, 0);
      assert.strictEqual(result.stdout, "hello from stdin\n");
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test("reopened panels serialize work for the same project", async () => {
    const root = temporary();
    const script = path.join(root, "sequence.cjs");
    const log = path.join(root, "sequence");
    fs.writeFileSync(script, "const fs=require('fs'),p=__dirname+'/sequence';fs.appendFileSync(p,'start\\n');setTimeout(()=>fs.appendFileSync(p,'end\\n'),50);");
    try {
      const request = processRequest(root, script);
      await Promise.all([createRequestExecutor(hostFor(root))(request), createRequestExecutor(hostFor(root))(request)]);
      assert.deepStrictEqual(fs.readFileSync(log, "utf8").trim().split("\n"), ["start", "end", "start", "end"]);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test("queued package work rechecks trust before starting a process", async () => {
    const root = temporary();
    let trusted = true;
    const host = { ...hostFor(root), get trusted() { return trusted; } };
    const execute = createRequestExecutor(host, { serialize: async (_key, task) => {
      trusted = false;
      return task();
    } });
    try {
      await assert.rejects(execute(processRequest(root, "unused")), /Trust this workspace/);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test("a dirty manifest or lock blocks the process before it starts", async () => {
    const root = temporary();
    try {
      for (const file of [path.join(root, "osprey.toml"), path.join(root, "osprey.lock")]) {
        await assert.rejects(createRequestExecutor(hostFor(root, [file]))(processRequest(root, "unused")), /Save or revert/);
      }
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test("the application may not name an executable or escape the workspace", async () => {
    const root = temporary();
    const outside = temporary();
    try {
      const request = processRequest(root, "unused");
      const wrong = { ...request, arguments: { ...(request.arguments as object), executable: "/bin/sh" } };
      await assert.rejects(createRequestExecutor(hostFor(root))(wrong), /Unknown package executable/);
      await assert.rejects(createRequestExecutor(hostFor(outside))(request), /outside this editor's workspace/);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
      fs.rmSync(outside, { recursive: true, force: true });
    }
  });

  test("HTTP refuses credentials, non-HTTPS origins and unknown capabilities", async () => {
    const root = temporary();
    try {
      const execute = createRequestExecutor(hostFor(root));
      await assert.rejects(execute({ ticket: "h", kind: "http", method: "GET", url: "https://user:pw@example.test/", headers: [], limit: 1024, timeout: 1000 }), /credentials/);
      await assert.rejects(execute({ ticket: "h", kind: "http", method: "GET", url: "http://example.test/", headers: [], limit: 1024, timeout: 1000 }), /HTTPS/);
      await assert.rejects(execute({ ticket: "h", kind: "http", method: "GET", url: "https://example.test/", headers: [{ name: "Authorization", value: "Bearer x" }], limit: 1024, timeout: 1000 }), /credentials/);
      await assert.rejects(execute({ ticket: "h", kind: "http", method: "GET", url: "http://127.0.0.1:1/", body: "ignored?", headers: [], limit: 1024, timeout: 1000 }), /cannot carry a body/);
      await assert.rejects(execute({ ticket: "h", kind: "host-call", namespace: "other", operation: "workspace.snapshot", arguments: {} }), /Unknown host namespace/);
      await assert.rejects(execute({ ticket: "h", kind: "host-call", namespace: "vscode", operation: "not.a.capability", arguments: {} }), /Unsupported VS Code capability/);
      await assert.rejects(execute({ ticket: "x", kind: "unknown" }), /Unsupported native capability/);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });

  test("loopback HTTP is bounded and returns its status and body", async () => {
    await withRoot(async (root) => withServer(async (url, headers) => {
      const execute = createRequestExecutor(hostFor(root));
      const request = { ticket: "h", kind: "http", method: "GET", url, headers: [], limit: 1024, timeout: 5000 };
      const result = await execute(request) as { status: number; body: string };
      assert.strictEqual(result.status, 200);
      assert.deepStrictEqual(JSON.parse(result.body), { ok: true });
      await assert.rejects(execute({ ...request, limit: 2 }), /exceeded its bound/);
      assert.deepStrictEqual(await execute({ ...request, method: "HEAD", headers: undefined }), { status: 200, body: "" });
      assert.strictEqual(headers().accept, "application/json");
      await execute({ ...request, headers: [{ name: "X-Probe", value: "forwarded" }] });
      assert.strictEqual(headers()["x-probe"], "forwarded");
      await assert.rejects(execute({ ...request, url: url + "interrupt" }), /interrupted|aborted/);
      await assert.rejects(execute({ ...request, url: url + "stall", timeout: 100 }), /timed out/);
    }));
  });
});

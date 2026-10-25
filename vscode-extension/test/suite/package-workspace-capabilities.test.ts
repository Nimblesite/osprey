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

suite("package workspace capabilities", () => {
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
    const root = temporary();
    const server = http.createServer((_request, response) => {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(JSON.stringify({ ok: true }));
    });
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    const port = typeof address === "object" && address ? address.port : 0;
    try {
      const result = await createRequestExecutor(hostFor(root))({
        ticket: "h", kind: "http", method: "GET", url: `http://127.0.0.1:${port}/`, headers: [], limit: 1024, timeout: 5000,
      }) as { status: number; body: string };
      assert.strictEqual(result.status, 200);
      assert.deepStrictEqual(JSON.parse(result.body), { ok: true });
      await assert.rejects(createRequestExecutor(hostFor(root))({
        ticket: "h", kind: "http", method: "GET", url: `http://127.0.0.1:${port}/`, headers: [], limit: 2, timeout: 5000,
      }), /exceeded its bound/);
    } finally {
      await new Promise<void>((resolve) => server.close(() => resolve()));
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});

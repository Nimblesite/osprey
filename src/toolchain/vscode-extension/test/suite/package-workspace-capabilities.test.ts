import * as assert from "assert";
import * as fs from "fs";
import * as http from "http";
import * as os from "os";
import * as path from "path";
import { pathToFileURL } from "url";
import { Ask } from "../../client/src/osprey-ui/asks";
import {
  ANSWERED_CAPABILITIES,
  createRequestExecutor,
  type CapabilityHost,
} from "../../client/src/osprey-ui/capabilities";

const HOST_CALL = 11;
const HTTP = 10;

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

type Field = readonly [number, number, string];

function ask(code: number, ticket: string, fields: readonly Field[]): Ask {
  return new Ask(code, ticket, fields.map(([field, number, text]) => ({ field, number, text })));
}

function pairs(field: number, values: readonly (readonly [string, string])[]): Field[] {
  return values.flatMap(([name, value]) => [[field, 0, name] as const, [field, 0, value] as const]);
}

interface ProcessOptions {
  readonly executable?: string;
  readonly argv?: readonly string[];
  readonly stdin?: string;
  readonly workspace?: string;
  readonly timeout?: number;
  readonly limit?: number;
}

/** A host-call ask that runs the package executable, as `Requests::fields` numbers it. */
function processAsk(root: string, script: string, options: ProcessOptions = {}): Ask {
  const arguments_: [string, string][] = [
    ["executable", options.executable ?? "osprey-pkg"],
    ["workspace", options.workspace ?? pathToFileURL(root).toString()],
    ...(options.argv ?? [script]).map((argument): [string, string] => ["argv", argument]),
    ...(options.stdin === undefined ? [] : [["stdin", options.stdin] as [string, string]]),
  ];
  const fields: Field[] = [[1, 0, "vscode"], [2, 0, "process.input"], ...pairs(3, arguments_)];
  if (options.timeout !== undefined) fields.push([4, options.timeout, ""]);
  if (options.limit !== undefined) fields.push([5, options.limit, ""]);
  return ask(HOST_CALL, "run:1", fields);
}

function httpAsk(url: string, options: { readonly method?: string; readonly headers?: readonly (readonly [string, string])[];
  readonly body?: string; readonly limit?: number; readonly deadline?: number } = {}): Ask {
  const fields: Field[] = [[1, 0, options.method ?? "GET"], [2, 0, url], ...pairs(3, options.headers ?? [])];
  if (options.body !== undefined) fields.push([4, 0, options.body]);
  if (options.limit !== undefined) fields.push([5, options.limit, ""]);
  if (options.deadline !== undefined) fields.push([6, options.deadline, ""]);
  fields.push([7, 0, "http"]);
  return ask(HTTP, "h", fields);
}

function hostCall(namespace: string, operation: string, arguments_: readonly (readonly [string, string])[] = []): Ask {
  return ask(HOST_CALL, "h", [[1, 0, namespace], [2, 0, operation], ...pairs(3, arguments_)]);
}

async function withRoot(action: (root: string) => Promise<void>): Promise<void> {
  const root = temporary();
  try {
    await action(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
}

function httpFixture(request: http.IncomingMessage, response: http.ServerResponse): void {
  if (request.url === "/stall") return;
  if (request.url === "/unavailable") {
    response.writeHead(503);
    response.end("retry later");
    return;
  }
  response.writeHead(200, { "content-type": "application/json" });
  if (request.url === "/interrupt") {
    response.write("partial");
    setImmediate(() => response.destroy());
  } else response.end(JSON.stringify({ ok: true }));
}

async function withServer(action: (url: string, headers: () => http.IncomingHttpHeaders) => Promise<void>): Promise<void> {
  let received: http.IncomingHttpHeaders = {};
  const server = http.createServer((request, response) => {
    received = request.headers;
    httpFixture(request, response);
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  assert.ok(address && typeof address === "object");
  try {
    await action(`http://127.0.0.1:${address.port}/`, () => received);
  } finally {
    server.closeAllConnections();
    await new Promise<void>((resolve) => server.close(() => resolve()));
  }
}

suite("package workspace capabilities", () => {
  test("untrusted, relative, missing and absent executables are refused before launch", async () => {
    await withRoot(async (root) => {
      const cases: [Partial<CapabilityHost>, RegExp][] = [
        [{ trusted: false }, /Trust this workspace/],
        [{ configuredExecutable: "relative" }, /absolute path/],
        [{ configuredExecutable: path.join(root, "missing") }, /does not exist/],
        [{ configuredExecutable: "", bundledExecutable: "" }, /not bundled or configured/],
      ];
      for (const [fields, expected] of cases) {
        await assert.rejects(createRequestExecutor({ ...hostFor(root), ...fields })(processAsk(root, "unused")), expected);
      }
    });
  });

  test("malformed and non-directory workspace addresses cannot launch commands", async () => {
    await withRoot(async (root) => {
      const file = path.join(root, "file");
      fs.writeFileSync(file, "file");
      for (const [workspace, error] of [
        ["invalid", /not a URI/],
        ["https://example.test/", /plain file URI/],
        [pathToFileURL(path.join(root, "missing")).toString(), /does not exist/],
        [pathToFileURL(file).toString(), /not a directory/],
      ] as const) {
        await assert.rejects(createRequestExecutor(hostFor(root))(processAsk(root, "unused", { workspace })), error);
      }
    });
  });

  test("process bounds reject malformed arguments, input and limits", async () => {
    await withRoot(async (root) => {
      const execute = createRequestExecutor(hostFor(root));
      const workspace = pathToFileURL(root).toString();
      const tooMany: [string, string][] = [["executable", "osprey-pkg"], ["workspace", workspace],
        ...Array.from({ length: 64 }, (): [string, string] => ["argv", "x"])];
      const malformed: Field[][] = [
        [[1, 0, "vscode"], [2, 0, "process.input"], [3, 0, "executable"]],
        [[1, 0, "vscode"], [2, 0, "process.input"], ...pairs(3, tooMany)],
        [[1, 0, "vscode"], [2, 0, "process.input"], ...pairs(3, [["", "v"]])],
        [[1, 0, "vscode"], [2, 0, "process.input"], ...pairs(3, [["argv".repeat(9000), "x"]])],
        // Input crosses as one text pair; an over-bound stdin is rejected before launch.
        [[1, 0, "vscode"], [2, 0, "process.input"], ...pairs(3, [["stdin", "x".repeat(1024 * 1024 + 1)]])],
      ];
      for (const fields of malformed) {
        await assert.rejects(execute(ask(HOST_CALL, "t", fields)), /malformed arguments|out-of-bounds arguments|Unknown package executable|stdin must be at most/);
      }
      for (const [field, value] of [[4, 0], [4, 120001], [5, 16 * 1024 * 1024 + 1]] as const) {
        const fields: Field[] = [[1, 0, "vscode"], [2, 0, "process.input"],
          ...pairs(3, [["executable", "osprey-pkg"], ["workspace", pathToFileURL(root).toString()]])];
        fields.push([field, value, ""]);
        await assert.rejects(execute(ask(HOST_CALL, "t", fields)), /between 1 and/);
      }
      await assert.rejects(execute(ask(HOST_CALL, "t", [[1, 0, "vscode"], [2, 0, "process.input"]])), /missing its arguments/);
      await assert.rejects(execute(ask(HOST_CALL, "t", [[1, 0, "vscode"], [2, 0, "process.input"], ...pairs(3, [["executable", "osprey-pkg"]])])), /workspace/);
      await assert.rejects(execute(processAsk(root, "unused", { stdin: "x".repeat(1024 * 1024 + 1) })), /out-of-bounds arguments name or value/);
      await assert.rejects(execute(hostCall("vscode", "process.input", [["workspace", workspace]])), /Unknown package executable/);
    });
  });

  test("process bounds reject malformed arguments, input and limits (extra: a bound-sized stdin reaches the real executable)", async () => {
    await withRoot(async (root) => {
      const execute = createRequestExecutor({ ...hostFor(root), configuredExecutable: "", bundledExecutable: process.execPath });
      const result = await execute(processAsk(root, "", { argv: ["-e", "let n=0;process.stdin.on(\"data\",c=>n+=c.length);process.stdin.on(\"end\",()=>process.stdout.write(String(n)))"],
        stdin: "x".repeat(1024 * 1024) }));
      assert.strictEqual(result.detail, String(1024 * 1024));
    });
  });

  test("bundled executables and omitted process options preserve real output and numeric exits", async () => {
    await withRoot(async (root) => {
      const execute = createRequestExecutor({ ...hostFor(root), configuredExecutable: "", bundledExecutable: process.execPath });
      const result = await execute(processAsk(root, "", { argv: [], stdin: 'process.stdout.write("default input")' }));
      assert.strictEqual(result.detail, "default input");
      // A crashed executable's raw output still arrives; the report's semantic status names the failure.
      const failed = await execute(processAsk(root, "", { argv: ["-e", 'process.stdout.write("partial");process.exit(3)'] }));
      assert.strictEqual(failed.detail, "partial");
      await assert.rejects(execute(processAsk(root, "", { argv: ["-e", "setTimeout(()=>{},9999)"], timeout: 100 })), /Package command failed/);
    });
  });

  test("nested workspace projects run despite unrelated unsaved files with missing parents", async () => {
    await withRoot(async (root) => {
      const nested = path.join(root, "nested");
      fs.mkdirSync(nested);
      const execute = createRequestExecutor(hostFor(root, [path.join(root, "missing", "note.osp")]));
      const result = await execute(processAsk(nested, "", { argv: ["-e", "process.stdout.write(process.cwd())"] }));
      assert.strictEqual(result.detail, fs.realpathSync(nested));
    });
  });

  test("malformed HTTP headers, methods and URLs fail before sending a request", async () => {
    await withRoot(async (root) => {
      const execute = createRequestExecutor(hostFor(root));
      const base: Field[] = [[1, 0, "GET"], [2, 0, "http://127.0.0.1:1/"]];
      for (const fields of [
        [[...base, [3, 0, "only-a-name"]] as Field[]],
        [[...base, ...pairs(3, Array.from({ length: 65 }, (): [string, string] => ["x", "y"]))] as Field[]],
        [[...base, ...pairs(3, [["", "v"]])] as Field[]],
        [[...base, ...pairs(3, [["X", "x".repeat(8193)]])] as Field[]],
      ]) {
        await assert.rejects(execute(ask(HTTP, "h", fields[0]!)), /headers/);
      }
      await assert.rejects(execute(ask(HTTP, "h", [[1, 0, "POST"], [2, 0, "http://127.0.0.1:1/"]])), /Unsupported package HTTP method/);
      await assert.rejects(execute(ask(HTTP, "h", [[1, 0, "GET"]])), /no valid url/);
      await assert.rejects(execute(ask(HTTP, "h", [[1, 0, "GET"], [2, 0, "x".repeat(8193)]])), /too long/);
    });
  });

  test("process.input runs a bounded executable without a shell and returns raw stdout", async () => {
    await withRoot(async (root) => {
      const script = path.join(root, "echo.cjs");
      fs.writeFileSync(script, "let s='';process.stdin.on('data',c=>s+=c);process.stdin.on('end',()=>process.stdout.write(s));");
      const result = await createRequestExecutor(hostFor(root))(processAsk(root, script, { stdin: "hello from stdin\n" }));
      assert.strictEqual(result.detail, "hello from stdin\n");
    });
  });

  test("reopened panels serialize work for the same project", async () => {
    await withRoot(async (root) => {
      const script = path.join(root, "sequence.cjs");
      const log = path.join(root, "sequence");
      fs.writeFileSync(script, "const fs=require('fs'),p=__dirname+'/sequence';fs.appendFileSync(p,'start\\n');setTimeout(()=>fs.appendFileSync(p,'end\\n'),50);");
      const request = processAsk(root, script);
      await Promise.all([
        createRequestExecutor(hostFor(root))(request),
        createRequestExecutor(hostFor(root))(request),
      ]);
      assert.deepStrictEqual(fs.readFileSync(log, "utf8").trim().split("\n"), ["start", "end", "start", "end"]);
    });
  });

  test("queued package work rechecks trust before starting a process", async () => {
    await withRoot(async (root) => {
      let trusted = true;
      const host = { ...hostFor(root), get trusted() { return trusted; } };
      const execute = createRequestExecutor(host, {
        serialize: async (_key, task) => { trusted = false; return task(); },
      });
      await assert.rejects(execute(processAsk(root, "unused")), /Trust this workspace/);
    });
  });

  test("queued package work rejects invalidated arguments without starting a process", async () => {
    await withRoot(async (root) => {
      const outside = temporary();
      const marker = path.join(root, "started");
      const script = path.join(root, "marker.cjs");
      fs.writeFileSync(script, `require("fs").writeFileSync(${JSON.stringify(marker)}, "started");`);
      try {
        const execute = createRequestExecutor(hostFor(root));
        const first = execute(processAsk(root, "", { argv: ["-e", ""] }));
        const queued = execute(processAsk(root, script, { workspace: pathToFileURL(outside).toString() }));
        await Promise.all([first, assert.rejects(queued, /outside this editor's workspace/)]);
        assert.strictEqual(fs.existsSync(marker), false);
      } finally {
        fs.rmSync(outside, { recursive: true, force: true });
      }
    });
  });

  test("a dirty manifest or lock blocks the process before it starts", async () => {
    await withRoot(async (root) => {
      for (const file of [path.join(root, "osprey.toml"), path.join(root, "osprey.lock")]) {
        await assert.rejects(createRequestExecutor(hostFor(root, [file]))(processAsk(root, "unused")), /Save or revert/);
      }
    });
  });

  test("the application may not name an executable or escape the workspace", async () => {
    const root = temporary();
    const outside = temporary();
    try {
      await assert.rejects(createRequestExecutor(hostFor(root))(processAsk(root, "unused", { executable: "/bin/sh" })), /Unknown package executable/);
      await assert.rejects(createRequestExecutor(hostFor(outside))(processAsk(root, "unused")), /outside this editor's workspace/);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
      fs.rmSync(outside, { recursive: true, force: true });
    }
  });

  test("HTTP refuses credentials, non-HTTPS origins and unknown capabilities", async () => {
    await withRoot(async (root) => {
      const execute = createRequestExecutor(hostFor(root));
      await assert.rejects(execute(httpAsk("https://user:pw@example.test/")), /credentials/);
      await assert.rejects(execute(httpAsk("http://example.test/")), /HTTPS/);
      await assert.rejects(execute(httpAsk("https://example.test/", { headers: [["Authorization", "Bearer x"]] })), /credentials/);
      await assert.rejects(execute(httpAsk("http://127.0.0.1:1/", { body: "ignored?" })), /cannot carry a body/);
      await assert.rejects(execute(hostCall("other", "workspace.snapshot")), /Unknown host namespace/);
      await assert.rejects(execute(hostCall("vscode", "not.a.capability")), /Unsupported VS Code capability/);
      await assert.rejects(execute(ask(26, "x", [])), /Unknown capability code/);
    });
  });

  test("host requests validate document positions, configuration and view names before dispatch", async () => {
    await withRoot(async (root) => {
      let opened = 0;
      const execute = createRequestExecutor({
        ...hostFor(root),
        openDocument: async (uri, line, column) => { opened++; return { uri, line, column }; },
        readConfiguration: (section, workspace) => ({ section, workspace }),
        revealView: async (view) => ({ view }),
      });
      const uri = pathToFileURL(path.join(root, "proof ∧.osp")).toString();
      await assert.rejects(execute(hostCall("vscode", "document.open")), /missing its arguments/);
      for (const arguments_ of [[["ignored", "value"]], [["uri", ""]], [["uri", "relative.osp"]], [["uri", "https://example.test/"]]] as const) {
        await assert.rejects(execute(hostCall("vscode", "document.open", arguments_)), /requires a file resource/);
      }
      for (const [line, column] of [[undefined, undefined], ["7", undefined], [undefined, "3"], [String(Number.MAX_SAFE_INTEGER), "1"]] as const) {
        const arguments_: [string, string][] = [["uri", uri]];
        if (line !== undefined) arguments_.push(["line", line]);
        if (column !== undefined) arguments_.push(["column", column]);
        const result = await execute(hostCall("vscode", "document.open", arguments_));
        assert.deepStrictEqual(JSON.parse(result.detail), { uri, line: Number(line ?? 1), column: Number(column ?? 1) });
      }
      for (const field of ["line", "column"]) {
        for (const value of ["0", "-1", "1.5", "NaN", "Infinity", "", String(Number.MAX_SAFE_INTEGER + 1)]) {
          await assert.rejects(execute(hostCall("vscode", "document.open", [["uri", uri], [field, value]])), /positive whole number/);
        }
      }
      assert.strictEqual(opened, 4, "invalid positions never reach the document host");
      for (const arguments_ of [[], [["section", "editor"]], [["workspace", uri]], [["section", ""], ["workspace", uri]], [["section", "editor"], ["workspace", ""]]] as const) {
        await assert.rejects(execute(hostCall("vscode", "configuration.read", arguments_)), /missing its arguments/);
      }
      const configuration = await execute(hostCall("vscode", "configuration.read", [["section", "editor"], ["workspace", uri]]));
      assert.deepStrictEqual(JSON.parse(configuration.detail), { section: "editor", workspace: uri });
      for (const arguments_ of [[], [["view", ""]]] as const) {
        await assert.rejects(execute(hostCall("vscode", "webview.reveal", arguments_)), /missing its arguments/);
      }
      assert.deepStrictEqual(JSON.parse((await execute(hostCall("vscode", "webview.reveal", [["view", "osprey.proof"]]))).detail), { view: "osprey.proof" });
      assert.deepStrictEqual(JSON.parse((await execute(hostCall("vscode", "workspace.snapshot"))).detail), { trusted: true });
    });
  });

  test("every framework capability is answered here or refused by name", async () => {
    // Every `Capability` constructor in src/osprey/packages/native/src/capability.ospml, as the encoder names it
    // ([NATIVE-HOSTS-COVERAGE]): a capability added to the framework fails here until this host answers or refuses it.
    const answered = new Set(ANSWERED_CAPABILITIES);
    const source = fs.readFileSync(path.resolve(__dirname, "..", "..", "..", "..", "..", "osprey", "packages", "native", "src", "capability.ospml"), "utf8").replace(/\r/g, "");
    const block = source.slice(source.indexOf("type Capability ="));
    const body = block.slice(0, block.search(/\n\S/));
    const kinds = [...body.matchAll(/^ {4}([A-Z]\w*)$/gm)].map((match) =>
      match[1] === "ReadPropertyLists" ? "read-plists" : match[1].replace(/(?<!^)([A-Z])/g, "-$1").toLowerCase());
    assert.strictEqual(kinds.length, 25, kinds.join(","));
    const refused = kinds.filter((kind) => !answered.has(kind));
    assert.deepStrictEqual([...answered, ...refused].sort(), [...kinds].sort());
    await withRoot(async (root) => {
      const execute = createRequestExecutor(hostFor(root));
      for (const kind of refused) {
        const code = kinds.indexOf(kind) + 1;
        await assert.rejects(execute(ask(code, kind, [])), (error) =>
          error instanceof Error && error.message === `Unsupported native capability: ${kind}`);
      }
    });
  });

  test("loopback HTTP is bounded and returns its raw body", async () => {
    await withRoot(async (root) => withServer(async (url, headers) => {
      const execute = createRequestExecutor(hostFor(root));
      const request = httpAsk(url, { deadline: 5000 });
      const result = await execute(request);
      assert.deepStrictEqual(JSON.parse(result.detail), { ok: true });
      await assert.rejects(execute(httpAsk(url, { deadline: 5000, limit: 2 })), /exceeded its bound/);
      assert.strictEqual((await execute(httpAsk(url, { method: "HEAD", deadline: 5000 }))).detail, "");
      assert.strictEqual(headers().accept, "application/json");
      await execute(httpAsk(url, { headers: [["X-Probe", "forwarded"]], deadline: 5000 }));
      assert.strictEqual(headers()["x-probe"], "forwarded");
      await assert.rejects(execute(httpAsk(url + "unavailable", { deadline: 5000 })), /Registry returned HTTP 503/);
      await assert.rejects(execute(httpAsk(url + "interrupt", { deadline: 5000 })), /interrupted|aborted/);
      await assert.rejects(execute(httpAsk(url + "stall", { deadline: 100 })), /timed out/);
    }));
  });
});

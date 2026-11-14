// Implements [VSCODE-SDK-CAPABILITIES] and [UI-EFFECTS]: the host executes only bounded
// capability requests read from a trusted frame's ask rows. It maps a logical package executable
// to a configured or bundled binary, never a path supplied by the application, and never uses a
// shell.

import { execFile } from "child_process";
import { createProjectSerializer } from "../keyed-serializer";
import * as fs from "fs";
import * as http from "http";
import * as https from "https";
import * as path from "path";
import { fileURLToPath } from "url";
import { document, type Ask, type Completion } from "./asks";
import { inside } from "./local-files";

const DEFAULT_TIMEOUT = 120000;
const DEFAULT_LIMIT = 16 * 1024 * 1024;
const MAX_STDIN = 1024 * 1024;
const MAX_HEADERS = 64;
const MAX_HEADER_LENGTH = 8192;
const MANIFEST = "osprey.toml";
const LOCK = "osprey.lock";
const LOGICAL_EXECUTABLE = "osprey-pkg";
const serializeProcesses = createProjectSerializer();

export interface CapabilityHost {
  readonly trusted: boolean;
  readonly workspaceFolders: readonly string[];
  readonly configuredExecutable: string;
  readonly bundledExecutable: string;
  dirtyFiles(): readonly string[];
  workspaceSnapshot(): unknown;
  openDocument(uri: string, line: number, column: number): Promise<unknown>;
  readConfiguration(section: string, workspaceUri: string): unknown;
  revealView(view: string): Promise<unknown>;
}

export interface ExecutorOptions {
  readonly serialize?: <T>(key: string, task: () => Promise<T>) => Promise<T>;
}

/** A required text at argument `field`; an ask that carries none is refused. */
function required(ask: Ask, field: number, name: string): string {
  const values = ask.texts(field);
  if (values.length === 0 || values[0] === "") throw new Error(`${ask.kind} has no valid ${name}`);
  return values[0]!;
}

/** The named arguments every host-call operation carries; an ask without them is refused. */
function argumentsOf(ask: Ask): readonly { name: string; value: string }[] {
  if (!ask.fields.some((item) => item.field === 3)) throw new Error(`${required(ask, 2, "operation")} is missing its arguments`);
  return named(ask, 3, "arguments");
}

/** A required whole number at argument `field`, bounded by `most`; omission may have a fallback. */
function bounded(ask: Ask, field: number, name: string, most: number, fallback?: number): number {
  const present = ask.fields.some((item) => item.field === field);
  if (!present && fallback !== undefined) return fallback;
  const value = ask.number(field);
  if (!present || !Number.isSafeInteger(value) || value <= 0 || value > most) {
    throw new Error(`${ask.kind} has no valid ${name} between 1 and ${most}`);
  }
  return value;
}

/** The texts at `field` taken two at a time: a header, an environment variable or an argument. */
function named(ask: Ask, field: number, name: string, bound = MAX_STDIN): readonly { name: string; value: string }[] {
  const values = ask.texts(field);
  if (values.length > MAX_HEADERS * 2 || values.length % 2 !== 0) throw new Error(`${ask.kind} has malformed ${name}`);
  const pairs = ask.pairs(field);
  for (const pair of pairs) {
    if (pair.name === "" || pair.name.length > MAX_HEADER_LENGTH || pair.value.length > bound) {
      throw new Error(`${ask.kind} has an out-of-bounds ${name} name or value`);
    }
  }
  return pairs;
}

function first(pairs: readonly { name: string; value: string }[], name: string): string | undefined {
  return pairs.find((pair) => pair.name === name)?.value;
}

function every(pairs: readonly { name: string; value: string }[], name: string): string[] {
  return pairs.filter((pair) => pair.name === name).map((pair) => pair.value);
}

function positive(value: string | undefined, name: string, fallback: number): number {
  if (value === undefined) return fallback;
  const number = Number(value);
  if (!Number.isSafeInteger(number) || number < 1) {
    throw new Error(`Request field ${name} must be a positive whole number`);
  }
  return number;
}

function directoryFor(workspaceUri: string, host: CapabilityHost): string {
  let url: URL;
  try {
    url = new URL(workspaceUri);
  } catch {
    throw new Error("The request workspace is not a URI");
  }
  if (url.protocol !== "file:" || url.username !== "" || url.password !== "" || url.search !== "" || url.hash !== "") {
    throw new Error("The request workspace must be a plain file URI");
  }
  let candidate: string;
  try {
    candidate = fs.realpathSync(fileURLToPath(url));
  } catch {
    throw new Error(`The request workspace does not exist: ${fileURLToPath(url)}`);
  }
  if (!fs.statSync(candidate).isDirectory()) throw new Error("The request workspace is not a directory");
  const roots = host.workspaceFolders.map((folder) => fs.realpathSync(folder));
  if (!roots.some((root) => inside(root, candidate))) throw new Error("The request workspace is outside this editor's workspace");
  return candidate;
}

function executableFor(host: CapabilityHost): string {
  if (!host.trusted) throw new Error("Trust this workspace before running package commands");
  const configured = host.configuredExecutable;
  if (configured !== "") {
    if (!path.isAbsolute(configured)) throw new Error("The configured package executable must be an absolute path");
    if (fs.existsSync(configured)) return configured;
    throw new Error(`The configured package executable does not exist: ${configured}`);
  }
  if (host.bundledExecutable !== "" && fs.existsSync(host.bundledExecutable)) return host.bundledExecutable;
  throw new Error("The Osprey package executable is not bundled or configured");
}

function canonical(file: string): string {
  try {
    return fs.realpathSync(file);
  } catch {
    try {
      return path.join(fs.realpathSync(path.dirname(file)), path.basename(file));
    } catch {
      return path.resolve(file);
    }
  }
}

function dirtyIn(cwd: string, host: CapabilityHost): boolean {
  const guarded = new Set([path.join(cwd, MANIFEST), path.join(cwd, LOCK)].map(canonical));
  return host.dirtyFiles().some((file) => guarded.has(canonical(file)));
}

interface ProcessPlan {
  readonly executable: string;
  readonly argv: readonly string[];
  readonly stdin: string;
  readonly cwd: string;
  readonly timeout: number;
  readonly limit: number;
}

function processPlan(ask: Ask, host: CapabilityHost): ProcessPlan {
  const arguments_ = argumentsOf(ask);
  const logical = first(arguments_, "executable") ?? "";
  if (logical !== LOGICAL_EXECUTABLE) throw new Error(`Unknown package executable: ${logical}`);
  const cwd = directoryFor(first(arguments_, "workspace") ?? "", host);
  if (dirtyIn(cwd, host)) throw new Error(`Save or revert unsaved ${MANIFEST} and ${LOCK} changes before running package commands`);
  const stdin = first(arguments_, "stdin") ?? "";
  return {
    executable: executableFor(host),
    argv: every(arguments_, "argv"),
    stdin,
    cwd,
    timeout: bounded(ask, 4, "timeout", DEFAULT_TIMEOUT, DEFAULT_TIMEOUT),
    limit: bounded(ask, 5, "limit", DEFAULT_LIMIT, 4 * 1024 * 1024),
  };
}

function runProcess(plan: ProcessPlan): Promise<{ readonly code: number; readonly stdout: string; readonly stderr: string }> {
  return new Promise((resolve, reject) => {
    const child = execFile(plan.executable, [...plan.argv], {
      cwd: plan.cwd, timeout: plan.timeout, maxBuffer: plan.limit, windowsHide: true, encoding: "utf8", shell: false,
    }, (error, stdout, stderr) => {
      if (error && (error.killed || error.signal || typeof (error as NodeJS.ErrnoException).code === "string")) {
        reject(new Error(`Package command failed: ${error.message}`));
        return;
      }
      const code = error && typeof error.code === "number" ? error.code : 0;
      resolve({ code, stdout: String(stdout ?? ""), stderr: String(stderr ?? "") });
    });
    child.stdin?.once("error", reject);
    child.stdin?.end(plan.stdin, () => undefined);
  });
}

function forbiddenHeader(name: string): boolean {
  return /authorization|cookie|token|secret|api[-_]?key|proxy-authorization/i.test(name);
}

function headersFor(pairs: readonly { name: string; value: string }[]): http.OutgoingHttpHeaders {
  if (pairs.length === 0) return { Accept: "application/json" };
  const headers: http.OutgoingHttpHeaders = {};
  for (const pair of pairs) {
    if (forbiddenHeader(pair.name)) throw new Error(`HTTP header ${pair.name} would send credentials and is refused`);
    headers[pair.name] = pair.value;
  }
  return headers;
}

function httpUrl(value: string): URL {
  if (value.length > MAX_HEADER_LENGTH) throw new Error("Package HTTP URL is too long");
  const url = new URL(value);
  const loopback = ["127.0.0.1", "localhost", "[::1]", "::1"].includes(url.hostname);
  if (url.username || url.password || url.hash || (url.protocol !== "https:" && !(url.protocol === "http:" && loopback))) {
    throw new Error("Package HTTP requests require an HTTPS URL without credentials (HTTP only on loopback)");
  }
  return url;
}

function collect(response: http.IncomingMessage, limit: number, resolve: (value: unknown) => void, reject: (reason: Error) => void): void {
  const chunks: Buffer[] = [];
  let bytes = 0;
  let failed = false;
  const fail = (reason: Error): void => {
    if (failed) return;
    failed = true;
    reject(reason);
  };
  response.on("data", (chunk: Buffer) => {
    if (failed) return;
    bytes += chunk.length;
    if (bytes > limit) {
      fail(new Error("Package HTTP response exceeded its bound"));
      response.destroy();
      return;
    }
    chunks.push(chunk);
  });
  response.once("end", () => {
    if (failed) return;
    failed = true;
    resolve({ status: response.statusCode ?? 0, body: Buffer.concat(chunks).toString("utf8") });
  });
  response.once("error", fail);
  response.once("aborted", () => fail(new Error("Package HTTP response was interrupted")));
}

function httpRequest(ask: Ask): Promise<Completion> {
  const method = required(ask, 1, "method").toUpperCase();
  if (method !== "GET" && method !== "HEAD") throw new Error(`Unsupported package HTTP method: ${method}`);
  const body = ask.text(4);
  if (body !== "") throw new Error(`Package HTTP ${method} requests cannot carry a body`);
  const url = httpUrl(required(ask, 2, "url"));
  const limit = bounded(ask, 5, "limit", DEFAULT_LIMIT, 1024 * 1024);
  const timeout = bounded(ask, 6, "deadline", DEFAULT_TIMEOUT, 15000);
  const headers = headersFor(named(ask, 3, "headers", MAX_HEADER_LENGTH));
  return new Promise((resolve, reject) => {
    const requestObject = (url.protocol === "https:" ? https : http).request(url, { method, headers }, (response) => {
      collect(response, limit, (value) => {
        const { status, body } = value as { status: number; body: string };
        if (status < 200 || status >= 300) {
          reject(new Error(`Registry returned HTTP ${status}`));
          return;
        }
        // The raw body is the completion text; the ask's slice carries the same bytes for bulk reads.
        resolve({ detail: body, slices: [{ key: ask.text(7), text: body }] });
      }, reject);
    });
    const timer = setTimeout(() => requestObject.destroy(new Error("Package HTTP request timed out")), timeout);
    requestObject.once("close", () => clearTimeout(timer));
    requestObject.once("error", reject);
    requestObject.end();
  });
}

function openDocument(ask: Ask, host: CapabilityHost): Promise<unknown> {
  const arguments_ = argumentsOf(ask);
  const uri = first(arguments_, "uri") ?? "";
  if (!uri.startsWith("file:///")) throw new Error("document.open requires a file resource");
  return host.openDocument(uri, positive(first(arguments_, "line"), "line", 1), positive(first(arguments_, "column"), "column", 1));
}

function configuration(ask: Ask, host: CapabilityHost): unknown {
  const arguments_ = argumentsOf(ask);
  const section = first(arguments_, "section") ?? "";
  const workspace = first(arguments_, "workspace") ?? "";
  if (section === "" || workspace === "") throw new Error("configuration.read is missing its arguments");
  return host.readConfiguration(section, workspace);
}

function hostCall(ask: Ask, host: CapabilityHost): Promise<Completion> | Completion {
  const space = required(ask, 1, "namespace");
  if (space !== "vscode") throw new Error(`Unknown host namespace: ${space}`);
  const operation = required(ask, 2, "operation");
  if (operation === "workspace.snapshot") return document(host.workspaceSnapshot());
  if (operation === "document.open") return openDocument(ask, host).then(document);
  if (operation === "configuration.read") return document(configuration(ask, host));
  if (operation === "webview.reveal") return revealView(ask, host).then(document);
  throw new Error(`Unsupported VS Code capability: ${operation}`);
}

function revealView(ask: Ask, host: CapabilityHost): Promise<unknown> {
  const view = first(argumentsOf(ask), "view") ?? "";
  if (view === "") throw new Error("webview.reveal is missing its arguments");
  return host.revealView(view);
}

/** Capability kinds this webview answers ([NATIVE-HOSTS-COVERAGE]); any other kind fails explicitly. */
export const ANSWERED_CAPABILITIES: readonly string[] = ["http", "host-call"];

/** Execute one bounded capability from a checked ask. */
export function createRequestExecutor(host: CapabilityHost, options: ExecutorOptions = {}): (ask: Ask) => Promise<Completion> {
  const serialize = options.serialize ?? serializeProcesses;
  return async (ask) => {
    const kind = ask.kind;
    if (kind === "http") {
      // The completion text is the raw response body — "formats owned by the outside world … stay
      // the business of the adapter that speaks them" ([NATIVE-SHM-WHERE-SERIALIZATION-REMAINS]) —
      // and the body also lands in the host-held slice the ask names ([NATIVE-SHM-ASK]).
      return httpRequest(ask);
    }
    if (kind === "host-call") {
      if (required(ask, 2, "operation") === "process.input") {
        const plan = processPlan(ask, host);
        return serialize(plan.cwd, async () => {
          if (!host.trusted) throw new Error("Trust this workspace before running package commands");
          const run = await runProcess(plan);
          const stdout = run.stdout;
          const slice = ask.text(6);
          return slice === "" ? { detail: stdout } : { detail: stdout, slices: [{ key: slice, text: stdout }] };
        });
      }
      return hostCall(ask, host);
    }
    throw new Error(`Unsupported native capability: ${kind}`);
  };
}

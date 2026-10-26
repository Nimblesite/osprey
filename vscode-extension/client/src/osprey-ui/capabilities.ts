// Implements [VSCODE-SDK-CAPABILITIES] and [UI-EFFECTS]: the host executes
// only bounded requests that arrived in a trusted native envelope. It maps a
// logical package executable to a configured or bundled binary, never a path
// supplied by the application, and never uses a shell.

import { execFile } from "child_process";
import { createProjectSerializer } from "../keyed-serializer";
import * as fs from "fs";
import * as http from "http";
import * as https from "https";
import * as path from "path";
import { fileURLToPath } from "url";
import { record, type NativeRequest } from "./protocol";

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

function requiredString(value: unknown, name: string): string {
  if (typeof value !== "string" || value === "") throw new Error(`Request field ${name} must be a non-empty string`);
  return value;
}

function boundedInteger(value: unknown, name: string, most: number, fallback: number): number {
  if (value === undefined) return fallback;
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0 || value > most) {
    throw new Error(`Request field ${name} must be between 1 and ${most}`);
  }
  return value;
}

function stringList(value: unknown, name: string): string[] {
  if (value === undefined) return [];
  if (!Array.isArray(value) || value.length > 64 || !value.every((item) => typeof item === "string" && item.length <= 8192)) {
    throw new Error(`Request field ${name} must be a short array of strings`);
  }
  return value;
}


function inside(root: string, candidate: string): boolean {
  const relative = path.relative(root, candidate);
  return relative === "" || (!relative.startsWith("..") && !path.isAbsolute(relative));
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
  readonly argv: string[];
  readonly stdin: string;
  readonly cwd: string;
  readonly timeout: number;
  readonly limit: number;
}

function processPlan(request: NativeRequest, host: CapabilityHost): ProcessPlan {
  const args = request.arguments;
  if (!record(args)) throw new Error("process.input is missing its arguments");
  const logical = requiredString(args.executable, "executable");
  if (logical !== LOGICAL_EXECUTABLE) throw new Error(`Unknown package executable: ${logical}`);
  const cwd = directoryFor(requiredString(args.workspace, "workspace"), host);
  if (dirtyIn(cwd, host)) throw new Error(`Save or revert unsaved ${MANIFEST} and ${LOCK} changes before running package commands`);
  if (args.stdin !== undefined && typeof args.stdin !== "string") throw new Error("Request field stdin must be a string");
  const stdin = args.stdin ?? "";
  if (stdin.length > MAX_STDIN) throw new Error(`Request field stdin must be at most ${MAX_STDIN} characters`);
  return {
    executable: executableFor(host),
    argv: stringList(args.argv, "argv"),
    stdin,
    cwd,
    timeout: boundedInteger(request.timeout, "timeout", DEFAULT_TIMEOUT, DEFAULT_TIMEOUT),
    limit: boundedInteger(request.limit, "limit", DEFAULT_LIMIT, 4 * 1024 * 1024),
  };
}

function runProcess(plan: ProcessPlan): Promise<unknown> {
  return new Promise((resolve, reject) => {
    const child = execFile(plan.executable, plan.argv, {
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

function headersFor(value: unknown): http.OutgoingHttpHeaders {
  if (value === undefined) return { Accept: "application/json" };
  if (!Array.isArray(value) || value.length > MAX_HEADERS) throw new Error("HTTP headers must be a bounded array");
  const headers: http.OutgoingHttpHeaders = {};
  for (const item of value) {
    if (!record(item) || typeof item.name !== "string" || typeof item.value !== "string") throw new Error("HTTP header is malformed");
    if (item.name === "" || item.value.length > MAX_HEADER_LENGTH) throw new Error("HTTP header name or value is out of bounds");
    if (forbiddenHeader(item.name)) throw new Error(`HTTP header ${item.name} would send credentials and is refused`);
    headers[item.name] = item.value;
  }
  return headers;
}

function httpUrl(value: unknown): URL {
  const source = requiredString(value, "url");
  if (source.length > MAX_HEADER_LENGTH) throw new Error("Package HTTP URL is too long");
  const url = new URL(source);
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

function httpRequest(request: NativeRequest): Promise<unknown> {
  const method = requiredString(request.method, "method").toUpperCase();
  if (method !== "GET" && method !== "HEAD") throw new Error(`Unsupported package HTTP method: ${method}`);
  if (request.body !== undefined && request.body !== "") throw new Error(`Package HTTP ${method} requests cannot carry a body`);
  const url = httpUrl(request.url);
  const limit = boundedInteger(request.limit, "limit", DEFAULT_LIMIT, 1024 * 1024);
  const timeout = boundedInteger(request.timeout, "timeout", DEFAULT_TIMEOUT, 15000);
  return new Promise((resolve, reject) => {
    const requestObject = (url.protocol === "https:" ? https : http).request(url, { method, headers: headersFor(request.headers) }, (response) => collect(response, limit, resolve, reject));
    const timer = setTimeout(() => requestObject.destroy(new Error("Package HTTP request timed out")), timeout);
    requestObject.once("close", () => clearTimeout(timer));
    requestObject.once("error", reject);
    requestObject.end();
  });
}

function openDocument(request: NativeRequest, host: CapabilityHost): Promise<unknown> {
  const args = request.arguments;
  if (!record(args)) throw new Error("document.open is missing its arguments");
  const uri = requiredString(args.uri, "uri");
  if (!uri.startsWith("file:///")) throw new Error("document.open requires a file resource");
  const line = boundedInteger(args.line, "line", Number.MAX_SAFE_INTEGER, 1);
  const column = boundedInteger(args.column, "column", Number.MAX_SAFE_INTEGER, 1);
  return host.openDocument(uri, line, column);
}

function configuration(request: NativeRequest, host: CapabilityHost): unknown {
  const args = request.arguments;
  if (!record(args)) throw new Error("configuration.read is missing its arguments");
  return host.readConfiguration(requiredString(args.section, "section"), requiredString(args.workspace, "workspace"));
}

function hostCall(request: NativeRequest, host: CapabilityHost): Promise<unknown> | unknown {
  const space = requiredString(request.namespace, "namespace");
  if (space !== "vscode") throw new Error(`Unknown host namespace: ${space}`);
  const operation = requiredString(request.operation, "operation");
  if (operation === "workspace.snapshot") return host.workspaceSnapshot();
  if (operation === "document.open") return openDocument(request, host);
  if (operation === "configuration.read") return configuration(request, host);
  if (operation === "webview.reveal") return revealView(request, host);
  if (operation === "process.input") return runProcess(processPlan(request, host));
  throw new Error(`Unsupported VS Code capability: ${operation}`);
}

function revealView(request: NativeRequest, host: CapabilityHost): Promise<unknown> {
  const args = request.arguments;
  if (!record(args)) throw new Error("webview.reveal is missing its arguments");
  return host.revealView(requiredString(args.view, "view"));
}

/** Execute one bounded capability from a validated native envelope. */
export function createRequestExecutor(host: CapabilityHost, options: ExecutorOptions = {}): (request: NativeRequest) => Promise<unknown> {
  const serialize = options.serialize ?? serializeProcesses;
  return async (request) => {
    if (request.kind === "http") return httpRequest(request);
    if (request.kind === "host-call") {
      if (request.operation === "process.input") {
        const args = request.arguments;
        if (!record(args)) throw new Error("process.input is missing its arguments");
        const cwd = directoryFor(requiredString(args.workspace, "workspace"), host);
        return serialize(cwd, async () => hostCall(request, host));
      }
      return hostCall(request, host);
    }
    throw new Error(`Unsupported native capability: ${request.kind}`);
  };
}

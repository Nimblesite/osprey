// Implements [LESSONS-FILES] and [LESSONS-PROGRESS] host glue: bounded reads
// of approved bundled application assets and atomic progress persistence
// inside one rooted store directory. The executor grants no arbitrary
// filesystem, network or process access; unsupported requests fail
// explicitly, and bulk results travel as host-held slice deliveries instead
// of unbounded completion payloads. Optional document dialogs grant access
// only to the file the person explicitly chooses.
import * as fs from "node:fs";
import * as path from "node:path";
import { type NativeRequest } from "./protocol";
import { approvedFile, inside, boundedText, persist, PRIVATE_DIRECTORY_MODE } from "./local-files";
import { documentRequest, type DocumentDialogs } from "./document-capabilities";
import { requiredString as text, boundedInteger, stringList, messageOf } from "./wire";

/** The envelope slice bound from [UI-PROTOCOL]; no read or write exceeds it. */
const MAX_TEXT = 8 * 1024 * 1024;

/** One host-held slice the executor produced, kept under `key` until written again. */
export interface SliceDelivery {
  readonly key: string;
  readonly text: string;
}

/** An executor answer: its completion result beside the slice deliveries it owes the app. */
export interface LocalResponse {
  readonly result: unknown;
  readonly slices?: readonly SliceDelivery[];
}

export interface LocalExecutorOptions {
  readonly assetRoot: string;
  readonly storeRoot: string;
  readonly dialogs?: DocumentDialogs;
}

/** One approved asset: its text on success, or the fault that refused it. */
function fileText(assets: string, source: string, limit: number): { path: string; text: string; error: string } {
  try {
    return { path: source, text: boundedText(approvedFile(assets, source), limit), error: "" };
  } catch (error) {
    return { path: source, text: "", error: messageOf(error) };
  }
}

function readTexts(request: NativeRequest, assets: string): LocalResponse {
  const limit = boundedInteger(request.limit, "limit", MAX_TEXT);
  const key = text(request.slice, "slice");
  const files = stringList(request.paths, "paths", true).map((source) => fileText(assets, source, limit));
  const result = { files };
  const contents = JSON.stringify(result);
  if (Buffer.byteLength(contents, "utf8") > MAX_TEXT) throw new Error(`Read slice ${key} exceeds its byte bound`);
  return { result, slices: [{ key, text: contents }] };
}

/** A store name stays inside the root: no absolute paths, parent references or empty parts. */
function rooted(store: string, value: unknown): { name: string; target: string } {
  const name = typeof value === "string" ? value : "";
  if (name === "" || /[\\\x00-\x1f\x7f]/.test(name) || path.isAbsolute(name)
    || name.split("/").some((part) => part === "" || part === "." || part === "..")) {
    throw new Error(`Invalid store name: ${name}`);
  }
  const target = path.join(store, name);
  const real = fs.existsSync(target) ? fs.realpathSync(target) : undefined;
  const ancestor = existingAncestor(path.dirname(target));
  const parent = fs.realpathSync(ancestor);
  if ((real !== undefined && (real !== path.resolve(target) || !inside(store, real)))
    || parent !== path.resolve(ancestor) || !inside(store, parent)) {
    throw new Error(`Store path ${name} escapes the rooted progress directory through a symlink`);
  }
  return { name, target };
}

function existingAncestor(directory: string): string {
  return fs.existsSync(directory) ? directory : existingAncestor(path.dirname(directory));
}

function storeRead(request: NativeRequest, store: string): LocalResponse {
  const limit = boundedInteger(request.limit, "limit", MAX_TEXT);
  const key = text(request.slice, "slice");
  const { target } = rooted(store, request.name);
  if (!fs.existsSync(target) || !fs.statSync(target).isFile()) return { result: { found: false }, slices: [] };
  return { result: { found: true }, slices: [{ key, text: boundedText(target, limit) }] };
}

function storeWrite(request: NativeRequest, store: string, held: ReadonlyMap<string, string>): LocalResponse {
  const key = text(request.slice, "slice");
  const content = held.get(key);
  if (content === undefined) throw new Error(`Store write requested the absent host slice ${key}`);
  if (Buffer.byteLength(content, "utf8") > MAX_TEXT) throw new Error(`Host slice ${key} exceeds its ${MAX_TEXT}-byte bound`);
  const { name, target } = rooted(store, request.name);
  persist(target, content);
  return { result: { path: name }, slices: [] };
}


function canonicalDirectory(directory: string, role: string): string {
  const real = fs.realpathSync(directory);
  if (!fs.statSync(real).isDirectory()) throw new Error(`The ${role} root must be a directory: ${directory}`);
  return real;
}

/** Execute one bounded local capability: approved asset reads and rooted progress persistence. */
export function createLocalExecutor(options: LocalExecutorOptions): (request: NativeRequest, slices: ReadonlyMap<string, string>) => Promise<unknown> {
  const assets = canonicalDirectory(options.assetRoot, "asset");
  fs.mkdirSync(options.storeRoot, { recursive: true, mode: PRIVATE_DIRECTORY_MODE });
  const store = canonicalDirectory(options.storeRoot, "store");
  return async (request, slices) => {
    if (request.kind === "read-texts") return readTexts(request, assets);
    if (request.kind === "store-read") return storeRead(request, store);
    if (request.kind === "store-write") return storeWrite(request, store, slices);
    if (options.dialogs && (request.kind === "choose-file" || request.kind === "save-file")) {
      return documentRequest(request, slices, options.dialogs, MAX_TEXT);
    }
    throw new Error(`Unsupported local capability: ${request.kind}`);
  };
}

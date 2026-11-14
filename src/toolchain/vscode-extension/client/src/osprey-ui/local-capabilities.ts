// Implements [LESSONS-FILES] and [LESSONS-PROGRESS] host glue: bounded reads of approved bundled
// application assets and atomic progress persistence inside one rooted store directory. The
// executor grants no arbitrary filesystem, network or process access; unsupported requests fail
// explicitly. Content the application writes travels as host-held slices, which its own ask rows
// named before the request that reads them.

import * as fs from "node:fs";
import * as path from "node:path";
import { document, type Ask, type Completion } from "./asks";
import { approvedFile, inside, boundedText, persist, PRIVATE_DIRECTORY_MODE } from "./local-files";
import { documentRequest, type DocumentDialogs } from "./document-capabilities";

/** The host-held slice bound ([NATIVE-SHM-ASK]); no read or write exceeds it. */
const MAX_TEXT = 8 * 1024 * 1024;

export interface LocalExecutorOptions {
  readonly assetRoot: string;
  readonly storeRoot: string;
  readonly dialogs?: DocumentDialogs;
}

/** Preserve every failure as readable text at a native host boundary. */
function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function required(ask: Ask, field: number, name: string): string {
  const value = ask.text(field);
  if (value === "") throw new Error(`${ask.kind} has no valid ${name}`);
  return value;
}

function bounded(ask: Ask, field: number, name: string, most: number): number {
  const value = ask.number(field);
  if (!ask.fields.some((item) => item.field === field) || value <= 0 || value > most) {
    throw new Error(`${ask.kind} has no valid ${name} between 1 and ${most}`);
  }
  return value;
}

function fileText(assets: string, source: string, limit: number): { path: string; text: string; error: string } {
  try {
    return { path: source, text: boundedText(approvedFile(assets, source), limit), error: "" };
  } catch (error) {
    return { path: source, text: "", error: messageOf(error) };
  }
}

function readTexts(ask: Ask, assets: string): Completion {
  const limit = bounded(ask, 2, "limit", MAX_TEXT);
  const sources = ask.texts(1);
  if (sources.length === 0 || sources.length > 64) throw new Error("read-texts has no valid paths");
  const key = required(ask, 3, "slice");
  const files = sources.map((source) => fileText(assets, source, limit));
  const contents = JSON.stringify({ files });
  if (Buffer.byteLength(contents, "utf8") > MAX_TEXT) throw new Error(`Read slice ${key} exceeds its byte bound`);
  return { ...document({ files: files.map(({ path: source, error }) => ({ path: source, error })) }), slices: [{ key, text: contents }] };
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

function storeRead(ask: Ask, store: string): Completion {
  const limit = bounded(ask, 2, "limit", MAX_TEXT);
  const key = required(ask, 3, "slice");
  const { target } = rooted(store, required(ask, 1, "name"));
  if (!fs.existsSync(target) || !fs.statSync(target).isFile()) return document({ found: false });
  return { ...document({ found: true }), slices: [{ key, text: boundedText(target, limit) }] };
}

function storeWrite(ask: Ask, store: string, held: ReadonlyMap<string, string>): Completion {
  const key = required(ask, 2, "slice");
  const content = held.get(key);
  if (content === undefined) throw new Error(`Store write requested the absent host slice ${key}`);
  if (Buffer.byteLength(content, "utf8") > MAX_TEXT) throw new Error(`Host slice ${key} exceeds its ${MAX_TEXT}-byte bound`);
  const { name, target } = rooted(store, required(ask, 1, "name"));
  persist(target, content);
  return document({ path: name });
}

function canonicalDirectory(directory: string, role: string): string {
  const real = fs.realpathSync(directory);
  if (!fs.statSync(real).isDirectory()) throw new Error(`The ${role} root must be a directory: ${directory}`);
  return real;
}

/** Execute one bounded local capability: approved asset reads and rooted progress persistence. */
export function createLocalExecutor(options: LocalExecutorOptions): (ask: Ask, slices: ReadonlyMap<string, string>) => Promise<Completion> {
  const assets = canonicalDirectory(options.assetRoot, "asset");
  fs.mkdirSync(options.storeRoot, { recursive: true, mode: PRIVATE_DIRECTORY_MODE });
  const store = canonicalDirectory(options.storeRoot, "store");
  return async (ask, slices) => {
    if (ask.kind === "read-texts") return readTexts(ask, assets);
    if (ask.kind === "store-read") return storeRead(ask, store);
    if (ask.kind === "store-write") return storeWrite(ask, store, slices);
    if (options.dialogs && (ask.kind === "choose-file" || ask.kind === "save-file")) {
      return documentRequest(ask, slices, options.dialogs, MAX_TEXT);
    }
    throw new Error(`Unsupported local capability: ${ask.kind}`);
  };
}

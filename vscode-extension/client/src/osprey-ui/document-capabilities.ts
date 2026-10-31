// Implements [UI-EFFECTS]: explicit document selection and export of Osprey-owned slices.
import * as path from "node:path";
import { boundedText, persist } from "./local-files";
import { boundedInteger, requiredString, stringList } from "./wire";
import type { NativeRequest } from "./protocol";
import type { LocalResponse } from "./local-capabilities";

export interface DocumentDialogs {
  open(types: readonly string[]): Promise<string | undefined>;
  save(name: string): Promise<string | undefined>;
}

const cancelled: LocalResponse = { result: { path: "", cancelled: true }, slices: [] };

async function choose(request: NativeRequest, dialogs: DocumentDialogs, maximum: number): Promise<LocalResponse> {
  const key = requiredString(request.slice, "slice");
  const limit = boundedInteger(request.limit, "limit", maximum);
  const types = stringList(request.types, "types", true);
  if (!types.every(type => /^[a-z0-9][a-z0-9_-]*$/i.test(type))) throw new Error("Invalid file extension filter");
  const selected = await dialogs.open(types);
  if (selected === undefined) return cancelled;
  if (!path.isAbsolute(selected)) throw new Error("File dialog returned a non-absolute path");
  return { result: { path: selected, cancelled: false }, slices: [{ key, text: boundedText(selected, limit) }] };
}

async function save(request: NativeRequest, held: ReadonlyMap<string, string>, dialogs: DocumentDialogs, maximum: number): Promise<LocalResponse> {
  const name = requiredString(request.name, "name");
  const key = requiredString(request.slice, "slice");
  const content = held.get(key);
  if (name === "." || name === ".." || name.length > 255 || /[\\/\x00-\x1f\x7f]/.test(name)) throw new Error("Invalid suggested file name");
  if (content === undefined) throw new Error(`Save requested the absent host slice ${key}`);
  if (Buffer.byteLength(content, "utf8") > maximum) throw new Error(`Export slice ${key} exceeds its byte bound`);
  const selected = await dialogs.save(name);
  if (selected === undefined) return cancelled;
  if (!path.isAbsolute(selected)) throw new Error("File dialog returned a non-absolute path");
  persist(selected, content);
  return { result: { path: selected, cancelled: false }, slices: [] };
}

/** Adapt only document capabilities; all content and interpretation remain with Osprey. */
export function documentRequest(request: NativeRequest, held: ReadonlyMap<string, string>, dialogs: DocumentDialogs, maximum: number): Promise<LocalResponse> {
  return request.kind === "choose-file" ? choose(request, dialogs, maximum) : save(request, held, dialogs, maximum);
}

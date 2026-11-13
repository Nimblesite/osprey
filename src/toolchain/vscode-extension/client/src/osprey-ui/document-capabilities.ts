// Implements [UI-EFFECTS]: explicit document selection and export of Osprey-owned slices. A
// chooser completes with the bare path it chose, or `""` when it was cancelled ([NATIVE-SHM-ASK]).

import * as path from "node:path";
import { bare, type Ask, type Completion } from "./asks";
import { persist } from "./local-files";

export interface DocumentDialogs {
  open(types: readonly string[]): Promise<string | undefined>;
  save(name: string): Promise<string | undefined>;
}

function maximum(ask: Ask, field: number, bound: number): number {
  const limit = ask.number(field);
  if (!ask.fields.some((item) => item.field === field) || limit <= 0 || limit > bound) {
    throw new Error(`${ask.kind} has no valid limit between 1 and ${bound}`);
  }
  return limit;
}

function required(ask: Ask, field: number, name: string): string {
  const value = ask.text(field);
  if (value === "") throw new Error(`${ask.kind} has no valid ${name}`);
  return value;
}

async function choose(ask: Ask, dialogs: DocumentDialogs, bound: number): Promise<Completion> {
  maximum(ask, 2, bound);
  const types = ask.texts(1);
  if (types.length === 0 || types.length > 64) throw new Error("choose-file has no valid file extension filter");
  if (!types.every((type) => /^[a-z0-9][a-z0-9_-]*$/i.test(type))) throw new Error("Invalid file extension filter");
  const selected = await dialogs.open(types);
  if (selected === undefined) return bare("");
  if (!path.isAbsolute(selected)) throw new Error("File dialog returned a non-absolute path");
  return bare(selected);
}

async function save(ask: Ask, held: ReadonlyMap<string, string>, dialogs: DocumentDialogs, bound: number): Promise<Completion> {
  const name = required(ask, 1, "name");
  const key = required(ask, 2, "slice");
  const content = held.get(key);
  if (name === "." || name === ".." || name.length > 255 || /[\\/\x00-\x1f\x7f]/.test(name)) throw new Error("Invalid suggested file name");
  if (content === undefined) throw new Error(`Save requested the absent host slice ${key}`);
  if (Buffer.byteLength(content, "utf8") > bound) throw new Error(`Export slice ${key} exceeds its byte bound`);
  const selected = await dialogs.save(name);
  if (selected === undefined) return bare("");
  if (!path.isAbsolute(selected)) throw new Error("File dialog returned a non-absolute path");
  persist(selected, content);
  return bare(selected);
}

/** Adapt only document capabilities; all content and interpretation remain with Osprey. */
export function documentRequest(ask: Ask, held: ReadonlyMap<string, string>, dialogs: DocumentDialogs, bound: number): Promise<Completion> {
  return ask.kind === "choose-file" ? choose(ask, dialogs, bound) : save(ask, held, dialogs, bound);
}

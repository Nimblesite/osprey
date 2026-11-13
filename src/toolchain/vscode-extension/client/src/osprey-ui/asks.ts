// [NATIVE-SHM-ASK]: one frame's asks checked and read as typed values. A row whose `field` is 0
// starts an ask: its `number` is the capability's code and its text the ticket. The rows after it
// are that ask's arguments, numbered from 1 by their position in the capability's declaration
// (mirrors hosts/gpui/src/rows.rs and capability.ospml).

import { ASK_ROW, ASK_ROW_BYTES } from "./generated/native-abi.js";
import { textAt, whole } from "./webview/rows.js";

export { ASK_ROW_BYTES };
export const SLICE_WRITE = 100;
export const DIAGNOSTIC = 101;

/** The capability names, by code from 1, as `Requests::kind` spells them. */
export const CAPABILITIES: readonly string[] = [
  "run", "list-directories", "read-plists", "read-texts", "fingerprint", "store-read",
  "store-write", "store-list", "store-delete", "http", "host-call", "running-applications",
  "choose-file", "choose-folder", "save-file", "measure-folders", "reveal", "open-settings",
  "cancel", "recognise", "animate", "process-start", "process-write", "process-read", "process-stop",
];

/** One field of an ask: its argument position, number and text. */
export interface Field {
  readonly field: number;
  readonly number: number;
  readonly text: string;
}

/** A named pair a request's list carries: an HTTP header or an environment variable. */
export interface Pair {
  readonly name: string;
  readonly value: string;
}

const KNOWN = new Set([...CAPABILITIES.map((_, index) => index + 1), SLICE_WRITE, DIAGNOSTIC]);

/** A capability request or a slice write: its code, its ticket (a slice write's key) and its fields. */
export class Ask {
  constructor(
    readonly code: number,
    readonly ticket: string,
    readonly fields: readonly Field[],
  ) {}

  /** The capability's name, or a refusal for a code this host does not know. */
  get kind(): string {
    if (this.code === SLICE_WRITE) return "slice-write";
    if (this.code === DIAGNOSTIC) return "diagnostic";
    const name = CAPABILITIES[this.code - 1];
    if (name === undefined) throw new Error(`Unknown capability code ${this.code}`);
    return name;
  }

  /** Every text at argument `field`, in order: a list's items or a pair's halves. */
  texts(field: number): string[] {
    return this.fields.filter((item) => item.field === field).map((item) => item.text);
  }

  /** The whole number at argument `field`, or 0. */
  number(field: number): number {
    return this.fields.find((item) => item.field === field)?.number ?? 0;
  }

  /** The flag at argument `field`. */
  flag(field: number): boolean {
    return this.number(field) !== 0;
  }

  /** The texts at `field` taken two at a time, in order. */
  pairs(field: number): Pair[] {
    const texts = this.texts(field);
    if (texts.length % 2 !== 0) throw new Error(`${this.kind} argument ${field} must be name and value pairs`);
    const pairs: Pair[] = [];
    for (let at = 0; at < texts.length; at += 2) pairs.push({ name: texts[at]!, value: texts[at + 1]! });
    return pairs;
  }

  /** The first text at `field`, which a required argument must carry. */
  text(field: number): string {
    return this.texts(field)[0] ?? "";
  }
}

/** Checks and reads a frame's ask run; a bad row refuses the whole run. */
export function readAsks(run: ArrayBuffer, textRun: ArrayBuffer): Ask[] {
  if (run.byteLength % ASK_ROW_BYTES !== 0) throw new Error("the ask run is not a whole number of rows");
  const rows = new DataView(run);
  const text = new Uint8Array(textRun);
  const asks: Ask[] = [];
  const fields: Field[][] = [];
  for (let at = 0; at < run.byteLength; at += ASK_ROW_BYTES) {
    const field = whole(rows, at + ASK_ROW.field);
    const number = whole(rows, at + ASK_ROW.number);
    const value: Field = { field, number, text: textAt(text, whole(rows, at + ASK_ROW.textOffset), whole(rows, at + ASK_ROW.textLength)) };
    if (field === 0) {
      if (!KNOWN.has(number)) throw new Error(`an ask with an unknown capability code ${number}`);
      asks.push(new Ask(number, value.text, []));
      fields.push([]);
    } else if (field > 0 && fields.length > 0) {
      fields[fields.length - 1]!.push(value);
    } else {
      throw new Error("an ask field before any ask, or below zero");
    }
  }
  return asks.map((ask, index) => new Ask(ask.code, ask.ticket, fields[index] ?? []));
}

/** The capability requests to answer, in order. */
export function requests(asks: readonly Ask[]): Ask[] {
  return asks.filter((ask) => ask.code >= 1 && ask.code <= CAPABILITIES.length);
}

/** The slice writes, as key and text. */
/** One host-held slice: its key and its text. */
export interface SliceWrite {
  readonly key: string;
  readonly text: string;
}

export function sliceWrites(asks: readonly Ask[]): SliceWrite[] {
  return asks.filter((ask) => ask.code === SLICE_WRITE).map((ask) => ({ key: ask.ticket, text: ask.text(1) }));
}

/** Why the application refused the event this frame answers, if it did. */
export function diagnostics(asks: readonly Ask[]): string[] {
  return asks.filter((ask) => ask.code === DIAGNOSTIC).map((ask) => ask.ticket);
}

/** A capability's answer, as its completion's `detail` text. */
export interface Completion {
  readonly detail: string;
  /** Host-held slices the adapter filled, which the application fetches by key ([UI-EFFECTS]). */
  readonly slices?: readonly SliceWrite[];
}

/** A result document as compact JSON text. */
export function document(value: unknown): Completion {
  return { detail: JSON.stringify(value) };
}

/** A chooser's bare path, `""` when it was cancelled. */
export function bare(path: string): Completion {
  return { detail: path };
}

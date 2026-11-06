// Implements [UI-PROTOCOL] and [UI-EVENTS]: the host keeps only the opaque
// session string, keyed regions and bounded requests printed by the Osprey
// runtime. Everything here validates the wire shape; it never interprets a
// package model or an application action.
import { record, naturalInteger as integer } from "./wire";
import { boardStroke, boardSelection } from "./board-value";
export { record } from "./wire";

export interface NativeRequest {
  readonly ticket: string;
  readonly kind: string;
  readonly [key: string]: unknown;
}

export interface NativeSlice {
  readonly key: string;
  readonly text: string;
}

export interface NativePatch {
  readonly key: string;
  readonly revision: number;
  readonly node: unknown;
}

export interface NativeEnvelope {
  readonly version: number;
  readonly state: string;
  readonly slices: readonly NativeSlice[];
  readonly patches: readonly NativePatch[];
  readonly removed: readonly string[];
  readonly requests: readonly NativeRequest[];
  readonly diagnostics: readonly string[];
  readonly theme: unknown;
}

export interface RegionSnapshot {
  readonly revision: number;
  readonly node: unknown;
}

export type Regions = ReadonlyMap<string, RegionSnapshot>;

export interface UiEvent {
  readonly region: string;
  readonly revision: number;
  readonly ordinal: number;
  readonly value: unknown;
  readonly now: number;
}

export type CompletionOutcome =
  | { readonly ok: true; readonly result: unknown }
  | { readonly ok: false; readonly error: string };

export const PROTOCOL_VERSION = 1;
const MAX_SLICES = 256;
const MAX_SLICE_KEY = 256;
const MAX_SLICE_TEXT = 8 * 1024 * 1024;

function strings(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function slices(value: unknown): NativeSlice[] {
  if (value === undefined) return [];
  if (!Array.isArray(value)) throw new Error("Native envelope slices must be an array");
  if (value.length > MAX_SLICES) throw new Error("Native envelope contains too many slices");
  const seen = new Set<string>();
  return value.map((item) => {
    if (!record(item) || typeof item.key !== "string" || typeof item.text !== "string") {
      throw new Error("Native envelope contains a malformed slice");
    }
    if (item.key === "" || item.key.length > MAX_SLICE_KEY) throw new Error("Native envelope slice keys must be short and non-empty");
    if (item.text.length > MAX_SLICE_TEXT) throw new Error(`Native envelope slice ${item.key} exceeds its bound`);
    if (seen.has(item.key)) throw new Error(`Native envelope repeats slice ${item.key}`);
    seen.add(item.key);
    return { key: item.key, text: item.text };
  });
}

function patches(value: unknown): NativePatch[] {
  if (!Array.isArray(value)) throw new Error("Native envelope is missing its patches");
  return value.map((item) => {
    if (!record(item) || typeof item.key !== "string" || !integer(item.revision) || !("node" in item)) {
      throw new Error("Native envelope contains a malformed region patch");
    }
    return { key: item.key, revision: item.revision, node: item.node };
  });
}

function requests(value: unknown): NativeRequest[] {
  if (!Array.isArray(value)) throw new Error("Native envelope is missing its requests");
  return value.map((item) => {
    if (!record(item) || typeof item.ticket !== "string" || item.ticket === "" || typeof item.kind !== "string") {
      throw new Error("Native envelope contains a malformed request");
    }
    return item as NativeRequest;
  });
}

function lastJsonLine(source: string): unknown {
  const lines = source.split(/\r?\n/).filter((line) => line.trim() !== "");
  for (let index = lines.length - 1; index >= 0; index -= 1) {
    try {
      return JSON.parse(lines[index]);
    } catch {
      // Scan upward: the runtime prints exactly one envelope, but a host may
      // capture a trailing non-JSON diagnostic line from libc.
    }
  }
  throw new Error("The Osprey workspace printed no JSON envelope");
}

export function parseEnvelope(source: string): NativeEnvelope {
  const parsed = lastJsonLine(source);
  if (!record(parsed)) throw new Error("The Osprey workspace envelope is not a JSON object");
  if (parsed.version !== PROTOCOL_VERSION) throw new Error(`Unsupported Osprey workspace protocol version: ${String(parsed.version)}`);
  if (typeof parsed.state !== "string") throw new Error("The Osprey workspace envelope is missing its opaque state");
  if (parsed.diagnostics !== undefined && !strings(parsed.diagnostics)) throw new Error("Native envelope diagnostics must be strings");
  if (parsed.removed !== undefined && !strings(parsed.removed)) throw new Error("Native envelope removals must be strings");
  return {
    version: PROTOCOL_VERSION,
    state: parsed.state,
    slices: slices(parsed.slices),
    patches: patches(parsed.patches),
    removed: strings(parsed.removed) ? parsed.removed : [],
    requests: requests(parsed.requests),
    diagnostics: strings(parsed.diagnostics) ? parsed.diagnostics : [],
    theme: parsed.theme ?? null,
  };
}

/** Apply keyed patches and removals; retained regions keep the host's copy. */
export function applyEnvelope(regions: Regions, envelope: NativeEnvelope): Map<string, RegionSnapshot> {
  const next = new Map(regions);
  for (const key of envelope.removed) next.delete(key);
  for (const patch of envelope.patches) next.set(patch.key, { revision: patch.revision, node: patch.node });
  return next;
}

export function startFrame(context: Record<string, unknown>): string {
  return JSON.stringify({ operation: "start", context });
}

function eventBody(event: UiEvent): Record<string, unknown> {
  return {
    kind: "ui",
    now: event.now,
    region: event.region,
    revision: event.revision,
    ordinal: event.ordinal,
    value: event.value,
  };
}

export function dispatchFrame(state: string, event: UiEvent): string {
  return JSON.stringify({ operation: "dispatch", state, event: eventBody(event) });
}

/** Preserve the issued control while Osprey resolves its binding against current state. */
export function editFrame(state: string, prior: string, event: UiEvent): string {
  return JSON.stringify({ operation: "dispatch", state, event: { ...eventBody(event), kind: "ui-edit", prior } });
}

export function completionFrame(state: string, now: number, ticket: string, outcome: CompletionOutcome): string {
  return JSON.stringify({
    operation: "dispatch",
    state,
    event: outcome.ok
      ? { kind: "completed", now, ticket, ok: true, result: outcome.result }
      : { kind: "completed", now, ticket, ok: false, error: outcome.error },
  });
}

function simpleValue(value: unknown): value is null | boolean | number | string {
  return value === null || typeof value === "boolean" || (typeof value === "number" && Number.isFinite(value)) || typeof value === "string";
}

/** Accept only a control event for a region revision the host currently shows. */
export function parseUiEvent(message: unknown, regions: Regions, now: number): UiEvent | undefined {
  if (!record(message) || message.type !== "event") return undefined;
  if (typeof message.region !== "string" || !integer(message.revision) || !integer(message.ordinal)) return undefined;
  if (!simpleValue(message.value) && !boardStroke(message.value) && !boardSelection(message.value)) return undefined;
  const region = regions.get(message.region);
  if (!region || region.revision !== message.revision) return undefined;
  return { region: message.region, revision: message.revision, ordinal: message.ordinal, value: message.value, now };
}

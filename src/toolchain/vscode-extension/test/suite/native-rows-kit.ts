// Shared fixture builders for the shared-memory frame tests: they write the generated wasm32
// layouts the same way the application runtime does, so a test's bytes are the host's bytes.

import { ASK_ROW, ASK_ROW_BYTES, SCENE_SHAPE, SCENE_SHAPE_BYTES, VIEW_ROW, VIEW_ROW_BYTES } from "../../client/src/osprey-ui/generated/native-abi";
import type { FrameRuns } from "../../client/src/osprey-ui/webview/rows";

/** Accumulates a frame's UTF-8 text run and names each appended range. */
export class TextRun {
  private readonly bytes: number[] = [];

  add(text: string): { offset: number; length: number } {
    const encoded = [...Buffer.from(text, "utf8")];
    const offset = this.bytes.length;
    this.bytes.push(...encoded);
    return { offset, length: encoded.length };
  }

  buffer(): ArrayBuffer {
    return Uint8Array.from(this.bytes).buffer;
  }
}

export interface ControlSpec {
  readonly kind: number;
  readonly parent?: number;
  readonly action?: number;
  readonly words?: string;
  readonly hint?: string;
  readonly ink?: number;
  readonly fill?: number;
  readonly size?: number;
  readonly padding?: number;
  readonly spacing?: number;
  readonly radius?: number;
  /** A view row's font weight; a told row's value type, 0 to 4 ([NATIVE-SHM-NODES]). */
  readonly weight?: number;
  readonly width?: number;
  readonly height?: number;
  readonly grow?: number;
  readonly flags?: number;
  readonly border?: number;
}

/** One or more view rows in drawing order, with their text written into `run`. */
export function viewRows(specs: readonly ControlSpec[], run: TextRun): ArrayBuffer {
  const buffer = new ArrayBuffer(specs.length * VIEW_ROW_BYTES);
  const view = new DataView(buffer);
  specs.forEach((spec, index) => {
    const at = index * VIEW_ROW_BYTES;
    const words = run.add(spec.words ?? "");
    const hint = run.add(spec.hint ?? "");
    view.setBigInt64(at + VIEW_ROW.kind, BigInt(spec.kind), true);
    view.setBigInt64(at + VIEW_ROW.parent, BigInt(spec.parent ?? -1), true);
    view.setBigInt64(at + VIEW_ROW.action, BigInt(spec.action ?? -1), true);
    view.setBigInt64(at + VIEW_ROW.textOffset, BigInt(words.offset), true);
    view.setBigInt64(at + VIEW_ROW.textLength, BigInt(words.length), true);
    view.setBigInt64(at + VIEW_ROW.colour, BigInt(spec.ink ?? 0), true);
    view.setBigInt64(at + VIEW_ROW.background, BigInt(spec.fill ?? 0), true);
    view.setFloat64(at + VIEW_ROW.size, spec.size ?? 0, true);
    view.setFloat64(at + VIEW_ROW.padding, spec.padding ?? 0, true);
    view.setFloat64(at + VIEW_ROW.spacing, spec.spacing ?? 0, true);
    view.setFloat64(at + VIEW_ROW.radius, spec.radius ?? 0, true);
    view.setBigInt64(at + VIEW_ROW.weight, BigInt(spec.weight ?? (spec.kind >= 33 ? 0 : 400)), true);
    view.setFloat64(at + VIEW_ROW.width, spec.width ?? 0, true);
    view.setFloat64(at + VIEW_ROW.height, spec.height ?? 0, true);
    view.setFloat64(at + VIEW_ROW.grow, spec.grow ?? 0, true);
    view.setBigInt64(at + VIEW_ROW.flags, BigInt(spec.flags ?? 0), true);
    view.setBigInt64(at + VIEW_ROW.border, BigInt(spec.border ?? 0), true);
    view.setBigInt64(at + VIEW_ROW.hintOffset, BigInt(hint.offset), true);
    view.setBigInt64(at + VIEW_ROW.hintLength, BigInt(hint.length), true);
  });
  return buffer;
}

export interface SceneSpec {
  readonly code: number;
  readonly index?: number;
  readonly x?: number;
  readonly y?: number;
  readonly width?: number;
  readonly height?: number;
  readonly angle?: number;
  readonly colour?: number;
  readonly depth?: number;
  readonly text?: string;
}

/** One or more scene rows in drawing order, with their text written into `run`. */
export function sceneRows(specs: readonly SceneSpec[], run: TextRun): ArrayBuffer {
  const buffer = new ArrayBuffer(specs.length * SCENE_SHAPE_BYTES);
  const scene = new DataView(buffer);
  specs.forEach((spec, index) => {
    const at = index * SCENE_SHAPE_BYTES;
    const text = run.add(spec.text ?? "");
    scene.setBigInt64(at + SCENE_SHAPE.code, BigInt(spec.code), true);
    scene.setBigInt64(at + SCENE_SHAPE.index, BigInt(spec.index ?? 0), true);
    scene.setFloat64(at + SCENE_SHAPE.x, spec.x ?? 0, true);
    scene.setFloat64(at + SCENE_SHAPE.y, spec.y ?? 0, true);
    scene.setFloat64(at + SCENE_SHAPE.width, spec.width ?? 0, true);
    scene.setFloat64(at + SCENE_SHAPE.height, spec.height ?? 0, true);
    scene.setFloat64(at + SCENE_SHAPE.angle, spec.angle ?? 0, true);
    scene.setBigInt64(at + SCENE_SHAPE.colour, BigInt(spec.colour ?? 0), true);
    scene.setBigInt64(at + SCENE_SHAPE.depth, BigInt(spec.depth ?? 0), true);
    scene.setBigInt64(at + SCENE_SHAPE.textOffset, BigInt(text.offset), true);
    scene.setBigInt64(at + SCENE_SHAPE.textLength, BigInt(text.length), true);
  });
  return buffer;
}

export interface AskSpec {
  readonly code: number;
  readonly ticket: string;
  readonly fields?: readonly (readonly [number, number, string])[];
}

/** One or more ask rows, with their text written into `run`. */
export function askRows(specs: readonly AskSpec[], run: TextRun): ArrayBuffer {
  const rows: { field: number; number: number; text: string }[] = [];
  for (const spec of specs) {
    rows.push({ field: 0, number: spec.code, text: spec.ticket });
    for (const [field, number, text] of spec.fields ?? []) rows.push({ field, number, text });
  }
  const buffer = new ArrayBuffer(rows.length * ASK_ROW_BYTES);
  const asks = new DataView(buffer);
  rows.forEach((row, index) => {
    const at = index * ASK_ROW_BYTES;
    const text = run.add(row.text);
    asks.setBigInt64(at + ASK_ROW.field, BigInt(row.field), true);
    asks.setBigInt64(at + ASK_ROW.number, BigInt(row.number), true);
    asks.setBigInt64(at + ASK_ROW.textOffset, BigInt(text.offset), true);
    asks.setBigInt64(at + ASK_ROW.textLength, BigInt(text.length), true);
  });
  return buffer;
}

/** A complete frame's four runs, sharing one text run. */
export function frame(options: {
  readonly views?: readonly ControlSpec[];
  readonly scenes?: readonly SceneSpec[];
  readonly asks?: readonly AskSpec[];
}): FrameRuns & { readonly asks: ArrayBuffer } {
  const run = new TextRun();
  const views = viewRows(options.views ?? [], run);
  const shapes = sceneRows(options.scenes ?? [], run);
  const asks = askRows(options.asks ?? [], run);
  return { shapes, views, text: run.buffer(), asks };
}

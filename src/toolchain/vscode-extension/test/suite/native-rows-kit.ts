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

/** Sets little-endian fields and shared text ranges at one row's base offset. */
function rowWriter(view: DataView, at: number, run: TextRun) {
  const i64 = (field: number, value: number) => view.setBigInt64(at + field, BigInt(value), true);
  return {
    i64,
    f64: (field: number, value: number) => view.setFloat64(at + field, value, true),
    text(value: string, offsetField: number, lengthField: number) {
      const { offset, length } = run.add(value);
      i64(offsetField, offset);
      i64(lengthField, length);
    },
  };
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
    const row = rowWriter(view, at, run);
    row.text(spec.words ?? "", VIEW_ROW.textOffset, VIEW_ROW.textLength);
    row.text(spec.hint ?? "", VIEW_ROW.hintOffset, VIEW_ROW.hintLength);
    row.i64(VIEW_ROW.kind, spec.kind);
    row.i64(VIEW_ROW.parent, spec.parent ?? -1);
    row.i64(VIEW_ROW.action, spec.action ?? -1);
    row.i64(VIEW_ROW.colour, spec.ink ?? 0);
    row.i64(VIEW_ROW.background, spec.fill ?? 0);
    row.f64(VIEW_ROW.size, spec.size ?? 0);
    row.f64(VIEW_ROW.padding, spec.padding ?? 0);
    row.f64(VIEW_ROW.spacing, spec.spacing ?? 0);
    row.f64(VIEW_ROW.radius, spec.radius ?? 0);
    row.i64(VIEW_ROW.weight, spec.weight ?? (spec.kind >= 33 ? 0 : 400));
    row.f64(VIEW_ROW.width, spec.width ?? 0);
    row.f64(VIEW_ROW.height, spec.height ?? 0);
    row.f64(VIEW_ROW.grow, spec.grow ?? 0);
    row.i64(VIEW_ROW.flags, spec.flags ?? 0);
    row.i64(VIEW_ROW.border, spec.border ?? 0);
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
    const row = rowWriter(scene, at, run);
    row.text(spec.text ?? "", SCENE_SHAPE.textOffset, SCENE_SHAPE.textLength);
    row.i64(SCENE_SHAPE.code, spec.code);
    row.i64(SCENE_SHAPE.index, spec.index ?? 0);
    row.f64(SCENE_SHAPE.x, spec.x ?? 0);
    row.f64(SCENE_SHAPE.y, spec.y ?? 0);
    row.f64(SCENE_SHAPE.width, spec.width ?? 0);
    row.f64(SCENE_SHAPE.height, spec.height ?? 0);
    row.f64(SCENE_SHAPE.angle, spec.angle ?? 0);
    row.i64(SCENE_SHAPE.colour, spec.colour ?? 0);
    row.i64(SCENE_SHAPE.depth, spec.depth ?? 0);
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
  rows.forEach((rowSpec, index) => {
    const row = rowWriter(asks, index * ASK_ROW_BYTES, run);
    row.text(rowSpec.text, ASK_ROW.textOffset, ASK_ROW.textLength);
    row.i64(ASK_ROW.field, rowSpec.field);
    row.i64(ASK_ROW.number, rowSpec.number);
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

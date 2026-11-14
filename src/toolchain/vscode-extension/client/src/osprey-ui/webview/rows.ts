// [NATIVE-SHM-VIEW] [NATIVE-SHM-SCENE]: one sealed frame checked and read as typed values.
// Nothing the application wrote is trusted: every kind and code must be known, every parent must
// be an earlier row, every text range must lie inside the text run on UTF-8 boundaries, every
// float must be finite and every count must stay inside the rows that follow it. A frame that
// fails any check is refused whole, and the last good view stays (mirrors hosts/gpui/src/rows.rs).

import { SCENE_SHAPE, SCENE_SHAPE_BYTES, VIEW_ROW, VIEW_ROW_BYTES } from "../generated/native-abi.js";

/** Each `kind` number the frame may carry, by the number `native.td` and `view.ospml` give it. */
const KIND_ROWS = [
  [0, "column"], [1, "line"], [2, "label"], [3, "press"], [4, "entry"], [5, "gap"], [6, "scroll"],
  [7, "layer"], [8, "toggle"], [9, "picker"], [10, "choice"], [11, "rule"], [12, "image"],
  [13, "canvas"], [14, "editor"], [15, "menu"], [16, "sheet"],
  /** [NATIVE-SHM-NODES] One told value, a record of members and a list of items. */
  [33, "field"], [34, "record"], [35, "list"],
] as const;

/** One control's `kind`, in the order `native.td` and `view.ospml` number them. */
export type Kind = (typeof KIND_ROWS)[number][1];

const KINDS: ReadonlyMap<number, Kind> = new Map(KIND_ROWS);

/** Whether a row tells a component rather than draws a control ([NATIVE-SHM-NODES]). */
export function told(kind: Kind): boolean {
  return kind === "field" || kind === "record" || kind === "list";
}

/** The `traits` flags a row's fit adds up ([NATIVE-SHM-VIEW]). */
export const FLAGS = {
  disabled: 1,
  chosen: 2,
  secret: 4,
  fixedWidth: 8,
  slanted: 16,
  centred: 32,
  onCommit: 64,
  all: 127,
} as const;

/** How one control looks; every value is decided in Osprey. */
export interface Look {
  readonly ink: number;
  readonly fill: number;
  readonly size: number;
  readonly padding: number;
  readonly spacing: number;
  readonly radius: number;
  readonly weight: number;
}

/** How one control sits: fixed extents (0 = natural), its share of free space, flags and border. */
export interface Fit {
  readonly width: number;
  readonly height: number;
  readonly grow: number;
  readonly flags: number;
  readonly border: number;
}

/** One checked control and the rows drawn inside it. */
export interface Control {
  readonly kind: Kind;
  /** The control number a host event names, when the control delivers one. */
  readonly action: number | null;
  readonly words: string;
  /** A field's prompt, an option's or tab's key, or the icon a label or press is led by. */
  readonly hint: string;
  readonly look: Look;
  readonly fit: Fit;
  /** A told field's whole number or flag; 0 for every other row ([NATIVE-SHM-NODES]). */
  readonly value: number;
  children: number[];
}

export interface Point {
  readonly x: number;
  readonly y: number;
}

/** One path-builder command in scene units; curves name their end point first. */
export type Step =
  | { readonly step: "move"; readonly to: Point }
  | { readonly step: "line"; readonly to: Point }
  | { readonly step: "curve"; readonly to: Point; readonly control: Point }
  | { readonly step: "cubic"; readonly to: Point; readonly first: Point; readonly second: Point }
  | { readonly step: "arc"; readonly radii: Point; readonly rotation: number; readonly large: boolean; readonly sweep: boolean; readonly to: Point }
  | { readonly step: "polygon"; readonly corners: readonly Point[]; readonly closed: boolean }
  | { readonly step: "close" };

/** Fill, or stroke with a width, bevel joins and an optional dash pattern. */
export type Outline =
  | { readonly outline: "fill" }
  | { readonly outline: "stroke"; readonly width: number; readonly bevel: boolean; readonly dashes: readonly number[] };

/** A float colour in the space the application gave it. */
export type Tint =
  | { readonly space: "rgb"; readonly channels: readonly [number, number, number, number] }
  | { readonly space: "hsl"; readonly channels: readonly [number, number, number, number] };

/** A solid tint or a two-stop linear gradient, angle in degrees. */
export type Paint =
  | { readonly paint: "solid"; readonly tint: Tint }
  | { readonly paint: "linear"; readonly angle: number; readonly from: readonly [Tint, number]; readonly to: readonly [Tint, number]; readonly oklab: boolean };

/** One primitive in scene units. */
export type Shape =
  | { readonly shape: "rect"; readonly x: number; readonly y: number; readonly w: number; readonly h: number; readonly angle: number; readonly colour: number; readonly depth: number }
  | { readonly shape: "disc"; readonly x: number; readonly y: number; readonly radius: number; readonly colour: number; readonly depth: number }
  | { readonly shape: "strip"; readonly points: readonly Point[]; readonly colour: number; readonly stroke: number; readonly depth: number }
  | { readonly shape: "sprite"; readonly atlas: number; readonly x: number; readonly y: number; readonly w: number; readonly h: number; readonly angle: number; readonly tint: number; readonly depth: number }
  | { readonly shape: "glyphs"; readonly text: string; readonly x: number; readonly y: number; readonly size: number; readonly colour: number; readonly depth: number }
  | { readonly shape: "path"; readonly steps: readonly Step[]; readonly outline: Outline; readonly paint: Paint; readonly transform: { readonly x: number; readonly y: number; readonly scale: number } | null; readonly depth: number }
  | { readonly shape: "lettering"; readonly text: string; readonly x: number; readonly y: number; readonly size: number; readonly colour: number; readonly depth: number; readonly anchor: number; readonly weight: number };

/** One accessible element a scene describes ([UI-MACHINE-A11Y]). */
export interface Access {
  readonly role: "button" | "slider" | "menuitem";
  readonly label: string;
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
  readonly value: number;
  readonly min: number;
  readonly max: number;
  readonly selected: boolean;
}

/** A canvas's scene: its extent, zoom, atlas, shapes and accessibility tree. */
export interface Scene {
  readonly width: number;
  readonly height: number;
  readonly zoom: number;
  readonly atlas: readonly string[];
  readonly shapes: readonly Shape[];
  readonly access: readonly Access[];
}

/** A checked frame: its controls (a row's index is its position), the roots, and each canvas's scene. */
export interface Shown {
  readonly controls: readonly Control[];
  readonly roots: readonly number[];
  readonly scenes: ReadonlyMap<number, Scene>;
}

/** The four sealed runs of one call, copied out of the application's linear memory. */
export interface FrameRuns {
  readonly shapes: ArrayBuffer;
  readonly views: ArrayBuffer;
  readonly text: ArrayBuffer;
}

export { SCENE_SHAPE_BYTES, VIEW_ROW_BYTES };

/** The UTF-8 bytes `offset..offset + length` of the run, checked. */
export function textAt(run: Uint8Array, offset: number, length: number): string {
  if (offset < 0) throw new Error("a text offset below zero");
  if (length < 0) throw new Error("a text length below zero");
  if (offset + length > run.length) throw new Error("a text range outside the frame");
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(run.subarray(offset, offset + length));
  } catch {
    throw new Error("text that is not UTF-8");
  }
}

/** A generated `Int` cell as a JavaScript number, refusing values a double cannot hold exactly. */
export function whole(view: DataView, offset: number): number {
  const value = view.getBigInt64(offset, true);
  if (value > BigInt(Number.MAX_SAFE_INTEGER) || value < BigInt(Number.MIN_SAFE_INTEGER)) {
    throw new Error("an integer outside the range this host can read exactly");
  }
  return Number(value);
}

function colour(view: DataView, offset: number): number {
  const value = whole(view, offset);
  if (value < 0 || value > 0xffffffff) throw new Error("a colour outside 0xRRGGBBAA");
  return value;
}

function extent(view: DataView, offset: number): number {
  const value = view.getFloat64(offset, true);
  if (!Number.isFinite(value) || value < 0) throw new Error("an extent that is negative or not finite");
  return value;
}

/** One told row ([NATIVE-SHM-NODES]): its name is its hint, a field's value is its words, its
 * whole number (its `colour`, which is not a colour here) or its real, and its weight says
 * which type it is: 0 text, 1 whole number, 2 real, 3 flag, 4 a negative real. */
function toldRow(kind: Kind, row: DataView, at: number, index: number, run: Uint8Array): Control {
  const weight = whole(row, at + VIEW_ROW.weight);
  if (weight < 0 || weight > 4) throw new Error(`row ${index} has a told value of an unknown type ${weight}`);
  return {
    kind,
    action: null,
    words: textAt(run, whole(row, at + VIEW_ROW.textOffset), whole(row, at + VIEW_ROW.textLength)),
    hint: textAt(run, whole(row, at + VIEW_ROW.hintOffset), whole(row, at + VIEW_ROW.hintLength)),
    look: { ink: 0, fill: 0, size: extent(row, at + VIEW_ROW.size), padding: 0, spacing: 0, radius: 0, weight },
    fit: { width: 0, height: 0, grow: 0, flags: 0, border: 0 },
    value: whole(row, at + VIEW_ROW.colour),
    children: [],
  };
}

function control(row: DataView, at: number, index: number, run: Uint8Array): Control {
  const code = whole(row, at + VIEW_ROW.kind);
  const kind = KINDS.get(code);
  if (kind === undefined) throw new Error(`row ${index} has an unknown kind ${code}`);
  if (told(kind)) return toldRow(kind, row, at, index, run);
  const flags = whole(row, at + VIEW_ROW.flags);
  if (flags < 0 || flags > 0xffffffff || (flags & ~FLAGS.all) !== 0) throw new Error(`row ${index} has unknown flags ${flags}`);
  const weight = whole(row, at + VIEW_ROW.weight);
  if (weight < 100 || weight > 900) throw new Error(`row ${index} has a font weight outside 100 to 900`);
  const action = whole(row, at + VIEW_ROW.action);
  return {
    kind,
    action: action >= 0 ? action : null,
    words: textAt(run, whole(row, at + VIEW_ROW.textOffset), whole(row, at + VIEW_ROW.textLength)),
    hint: textAt(run, whole(row, at + VIEW_ROW.hintOffset), whole(row, at + VIEW_ROW.hintLength)),
    look: {
      ink: colour(row, at + VIEW_ROW.colour),
      fill: colour(row, at + VIEW_ROW.background),
      size: extent(row, at + VIEW_ROW.size),
      padding: extent(row, at + VIEW_ROW.padding),
      spacing: extent(row, at + VIEW_ROW.spacing),
      radius: extent(row, at + VIEW_ROW.radius),
      weight,
    },
    fit: {
      width: extent(row, at + VIEW_ROW.width),
      height: extent(row, at + VIEW_ROW.height),
      grow: extent(row, at + VIEW_ROW.grow),
      flags,
      border: colour(row, at + VIEW_ROW.border),
    },
    value: 0,
    children: [],
  };
}

/** The controls, each child listed under its parent, and the rows with no parent. */
function controls(run: ArrayBuffer, textRun: Uint8Array): { controls: Control[]; roots: number[] } {
  if (run.byteLength % VIEW_ROW_BYTES !== 0) throw new Error("the view run is not a whole number of rows");
  const rows = new DataView(run);
  const all: Control[] = [];
  const roots: number[] = [];
  for (let index = 0; index < run.byteLength / VIEW_ROW_BYTES; index += 1) {
    const at = index * VIEW_ROW_BYTES;
    all.push(control(rows, at, index, textRun));
    const parent = whole(rows, at + VIEW_ROW.parent);
    if (parent === -1) {
      roots.push(index);
    } else {
      if (parent < 0 || parent >= index) throw new Error(`row ${index} names a parent that is not an earlier row`);
      all[parent]!.children.push(index);
    }
  }
  return { controls: all, roots };
}

interface SceneRow {
  readonly view: DataView;
  readonly at: number;
}

const POINT = 7;

/** Scene rows in order, with the next one taken on demand; every count is checked against what is left. */
class Cursor {
  private index = 0;
  constructor(
    private readonly rows: readonly SceneRow[],
    private readonly run: Uint8Array,
  ) {}

  get more(): boolean {
    return this.index < this.rows.length;
  }

  head(): SceneRow {
    const row = this.rows[this.index];
    if (!row) throw new Error("a scene row past the end of the frame");
    finite(row);
    this.index += 1;
    return row;
  }

  take(code: number): SceneRow {
    const row = this.rows[this.index];
    if (!row || whole(row.view, row.at + SCENE_SHAPE.code) !== code) throw new Error(`scene row ${this.index} should have code ${code}`);
    finite(row);
    this.index += 1;
    return row;
  }

  private count(value: number): number {
    if (!Number.isSafeInteger(value) || value < 0 || value > this.rows.length - this.index) {
      throw new Error("a scene count that runs past its rows");
    }
    return value;
  }

  points(count: number): Point[] {
    const length = this.count(count);
    const points: Point[] = [];
    for (let at = 0; at < length; at += 1) {
      const row = this.take(POINT);
      points.push({ x: f(row, 0), y: f(row, 1) });
    }
    return points;
  }

  text(row: SceneRow): string {
    return textAt(this.run, whole(row.view, row.at + SCENE_SHAPE.textOffset), whole(row.view, row.at + SCENE_SHAPE.textLength));
  }
}

function f(row: SceneRow, at: number): number {
  return row.view.getFloat64(row.at + SCENE_SHAPE.x + at * 8, true);
}

function finite(row: SceneRow): void {
  for (let at = 0; at < 5; at += 1) {
    if (!Number.isFinite(row.view.getFloat64(row.at + SCENE_SHAPE.x + at * 8, true))) throw new Error("a number that is not finite");
  }
}

function tint(rows: Cursor): Tint {
  const row = rows.take(11);
  const channels = [f(row, 0), f(row, 1), f(row, 2), f(row, 3)] as const;
  const index = whole(row.view, row.at + SCENE_SHAPE.index);
  if (index === 0) return { space: "rgb", channels };
  if (index === 1) return { space: "hsl", channels };
  throw new Error("an unknown tint");
}

function step(rows: Cursor): Step {
  const row = rows.take(8);
  const at = (x: number, y: number): Point => ({ x: f(row, x), y: f(row, y) });
  const index = whole(row.view, row.at + SCENE_SHAPE.index);
  if (index === 0) return { step: "move", to: at(0, 1) };
  if (index === 1) return { step: "line", to: at(0, 1) };
  if (index === 2) return { step: "curve", to: at(0, 1), control: at(2, 3) };
  if (index === 3) {
    const last = rows.take(POINT);
    return { step: "cubic", to: at(0, 1), first: at(2, 3), second: { x: f(row, 4), y: f(last, 0) } };
  }
  if (index === 4) {
    const colourValue = whole(row.view, row.at + SCENE_SHAPE.colour);
    return { step: "arc", radii: at(0, 1), rotation: f(row, 2), large: (colourValue & 1) !== 0, sweep: (colourValue & 2) !== 0, to: at(3, 4) };
  }
  if (index === 5) {
    const depth = whole(row.view, row.at + SCENE_SHAPE.depth);
    return { step: "polygon", corners: rows.points(depth), closed: (whole(row.view, row.at + SCENE_SHAPE.colour) & 1) !== 0 };
  }
  if (index === 6) return { step: "close" };
  throw new Error("an unknown path step");
}

function outline(rows: Cursor): Outline {
  const row = rows.take(9);
  const index = whole(row.view, row.at + SCENE_SHAPE.index);
  if (index === 0) return { outline: "fill" };
  if (index === 1) {
    const depth = whole(row.view, row.at + SCENE_SHAPE.depth);
    return { outline: "stroke", width: f(row, 0), bevel: (whole(row.view, row.at + SCENE_SHAPE.colour) & 1) !== 0,
      dashes: rows.points(depth).map((point) => point.x) };
  }
  throw new Error("an unknown outline");
}

function paint(rows: Cursor): Paint {
  const row = rows.take(10);
  const index = whole(row.view, row.at + SCENE_SHAPE.index);
  if (index === 0) return { paint: "solid", tint: tint(rows) };
  if (index === 1) {
    const angle = f(row, 0);
    return { paint: "linear", angle, from: [tint(rows), f(row, 1)], to: [tint(rows), f(row, 2)], oklab: (whole(row.view, row.at + SCENE_SHAPE.colour) & 1) !== 0 };
  }
  throw new Error("an unknown paint");
}

function path(rows: Cursor, head: SceneRow): Shape {
  const count = whole(head.view, head.at + SCENE_SHAPE.index);
  if (count < 0 || count > 0x7fffffff) throw new Error("a scene count that runs past its rows");
  const steps: Step[] = [];
  for (let at = 0; at < count; at += 1) steps.push(step(rows));
  const moved = { x: f(head, 0), y: f(head, 1) };
  const scale = f(head, 2);
  return {
    shape: "path",
    steps,
    outline: outline(rows),
    paint: paint(rows),
    transform: moved.x !== 0 || moved.y !== 0 || scale !== 1 ? { x: moved.x, y: moved.y, scale } : null,
    depth: whole(head.view, head.at + SCENE_SHAPE.depth),
  };
}

function shape(rows: Cursor, head: SceneRow): Shape {
  const code = whole(head.view, head.at);
  const depth = whole(head.view, head.at + SCENE_SHAPE.depth);
  switch (code) {
    case 0:
      return { shape: "rect", x: f(head, 0), y: f(head, 1), w: f(head, 2), h: f(head, 3), angle: f(head, 4), colour: colour(head.view, head.at + SCENE_SHAPE.colour), depth };
    case 1:
      return { shape: "disc", x: f(head, 0), y: f(head, 1), radius: f(head, 2), colour: colour(head.view, head.at + SCENE_SHAPE.colour), depth };
    case 2:
      return { shape: "strip", points: rows.points(whole(head.view, head.at + SCENE_SHAPE.index)), colour: colour(head.view, head.at + SCENE_SHAPE.colour), stroke: f(head, 2), depth };
    case 3: {
      const atlas = whole(head.view, head.at + SCENE_SHAPE.index);
      if (atlas < 0) throw new Error("a sprite index below zero");
      return { shape: "sprite", atlas, x: f(head, 0), y: f(head, 1), w: f(head, 2), h: f(head, 3), angle: f(head, 4), tint: colour(head.view, head.at + SCENE_SHAPE.colour), depth };
    }
    case 4:
      return { shape: "glyphs", text: rows.text(head), x: f(head, 0), y: f(head, 1), size: f(head, 2), colour: colour(head.view, head.at + SCENE_SHAPE.colour), depth };
    case 5:
      return path(rows, head);
    case 6: {
      const anchor = whole(head.view, head.at + SCENE_SHAPE.index);
      if (anchor < 0 || anchor > 2) throw new Error("an unknown lettering anchor");
      return { shape: "lettering", text: rows.text(head), x: f(head, 0), y: f(head, 1), size: f(head, 2), colour: colour(head.view, head.at + SCENE_SHAPE.colour), depth, anchor, weight: f(head, 3) };
    }
    default:
      throw new Error(`an unknown scene code ${code}`);
  }
}

function element(rows: Cursor, head: SceneRow): Access | null {
  const range = rows.take(POINT);
  const index = whole(head.view, head.at + SCENE_SHAPE.index);
  const role = index === 0 || index === 2 ? "button" : index === 1 ? "slider" : index === 3 ? "menuitem" : index === 4 ? null : undefined;
  if (role === undefined) throw new Error("an unknown accessible role");
  const label = rows.text(head);
  if (role === null || label === "") return null;
  return {
    role,
    label,
    x: f(head, 0),
    y: f(head, 1),
    w: f(head, 2),
    h: f(head, 3),
    value: f(head, 4),
    min: f(range, 0),
    max: f(range, 1),
    selected: role === "menuitem" && (whole(head.view, head.at + SCENE_SHAPE.colour) & 1) !== 0,
  };
}

function scenes(run: ArrayBuffer, textRun: Uint8Array, canvases: readonly number[]): Map<number, Scene> {
  if (run.byteLength % SCENE_SHAPE_BYTES !== 0) throw new Error("the scene run is not a whole number of rows");
  const all = new Map<number, Scene>();
  const rows: SceneRow[] = [];
  const view = new DataView(run);
  for (let index = 0; index < run.byteLength / SCENE_SHAPE_BYTES; index += 1) {
    rows.push({ view, at: index * SCENE_SHAPE_BYTES });
  }
  const cursor = new Cursor(rows, textRun);
  let current: { width: number; height: number; zoom: number; atlas: string[]; shapes: Shape[]; access: Access[] } | undefined;
  while (cursor.more) {
    const head = cursor.head();
    const code = whole(head.view, head.at);
    if (code === 20) {
      const row = whole(head.view, head.at + SCENE_SHAPE.index);
      if (!canvases.includes(row) || all.has(row)) throw new Error("a scene that names no canvas, or one twice");
      const width = extent(head.view, head.at + SCENE_SHAPE.x);
      const height = extent(head.view, head.at + SCENE_SHAPE.y);
      if ((width === 0) !== (height === 0)) throw new Error("a window-pixel scene has zero width and zero height");
      current = { width, height, zoom: extent(head.view, head.at + SCENE_SHAPE.width), atlas: [], shapes: [], access: [] };
      all.set(row, current);
      continue;
    }
    if (current === undefined) throw new Error("a scene row before any scene");
    if (code === 21) current.atlas.push(cursor.text(head));
    else if (code === 22) {
      const found = element(cursor, head);
      if (found !== null) current.access.push(found);
    } else current.shapes.push(shape(cursor, head));
  }
  return all;
}

/** Checks and reads one sealed frame; a bad row refuses the whole frame. A scene marker may
 * name a canvas row or the record row of a scene component ([NATIVE-SHM-NODES]). */
export function readFrame(runs: FrameRuns): Shown {
  const textRun = new Uint8Array(runs.text);
  const read = controls(runs.views, textRun);
  const canvases = read.controls.flatMap((control, index) => (control.kind === "canvas" || control.kind === "record" ? [index] : []));
  return { controls: read.controls, roots: read.roots, scenes: scenes(runs.shapes, textRun, canvases) };
}

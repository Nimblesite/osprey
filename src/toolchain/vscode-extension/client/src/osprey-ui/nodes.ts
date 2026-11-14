// Implements [NATIVE-SHM-NODES] on the host: the component tree the application told, read from
// the checked rows of the shared frame. A `field` row is one typed value, a `record` row a
// component or one of its members and a `list` row the items under one name. Nothing here is
// parsed from text: every value is a row the frame check already accepted (mirrors the Swift
// host's `Nodes/Tree.swift` and `Nodes/RegionStore.swift`).

import { told, type Control, type Shown } from "./webview/rows.js";

/** A JSON object shape at the renderer boundary: the accessors main's renderer used on nodes. */
export function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** A non-negative integer every JavaScript-backed host can represent exactly. */
export function naturalInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/** One region's current tree, with the revision its cells have drawn it at ([UI-REGIONS]). */
export interface RegionSnapshot {
  readonly revision: number;
  readonly node: unknown;
}

/** Regions by key; the renderer resolves a `slot` against them. */
export type Regions = ReadonlyMap<string, RegionSnapshot>;

/** The frame a host draws: the theme and every region's component tree, by key. */
export interface ToldFrame {
  readonly theme: unknown;
  readonly regions: readonly { readonly key: string; readonly node: unknown }[];
}

/** Structural equality of two told trees, so a region whose tree did not change keeps its
 * revision (mirrors the Swift `Tree`'s value equality). */
export function sameTree(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (Array.isArray(left) && Array.isArray(right)) {
    return left.length === right.length && left.every((item, at) => sameTree(item, right[at]));
  }
  if (record(left) && record(right)) {
    const keys = Object.keys(left);
    return keys.length === Object.keys(right).length && keys.every((key) => key in right && sameTree(left[key], right[key]));
  }
  return false;
}

/** One component record: its `kind` and exactly the members the application told. */
function members(shown: Shown, control: Control): Record<string, unknown> {
  const node: Record<string, unknown> = Object.create(null);
  for (const child of control.children) {
    const member = shown.controls[child];
    if (member === undefined || member.hint === "") continue;
    node[member.hint] = tree(shown, child);
  }
  return node;
}

/** The value of one row and the rows inside it. A record a scene names carries its row index as
 * `sceneRow`, so the canvas finds the scene the frame gave it. */
function tree(shown: Shown, index: number): unknown {
  const control = shown.controls[index];
  if (control === undefined) return null;
  switch (control.kind) {
    case "field":
      switch (control.look.weight) {
        case 1: return control.value;
        case 2: return control.look.size;
        case 3: return control.value !== 0;
        case 4: return -control.look.size;
        default: return control.words;
      }
    case "list":
      return control.children.map((child) => tree(shown, child));
    default: {
      const node = members(shown, control);
      if (shown.scenes.has(index)) node.sceneRow = index;
      return node;
    }
  }
}

function text(shown: Shown, index: number): string {
  const control = shown.controls[index];
  return control === undefined ? "" : control.words;
}

function node(shown: Shown, index: number): unknown {
  const control = shown.controls[index];
  if (control === undefined) return null;
  for (const child of control.children) {
    const member = shown.controls[child];
    if (member !== undefined && member.hint === "node") return tree(shown, child);
  }
  return null;
}

/** The frame's theme and regions, or undefined when the frame tells no components: a frame of
 * plain view rows, which the row view draws. */
export function toldFrame(shown: Shown): ToldFrame | undefined {
  const roots = shown.roots.filter((root) => {
    const kind = shown.controls[root]?.kind;
    return kind !== undefined && told(kind);
  });
  if (roots.length === 0) return undefined;
  let theme: unknown;
  const regions: { key: string; node: unknown }[] = [];
  for (const root of roots) {
    const control = shown.controls[root];
    if (control?.hint === "theme") theme = tree(shown, root);
    else if (control?.hint === "region") {
      let key = "";
      for (const child of control.children) {
        if (shown.controls[child]?.hint === "key") key = text(shown, child);
      }
      regions.push({ key, node: node(shown, root) });
    }
  }
  return { theme, regions };
}

/** Every approved asset source a told frame names: media sources, posters, captions, video
 * tracks and board backdrops. Scene atlases arrive in the scene run and are collected beside
 * these by the session. */
export function sourcesOf(frame: ToldFrame): string[] {
  const sources = new Set<string>();
  const visit = (value: unknown): void => {
    if (Array.isArray(value)) {
      value.forEach(visit);
      return;
    }
    if (!record(value)) return;
    if (value.kind === "image" || value.kind === "video") {
      for (const key of ["source", "poster", "captions"]) {
        if (typeof value[key] === "string" && value[key] !== "") sources.add(value[key] as string);
      }
    }
    if (value.kind === "board" && typeof value.backdrop === "string" && value.backdrop !== "") sources.add(value.backdrop);
    for (const child of Object.values(value)) visit(child);
  };
  visit(frame.theme);
  for (const region of frame.regions) visit(region.node);
  return [...sources];
}

/** Keeps the regions of a frame: a region whose tree did not change keeps its revision, so its
 * controls stay stable while other regions move ([UI-REGIONS]). */
export class RegionStore {
  private cells = new Map<string, RegionSnapshot>();

  /** The regions after adopting `frame`, by key. */
  adopt(frame: ToldFrame): Regions {
    const next = new Map(this.cells);
    for (const region of frame.regions) {
      const held = next.get(region.key);
      if (held !== undefined && sameTree(held.node, region.node)) continue;
      next.set(region.key, { revision: (held?.revision ?? -1) + 1, node: region.node });
    }
    const live = new Set(frame.regions.map((region) => region.key));
    for (const key of [...next.keys()]) if (!live.has(key)) next.delete(key);
    this.cells = next;
    return next;
  }

  /** Forget every region, when the session reopens or closes. */
  clear(): void {
    this.cells = new Map();
  }
}

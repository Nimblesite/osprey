// Pure flame-graph model for the Osprey profiler webview ([PROF-VSCODE-FLAME]).
// Parses the speedscope "sampled" export written by `osprey --run --profile`
// and precomputes everything the canvas renderer needs: a LEFT-HEAVY layout
// (samples merged into an aggregated tree, heaviest children first) and a
// TIME-ORDER layout (a flame chart where adjacent identical stacks fuse into
// continuous slabs), plus per-frame self/total weights, deterministic frame
// colors, and substring search. No vscode imports — everything here is unit
// testable and JSON-serializable for embedding in the webview.

export interface SpeedscopeFrame {
  name: string;
  file?: string;
  line?: number;
}

export interface SpeedscopeProfile {
  type: string;
  name: string;
  unit: string;
  startValue: number;
  endValue: number;
  /** Stacks as ROOT-FIRST frame-index arrays, one per sample. */
  samples: number[][];
  /** Weight of each sample, in `unit` (seconds for Osprey exports). */
  weights: number[];
}

export interface SpeedscopeFile {
  shared: { frames: SpeedscopeFrame[] };
  profiles: SpeedscopeProfile[];
}

export type Result<T> = { ok: true; value: T } | { ok: false; error: string };

/** One renderable slab. x0/x1 are in [0,1] config space; weights in seconds. */
export interface RenderRect {
  x0: number;
  x1: number;
  depth: number;
  frameIdx: number;
  selfWeight: number;
  totalWeight: number;
}

/** Per-frame aggregates for one fiber, indexed by frameIdx. */
export interface FrameStats {
  self: number[];
  total: number[];
  count: number[];
}

export interface FiberFlame {
  name: string;
  totalWeight: number;
  sampleCount: number;
  maxDepthLeft: number;
  maxDepthTime: number;
  leftHeavy: RenderRect[];
  timeOrder: RenderRect[];
  stats: FrameStats;
}

export interface FrameMeta {
  name: string;
  file: string;
  line: number;
  color: string;
}

export interface FlameModel {
  frames: FrameMeta[];
  fibers: FiberFlame[];
}

// --- parsing -----------------------------------------------------------------

function isFrame(value: unknown): value is SpeedscopeFrame {
  return (
    typeof value === "object" &&
    value !== null &&
    typeof (value as SpeedscopeFrame).name === "string"
  );
}

function profileError(p: SpeedscopeProfile, frameCount: number): string | undefined {
  if (p.type !== "sampled") {
    return `profile "${p.name}" has unsupported type "${p.type}"`;
  }
  if (!Array.isArray(p.samples) || !Array.isArray(p.weights)) {
    return `profile "${p.name}" is missing samples/weights`;
  }
  if (p.samples.length !== p.weights.length) {
    return `profile "${p.name}" has ${p.samples.length} samples but ${p.weights.length} weights`;
  }
  const badStack = p.samples.find(
    (s) => !Array.isArray(s) || s.some((f) => !Number.isInteger(f) || f < 0 || f >= frameCount),
  );
  return badStack === undefined
    ? undefined
    : `profile "${p.name}" references a frame outside shared.frames`;
}

/** JSON.parse into a Result — shared by both profiler artifact parsers. */
export function parseJsonResult(text: string, what: string): Result<unknown> {
  try {
    return { ok: true, value: JSON.parse(text) };
  } catch (error) {
    return { ok: false, error: `${what} is not valid JSON: ${String(error)}` };
  }
}

/** Parse + validate a speedscope JSON document. Never throws. */
export function parseSpeedscope(text: string): Result<SpeedscopeFile> {
  const raw = parseJsonResult(text, "speedscope file");
  if (!raw.ok) {
    return raw;
  }
  const doc = raw.value as SpeedscopeFile;
  if (!Array.isArray(doc?.shared?.frames) || !doc.shared.frames.every(isFrame)) {
    return { ok: false, error: "speedscope file is missing shared.frames" };
  }
  if (!Array.isArray(doc.profiles) || doc.profiles.length === 0) {
    return { ok: false, error: "speedscope file has no profiles" };
  }
  const bad = doc.profiles.map((p) => profileError(p, doc.shared.frames.length)).find(Boolean);
  return bad ? { ok: false, error: bad } : { ok: true, value: doc };
}

// --- left-heavy layout ---------------------------------------------------------

interface AggNode {
  frameIdx: number;
  total: number;
  self: number;
  children: Map<number, AggNode>;
}

function childOf(children: Map<number, AggNode>, frameIdx: number): AggNode {
  const existing = children.get(frameIdx);
  if (existing !== undefined) {
    return existing;
  }
  const node: AggNode = { frameIdx, total: 0, self: 0, children: new Map() };
  children.set(frameIdx, node);
  return node;
}

function insertStack(root: Map<number, AggNode>, stack: number[], weight: number): void {
  let children = root;
  let node: AggNode | undefined;
  for (const frameIdx of stack) {
    node = childOf(children, frameIdx);
    node.total += weight;
    children = node.children;
  }
  if (node !== undefined) {
    node.self += weight;
  }
}

function sortedByWeight(children: Map<number, AggNode>): AggNode[] {
  return [...children.values()].sort(
    (a, b) => b.total - a.total || a.frameIdx - b.frameIdx,
  );
}

function emitAggNodes(
  nodes: AggNode[],
  x0: number,
  depth: number,
  scale: number,
  out: RenderRect[],
): void {
  let x = x0;
  for (const node of nodes) {
    out.push({
      x0: x * scale,
      x1: (x + node.total) * scale,
      depth,
      frameIdx: node.frameIdx,
      selfWeight: node.self,
      totalWeight: node.total,
    });
    emitAggNodes(sortedByWeight(node.children), x, depth + 1, scale, out);
    x += node.total;
  }
}

/** Merge samples into the aggregated left-heavy layout (heaviest first). */
export function leftHeavyRects(profile: SpeedscopeProfile): RenderRect[] {
  const total = profile.weights.reduce((sum, w) => sum + w, 0);
  if (total <= 0) {
    return [];
  }
  const root: Map<number, AggNode> = new Map();
  profile.samples.forEach((stack, i) => insertStack(root, stack, profile.weights[i]));
  const out: RenderRect[] = [];
  emitAggNodes(sortedByWeight(root), 0, 0, 1 / total, out);
  return out;
}

// --- time-order layout ----------------------------------------------------------

interface OpenFrame {
  frameIdx: number;
  x0: number;
  self: number;
}

function commonPrefixLength(open: OpenFrame[], stack: number[]): number {
  let lcp = 0;
  while (lcp < open.length && lcp < stack.length && open[lcp].frameIdx === stack[lcp]) {
    lcp += 1;
  }
  return lcp;
}

function closeFramesAbove(
  open: OpenFrame[],
  keep: number,
  cursor: number,
  out: RenderRect[],
): void {
  while (open.length > keep) {
    const frame = open.pop();
    if (frame !== undefined) {
      out.push({
        x0: frame.x0,
        x1: cursor,
        depth: open.length,
        frameIdx: frame.frameIdx,
        selfWeight: frame.self,
        totalWeight: cursor - frame.x0,
      });
    }
  }
}

function normalizeRects(rects: RenderRect[], total: number): RenderRect[] {
  return rects
    .map((r) => ({ ...r, x0: r.x0 / total, x1: r.x1 / total }))
    .sort((a, b) => a.depth - b.depth || a.x0 - b.x0);
}

/** Flame-chart layout: x = time; adjacent identical stacks merge into slabs. */
export function timeOrderRects(profile: SpeedscopeProfile): RenderRect[] {
  const out: RenderRect[] = [];
  const open: OpenFrame[] = [];
  let cursor = 0;
  profile.samples.forEach((stack, i) => {
    const weight = profile.weights[i];
    closeFramesAbove(open, commonPrefixLength(open, stack), cursor, out);
    for (let d = open.length; d < stack.length; d += 1) {
      open.push({ frameIdx: stack[d], x0: cursor, self: 0 });
    }
    if (open.length > 0) {
      open[open.length - 1].self += weight;
    }
    cursor += weight;
  });
  closeFramesAbove(open, 0, cursor, out);
  return cursor > 0 ? normalizeRects(out, cursor) : [];
}

// --- per-frame aggregates ---------------------------------------------------------

/** Self/total weight and sample count per frameIdx (recursion deduplicated). */
export function frameStats(profile: SpeedscopeProfile, frameCount: number): FrameStats {
  const stats: FrameStats = {
    self: new Array<number>(frameCount).fill(0),
    total: new Array<number>(frameCount).fill(0),
    count: new Array<number>(frameCount).fill(0),
  };
  profile.samples.forEach((stack, i) => {
    const weight = profile.weights[i];
    for (const frameIdx of new Set(stack)) {
      stats.total[frameIdx] += weight;
      stats.count[frameIdx] += 1;
    }
    if (stack.length > 0) {
      stats.self[stack[stack.length - 1]] += weight;
    }
  });
  return stats;
}

// --- colors -----------------------------------------------------------------------

/** Triangle wave in [0,1] with period 2 — speedscope's color mixer. */
export const triangle = (v: number): number =>
  2 * Math.abs(v / 2 - Math.floor(v / 2 + 0.5));

/** Osprey source frames get the warm ramp; runtime frames a gray-blue. */
export function isOspreySource(file: string): boolean {
  return file.endsWith(".osp") || file.endsWith(".ospml");
}

const RUNTIME_HUE = 215;

/** Deterministic speedscope-style color for the frame ranked `rank` of `n`. */
export function colorForRank(file: string, rank: number, n: number): string {
  const bucket = Math.floor((255 * rank) / Math.max(n, 1));
  const t = bucket / 255;
  const x = triangle(30 * t);
  if (!isOspreySource(file)) {
    return `hsl(${RUNTIME_HUE}, 12%, ${(58 - 12 * x).toFixed(1)}%)`;
  }
  return `hsl(${(324 * t).toFixed(1)}, ${(25 + 20 * x).toFixed(1)}%, ${(65 - 15 * x).toFixed(1)}%)`;
}

/** Stable per-frame colors: frames ranked by (file+name), ramped over hue. */
export function frameColors(frames: SpeedscopeFrame[]): string[] {
  const order = frames
    .map((frame, i) => ({ key: `${frame.file ?? ""} ${frame.name}`, i }))
    .sort((a, b) => (a.key < b.key ? -1 : a.key > b.key ? 1 : a.i - b.i));
  const colors = new Array<string>(frames.length);
  order.forEach(({ i }, rank) => {
    colors[i] = colorForRank(frames[i].file ?? "", rank, frames.length);
  });
  return colors;
}

// --- search ------------------------------------------------------------------------

/** Case-insensitive substring match over frame names → set of frameIdx. */
export function searchMatches(frames: SpeedscopeFrame[], query: string): Set<number> {
  const needle = query.trim().toLowerCase();
  if (needle === "") {
    return new Set();
  }
  const hits = frames.flatMap((frame, i) =>
    frame.name.toLowerCase().includes(needle) ? [i] : [],
  );
  return new Set(hits);
}

// --- assembly ------------------------------------------------------------------------

const maxDepth = (rects: RenderRect[]): number =>
  rects.reduce((deepest, r) => Math.max(deepest, r.depth + 1), 0);

function buildFiber(profile: SpeedscopeProfile, frameCount: number): FiberFlame {
  const leftHeavy = leftHeavyRects(profile);
  const timeOrder = timeOrderRects(profile);
  return {
    name: profile.name,
    totalWeight: profile.weights.reduce((sum, w) => sum + w, 0),
    sampleCount: profile.samples.length,
    maxDepthLeft: maxDepth(leftHeavy),
    maxDepthTime: maxDepth(timeOrder),
    leftHeavy,
    timeOrder,
    stats: frameStats(profile, frameCount),
  };
}

/** The complete serializable model for one speedscope document. */
export function buildFlameModel(doc: SpeedscopeFile): FlameModel {
  const colors = frameColors(doc.shared.frames);
  return {
    frames: doc.shared.frames.map((frame, i) => ({
      name: frame.name,
      file: frame.file ?? "",
      line: frame.line ?? 0,
      color: colors[i],
    })),
    fibers: doc.profiles.map((p) => buildFiber(p, doc.shared.frames.length)),
  };
}

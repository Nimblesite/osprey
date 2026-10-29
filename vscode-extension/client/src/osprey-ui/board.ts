// Mechanical rendering of the host-held board slice defined by Osprey.
import { boardStroke } from "./board-value";
import { escapeHtml } from "./html";
import { naturalInteger, record } from "./wire";

function integer(value: unknown, least: number, most: number): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= least && value <= most;
}

function validMetadata(value: unknown): value is Record<string, unknown> {
  if (!record(value) || value.version !== 1 || !record(value.size) || !record(value.view)) return false;
  return integer(value.size.width, 1, 16384) && integer(value.size.height, 1, 16384)
    && integer(value.view.x, -1000000, 1000000) && integer(value.view.y, -1000000, 1000000) && integer(value.view.scale, 25, 400);
}

function slice(slices: ReadonlyMap<string, string> | undefined, key: string): unknown {
  const source = slices?.get(key);
  if (source === undefined) throw new Error("The drawing board's host-held slice is missing");
  try { return JSON.parse(source); } catch { throw new Error("The drawing board's host-held slice is not JSON"); }
}

function drawing(slices: ReadonlyMap<string, string> | undefined, base: string, count: unknown): string {
  const metadata = slice(slices, base);
  if (!validMetadata(metadata) || !naturalInteger(count) || count > 10000) throw new Error("The drawing board's metadata or stroke count is malformed");
  const strokes = Array.from({ length: count }, (_, index) => slice(slices, `${base}:${index}`));
  if (!strokes.every(boardStroke)) throw new Error("The drawing board has a malformed stroke slice");
  return JSON.stringify({ ...metadata, strokes });
}

/** Canvas receives retained strokes and framing; application code owns all board transitions. */
export function renderBoard(node: Record<string, unknown>, control: string, slices?: ReadonlyMap<string, string>): string {
  if (typeof node.slice !== "string" || !naturalInteger(node.lassoEvent)) throw new Error("The drawing board has no issued slice or lasso binding");
  const state = drawing(slices, node.slice, node.count);
  return `<div class="osp-board"><canvas tabindex="0" role="img" aria-label="Drawing board" data-board-slice="${escapeHtml(node.slice)}" data-board-state="${escapeHtml(state)}" data-lasso-enabled="${node.lasso === true}" data-lasso-ordinal="${node.lassoEvent}"${control}></canvas></div>`;
}

// Sort order and filter of the Module Explorer's code elements [EDITOR-MODULE-EXPLORER],
// cycled and typed from the view's title bar as SharpLsp's Solution Explorer does.

/** What arrange reads of a tree node. */
export interface Arrangeable {
  readonly label: string;
  /** The declaration kind; absent on anything that is not a code element. */
  readonly kind?: string;
  readonly location?: { readonly file: string; readonly line: number };
  readonly children?: readonly Arrangeable[];
}

export type SortOrder = "alphabetical" | "source" | "kind";
export const SORT_ORDERS: readonly SortOrder[] = ["alphabetical", "source", "kind"];
const KIND_RANK = ["namespace", "module", "type", "effect", "function", "extern", "variable"];

const byLabel = (a: Arrangeable, b: Arrangeable) => a.label.localeCompare(b.label);
const bySource = (a: Arrangeable, b: Arrangeable) =>
  (a.location?.file ?? "").localeCompare(b.location?.file ?? "") || (a.location?.line ?? 0) - (b.location?.line ?? 0);
const byKind = (a: Arrangeable, b: Arrangeable) =>
  KIND_RANK.indexOf(a.kind ?? "") - KIND_RANK.indexOf(b.kind ?? "") || byLabel(a, b);
const COMPARE: Readonly<Record<SortOrder, (a: Arrangeable, b: Arrangeable) => number>> = {
  alphabetical: byLabel, source: bySource, kind: byKind,
};

/** The order after `order`, wrapping round. */
export const nextOrder = (order: SortOrder): SortOrder =>
  SORT_ORDERS[(SORT_ORDERS.indexOf(order) + 1) % SORT_ORDERS.length];

/** Whether `node` or anything under it names `text` (already lower-case). */
function matches(node: Arrangeable, text: string): boolean {
  return node.label.toLowerCase().includes(text) || (node.children ?? []).some((child) => matches(child, text));
}

/** Code elements kept by `filter` and sorted by `order`; anything else passes through untouched. */
export function arrange<T extends Arrangeable>(nodes: readonly T[], order: SortOrder, filter: string): T[] {
  return nodes.some((node) => node.kind !== undefined)
    ? nodes.filter((node) => !filter || matches(node, filter)).sort(COMPARE[order])
    : [...nodes];
}

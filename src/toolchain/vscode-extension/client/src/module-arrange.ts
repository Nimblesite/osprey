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

/** A declaration's node: always kinded, located and given its (possibly empty) children. */
interface CodeElement extends Arrangeable {
  readonly kind: string;
  readonly location: { readonly file: string; readonly line: number };
  readonly children: readonly Arrangeable[];
}

export type SortOrder = "alphabetical" | "source" | "kind";
export const SORT_ORDERS: readonly SortOrder[] = ["alphabetical", "source", "kind"];
const NEXT_ORDER: Readonly<Record<SortOrder, SortOrder>> = { alphabetical: "source", source: "kind", kind: "alphabetical" };
const KIND_RANK = ["namespace", "module", "type", "effect", "function", "extern", "variable"];

const byLabel = (a: CodeElement, b: CodeElement) => a.label.localeCompare(b.label);
const bySource = (a: CodeElement, b: CodeElement) =>
  a.location.file.localeCompare(b.location.file) || a.location.line - b.location.line;
const byKind = (a: CodeElement, b: CodeElement) =>
  KIND_RANK.indexOf(a.kind) - KIND_RANK.indexOf(b.kind) || byLabel(a, b);
const COMPARE: Readonly<Record<SortOrder, (a: CodeElement, b: CodeElement) => number>> = {
  alphabetical: byLabel, source: bySource, kind: byKind,
};

/** The order after `order`, wrapping round. */
export const nextOrder = (order: SortOrder): SortOrder => NEXT_ORDER[order];

const isCodeElement = <T extends Arrangeable>(node: T): node is T & CodeElement =>
  node.kind !== undefined && node.location !== undefined && node.children !== undefined;

/** Whether `node` or anything under it names `text` (already lower-case). */
function matches(node: CodeElement, text: string): boolean {
  return node.label.toLowerCase().includes(text) || node.children.filter(isCodeElement).some((child) => matches(child, text));
}

/** Code elements kept by `filter` and sorted by `order`; anything else passes through untouched. */
export function arrange<T extends Arrangeable>(nodes: readonly T[], order: SortOrder, filter: string): T[] {
  const elements = nodes.filter(isCodeElement);
  return elements.length
    ? elements.filter((node) => !filter || matches(node, filter)).sort(COMPARE[order])
    : [...nodes];
}

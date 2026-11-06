// Implements [UI-COMPONENTS-MATH]: mechanical MathML
// rendering of Osprey's typed layout tree. No parsing or proof decisions.
import { escapeHtml } from "./html";
import { record } from "./protocol";

type MathNode = Record<string, unknown>;
type Visit = (depth: number) => void;
type Render = (value: unknown) => string;
type Layout = (node: MathNode, render: Render) => string;

function text(node: MathNode, key: string): string {
  if (typeof node[key] !== "string") throw new Error(`Mathematics ${key} must be text`);
  return escapeHtml(node[key]);
}

function items(node: MathNode, key: string, render: Render): string {
  if (!Array.isArray(node[key])) throw new Error(`Mathematics ${key} must be a list`);
  return node[key].map(render).join("");
}

function binder(node: MathNode, render: Render): string {
  return `<mrow><mo>${text(node, "symbol")}</mo><mrow>${items(node, "names", render)}</mrow><mo>,</mo>${render(node.body)}</mrow>`;
}

function big(node: MathNode, render: Render): string {
  const limits = `<munderover><mo largeop="true" movablelimits="true">${text(node, "symbol")}</mo>${render(node.lower)}${render(node.upper)}</munderover>`;
  return `<mrow>${limits}${render(node.body)}</mrow>`;
}

function symbol(node: MathNode): string {
  const value = text(node, "text");
  const tag = typeof node.text === "string" && /^\p{Sm}+$/u.test(node.text) ? "mo" : "mi";
  return `<${tag}>${value}</${tag}>`;
}

const LAYOUTS: Readonly<Record<string, Layout>> = {
  sym: symbol,
  num: (node) => `<mn>${text(node, "text")}</mn>`,
  seq: (node, render) => `<mrow>${items(node, "items", render)}</mrow>`,
  sup: (node, render) => `<msup>${render(node.base)}${render(node.script)}</msup>`,
  sub: (node, render) => `<msub>${render(node.base)}${render(node.script)}</msub>`,
  frac: (node, render) => `<mfrac>${render(node.top)}${render(node.bottom)}</mfrac>`,
  root: (node, render) => `<msqrt>${render(node.body)}</msqrt>`,
  binder,
  op: (node, render) => `<mrow>${render(node.left)}<mo>${text(node, "symbol")}</mo>${render(node.right)}</mrow>`,
  paren: (node, render) => `<mrow><mo fence="true" stretchy="true">(</mo>${render(node.body)}<mo fence="true" stretchy="true">)</mo></mrow>`,
  big,
};

function layout(value: unknown, depth: number, visit: Visit): string {
  visit(depth);
  if (!record(value) || typeof value.kind !== "string") throw new Error("Mathematics node must have a kind");
  const renderer = Object.prototype.hasOwnProperty.call(LAYOUTS, value.kind) ? LAYOUTS[value.kind] : undefined;
  if (!renderer) throw new Error(`Unknown mathematics layout: ${value.kind}`);
  return renderer(value, (child) => layout(child, depth + 1, visit));
}

/** Typeset a complete formula; the application's Unicode text names it for assistive technology. */
export function renderMath(tree: unknown, spoken: unknown, visit: Visit): string {
  if (typeof spoken !== "string" || spoken.trim() === "") throw new Error("Mathematics requires accessible text");
  return `<math class="osp-math" xmlns="http://www.w3.org/1998/Math/MathML" aria-label="${escapeHtml(spoken)}">${layout(tree, 1, visit)}</math>`;
}

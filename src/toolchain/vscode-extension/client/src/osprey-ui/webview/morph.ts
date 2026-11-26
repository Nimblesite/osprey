// Implements [UI-HOST-MORPH]: a told frame updates the live DOM in place instead of replacing it, so
// the node under the person's pointer, caret or selection survives every frame. Replacing the root's
// markup on each caret report destroyed the focused editor mid-drag, which made selection flicker.

type Field = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

function syncAttributes(current: Element, next: Element): void {
  for (const attribute of Array.from(current.attributes)) {
    if (!next.hasAttribute(attribute.name)) current.removeAttribute(attribute.name);
  }
  for (const attribute of Array.from(next.attributes)) {
    if (current.getAttribute(attribute.name) !== attribute.value) current.setAttribute(attribute.name, attribute.value);
  }
}

function isField(node: Node): node is Field {
  return node instanceof HTMLInputElement || node instanceof HTMLTextAreaElement || node instanceof HTMLSelectElement;
}

/** A field takes the frame's value unless it is focused with a draft the frame has not seen yet. */
function syncField(current: Field, next: Field, drafting: (field: Element) => boolean): void {
  if (current instanceof HTMLInputElement && next instanceof HTMLInputElement) current.checked = next.checked;
  if (current.value === next.value || (current === document.activeElement && drafting(current))) return;
  const live = current === document.activeElement && !(current instanceof HTMLSelectElement) ? current : null;
  const start = live?.selectionStart ?? null, end = live?.selectionEnd ?? null;
  current.value = next.value;
  if (live !== null && start !== null && end !== null) live.setSelectionRange(start, end, live.selectionDirection ?? "none");
}

function sameKind(current: Node, next: Node): boolean {
  return current.nodeType === next.nodeType && current.nodeName === next.nodeName;
}

function morphChildren(current: Node, next: Node, drafting: (field: Element) => boolean): void {
  const incoming = Array.from(next.childNodes);
  incoming.forEach((node, index) => {
    const live = current.childNodes[index];
    if (live === undefined) current.appendChild(node);
    else if (sameKind(live, node)) morphNode(live, node, drafting);
    else live.replaceWith(node);
  });
  while (current.childNodes.length > incoming.length) current.lastChild?.remove();
}

function morphNode(current: Node, next: Node, drafting: (field: Element) => boolean): void {
  if (!(current instanceof Element) || !(next instanceof Element)) {
    if (current.nodeValue !== next.nodeValue) current.nodeValue = next.nodeValue;
    return;
  }
  syncAttributes(current, next);
  morphChildren(current, next, drafting);
  if (isField(current) && isField(next)) syncField(current, next, drafting);
}

/** Bring `root`'s children in line with `html`, keeping every node whose place and kind still match. */
export function morphFrame(root: HTMLElement, html: string, drafting: (field: Element) => boolean): void {
  const template = document.createElement("template");
  template.innerHTML = html;
  morphChildren(root, template.content, drafting);
}

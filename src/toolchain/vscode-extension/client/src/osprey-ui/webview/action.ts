// Preserve the actual pressed control and its issued binding while the rest of a frame updates
// (restored from the JSON-protocol host, [WORKBENCH-VSIX]). Presentation matching never supplies
// a replacement action; the pressed node survives until its pointer releases.

export interface ActionState {
  pressed: HTMLButtonElement | null;
  pressedPointerId: number | null;
  cancelled: HTMLButtonElement | null;
  pendingPatch: HTMLButtonElement | null;
}

interface ActionHooks {
  readonly state: ActionState;
  /** Whether an element is a control that can be pressed and held. */
  held(target: EventTarget | null): HTMLButtonElement | null;
  /** Flush pending field drafts before the press is recorded. */
  flushFields(): void;
  /** Busy state may change while a press is held. */
  updateBusy(): void;
}

function copyAttributes(current: Element, next: Element): void {
  for (const attribute of Array.from(current.attributes)) {
    if (!next.hasAttribute(attribute.name)) current.removeAttribute(attribute.name);
  }
  for (const attribute of Array.from(next.attributes)) current.setAttribute(attribute.name, attribute.value);
}

/** The pressed node's path below `root`, or null when it is no longer inside it. */
function heldPath(root: Element, pressed: Element): number[] | null {
  const path: number[] = [];
  let node: Node = pressed;
  while (node !== root) {
    const parent = node.parentNode;
    if (parent === null) return null;
    path.unshift(Array.prototype.indexOf.call(parent.childNodes, node));
    node = parent;
  }
  return path;
}

function compatiblePath(root: Node, next: Node, path: readonly number[], pressed: HTMLButtonElement): boolean {
  let current: Node | null = root;
  let candidate: Node | null = next;
  for (const index of path) {
    if (current === null || candidate === null || current.childNodes.length !== candidate.childNodes.length) return false;
    current = current.childNodes[index] ?? null;
    candidate = candidate.childNodes[index] ?? null;
    if (current === null || candidate === null || current.nodeType !== candidate.nodeType || current.nodeName !== candidate.nodeName) return false;
  }
  return candidate instanceof HTMLButtonElement && !candidate.disabled && candidate.getAttribute("data-focus") === pressed.getAttribute("data-focus");
}

function patchBranch(current: Node, next: Node, state: ActionState): void {
  if (current === state.pressed) {
    state.pendingPatch = next instanceof HTMLButtonElement ? next : null;
    return;
  }
  if (current instanceof Element && next instanceof Element) copyAttributes(current, next);
  const children = Array.from(current.childNodes);
  const replacements = Array.from(next.childNodes);
  children.forEach((child, index) => {
    const replacement = replacements[index];
    if (replacement === undefined) return;
    if (state.pressed !== null && child.contains(state.pressed)) patchBranch(child, replacement, state);
    else child.replaceWith(replacement);
  });
}

/** Try to keep the pressed button's exact node across a re-render; false when it cannot. */
export function patchHeldAction(root: HTMLElement, html: string, state: ActionState): boolean {
  const pressed = state.pressed;
  if (pressed === null) return false;
  const template = document.createElement("template");
  template.innerHTML = html;
  const path = heldPath(root, pressed);
  if (path === null || !compatiblePath(root, template.content, path, pressed)) return false;
  patchBranch(root, template.content, state);
  return true;
}

function release(state: ActionState, hooks: ActionHooks): void {
  const button = state.pressed ?? state.cancelled;
  const next = state.pendingPatch;
  state.pressed = null;
  state.pressedPointerId = null;
  state.cancelled = null;
  state.pendingPatch = null;
  if (button !== null && next !== null && button.isConnected) {
    copyAttributes(button, next);
    button.replaceChildren(...Array.from(next.childNodes));
  }
  hooks.updateBusy();
}

/** Install the press-preserving pointer listeners. */
export function installActions(hooks: ActionHooks): void {
  const { state } = hooks;
  let sequence = 0;
  const cancel = (): void => {
    const button = state.pressed ?? state.cancelled;
    release(state, hooks);
    state.cancelled = button;
  };
  document.addEventListener("pointerdown", (event) => {
    const button = hooks.held(event.target);
    if (!event.isPrimary || event.button !== 0 || button === null || button.disabled) return;
    if (state.pressedPointerId !== null && event.pointerId !== state.pressedPointerId) return;
    release(state, hooks);
    state.pressed = button;
    state.pressedPointerId = event.pointerId;
    hooks.flushFields();
  });
  document.addEventListener("pointerup", (event) => {
    if (state.pressedPointerId !== event.pointerId) return;
    if (state.pressed !== null && !state.pressed.contains(document.elementFromPoint(event.clientX, event.clientY))) {
      state.cancelled = state.pressed;
    }
    // The release waits for this gesture's click; a later pointer-up supersedes it.
    sequence += 1;
    const at = sequence;
    setTimeout(() => {
      if (at === sequence) release(state, hooks);
    }, 0);
  });
  document.addEventListener("pointercancel", (event) => {
    if (event.pointerId === state.pressedPointerId) cancel();
  });
  document.addEventListener("pointermove", (event) => {
    if (event.pointerId === state.pressedPointerId && event.buttons === 0) release(state, hooks);
  });
  window.addEventListener("blur", cancel);
}

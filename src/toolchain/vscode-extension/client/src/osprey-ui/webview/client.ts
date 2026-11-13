// The webview's interaction layer, restored from the JSON-protocol host and retargeted at the
// typed event boundary ([NATIVE-SHM-EVENT]): generic focus and scroll preservation, pending edit
// flushing, event delegation, splitter dragging and context menus. A control event is the
// control's ordinal and its value as words; there are no region names or revisions any more.

import { controlEvent, type HostEvent, wordsOf } from "./events.js";
import { installActions, patchHeldAction, type ActionState } from "./action.js";

export interface FramePosted {
  /** A control event: the ordinal and its value as words. */
  control(ordinal: number, words: string): void;
  /** Any typed host event, as the scene and board inputs build them. */
  event(event: HostEvent): void;
  /** The host opens only HTTPS links it approves. */
  external(url: string): void;
}

interface FieldEdit {
  readonly ordinal: number;
  readonly words: string;
  sent: boolean;
}

/** Everything the client keeps between frames. */
export interface ClientState {
  readonly action: ActionState;
  readonly edits: Map<string, FieldEdit>;
  readonly timers: Map<string, number>;
  savedFocus: { id: string; start: number | null; end: number | null; signature: string | null } | null;
  hostDiagnostics: readonly string[];
  pendingLoss: string | undefined;
  runtimeBusy: boolean;
  runtimeTracked: boolean;
}

export interface Client extends ClientState {
  readonly post: FramePosted;
  /** Drawn after each render: scene canvases, boards and editors. */
  rendered?(): void;
}

function closest(target: EventTarget | null, selector: string): Element | null {
  return target instanceof Element ? target.closest(selector) : null;
}

/** The control number an element carries, when it delivers one. */
export function ordinalOf(element: Element): number | null {
  const ordinal = Number(element.getAttribute("data-event-ordinal") ?? element.getAttribute("data-ordinal"));
  return Number.isSafeInteger(ordinal) && ordinal >= 0 ? ordinal : null;
}

/** The words a drawn control's value becomes; a switch carries its next state. */
function wordsOfElement(element: Element, override?: unknown): string {
  if (override !== undefined) return wordsOf(override);
  const kind = element.getAttribute("data-value-kind");
  const raw = element.getAttribute("data-value");
  if (kind === null) return element.getAttribute("data-words") ?? "";
  return wordsOf(kind === "number" ? Number(raw) : kind === "bool" ? raw === "true" : kind === "string" ? raw : null);
}

export function fieldSignature(element: Element): string {
  const label = element.closest("label");
  const labelSpan = label?.querySelector("span") ?? null;
  return JSON.stringify([
    element.getAttribute("data-focus"),
    element.tagName,
    element.getAttribute("type"),
    element.getAttribute("data-value-kind"),
    element.getAttribute("data-commit"),
    element.getAttribute("placeholder") || "",
    labelSpan !== null ? labelSpan.textContent : "",
  ]);
}

type Field = HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;

function fieldsBySignature(signature: string): Field[] {
  return Array.from(document.querySelectorAll<Field>("input, textarea, select"))
    .filter((element) => fieldSignature(element) === signature);
}

function notice(client: Client): void {
  const combined = [...new Set([...client.hostDiagnostics, ...(client.pendingLoss === undefined ? [] : [client.pendingLoss])])];
  const diagnostics = document.getElementById("osprey-diagnostics");
  if (diagnostics !== null) {
    diagnostics.textContent = combined.join("\n");
    diagnostics.hidden = combined.length === 0;
  }
  const status = document.getElementById("osprey-runtime-status");
  if (status !== null) status.hidden = !client.runtimeBusy;
  const root = document.getElementById("osprey-root");
  if (root !== null) root.setAttribute("aria-busy", String(client.runtimeBusy));
}

/** Pause every control while the runtime is busy, keeping the one held by its pointer. */
const pausedControls = new Map<HTMLButtonElement | HTMLSelectElement, boolean>();

function updateBusy(client: Client): void {
  const root = document.getElementById("osprey-root");
  notice(client);
  if (root === null) return;
  for (const control of Array.from(root.querySelectorAll("button, select"))) {
    if (!(control instanceof HTMLButtonElement) && !(control instanceof HTMLSelectElement)) continue;
    if (control === client.action.pressed) continue;
    if (client.runtimeBusy) {
      if (!pausedControls.has(control)) pausedControls.set(control, control.disabled);
      control.disabled = true;
    } else if (pausedControls.has(control)) {
      control.disabled = pausedControls.get(control) === true;
      pausedControls.delete(control);
    }
  }
  if (!client.runtimeBusy) pausedControls.clear();
}

function postControl(client: Client, element: Element, override?: unknown): void {
  const ordinal = ordinalOf(element);
  if (ordinal === null) return;
  const words = wordsOfElement(element, override);
  flushFields(client);
  if (client.runtimeTracked) {
    client.runtimeBusy = true;
    updateBusy(client);
  }
  client.post.event(controlEvent(ordinal, words));
}

function sendField(client: Client, signature: string): void {
  const edit = client.edits.get(signature);
  if (edit === undefined || edit.sent) return;
  edit.sent = true;
  client.post.event(controlEvent(edit.ordinal, edit.words));
}

/** Commit every draft before another control's action. */
export function flushFields(client: Client): void {
  for (const timer of client.timers.values()) window.clearTimeout(timer);
  client.timers.clear();
  for (const signature of [...client.edits.keys()]) sendField(client, signature);
}

function armFieldTimer(client: Client, signature: string): void {
  const previous = client.timers.get(signature);
  if (previous !== undefined) window.clearTimeout(previous);
  client.timers.set(signature, window.setTimeout(() => {
    client.timers.delete(signature);
    sendField(client, signature);
  }, 160));
}

function onInput(client: Client, event: Event): void {
  const element = event.target;
  if (!(element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement)) return;
  const commit = element.getAttribute("data-commit") ?? "";
  if (commit !== "edit" && commit !== "change") return;
  client.pendingLoss = undefined;
  const signature = fieldSignature(element);
  const ordinal = ordinalOf(element);
  if (ordinal === null) return;
  client.edits.set(signature, { ordinal, words: element.value, sent: false });
  armFieldTimer(client, signature);
}

function onChange(client: Client, event: Event): void {
  const element = event.target;
  if (!(element instanceof HTMLInputElement || element instanceof HTMLSelectElement || element instanceof HTMLTextAreaElement)) return;
  const commit = element.getAttribute("data-commit") ?? "";
  if (!(element instanceof HTMLSelectElement) && commit !== "submit" && commit !== "change") return;
  postControl(client, element, element.value);
}

function closeMenus(): void {
  for (const menu of Array.from(document.querySelectorAll(".osp-menu"))) {
    if (!(menu instanceof HTMLElement)) continue;
    menu.hidden = true;
    menu.style.left = "";
    menu.style.top = "";
  }
}

function openMenu(host: Element, x: number, y: number): void {
  const menu = host.querySelector(".osp-menu");
  if (!(menu instanceof HTMLElement)) return;
  closeMenus();
  menu.hidden = false;
  menu.style.left = `${Math.max(8, Math.min(x, window.innerWidth - 260))}px`;
  menu.style.top = `${Math.max(8, Math.min(y, window.innerHeight - 240))}px`;
}

let dialogReturnFocus: string | null = null;

function focusDialog(): void {
  const dialog = document.querySelector("[data-native-dialog]");
  if (!document.hasFocus()) {
    if (dialog === null) dialogReturnFocus = null;
    return;
  }
  if (dialog instanceof HTMLElement) {
    if (dialogReturnFocus === null) dialogReturnFocus = document.activeElement?.getAttribute("data-focus") ?? "";
    if (!dialog.contains(document.activeElement)) {
      (dialog.querySelector<HTMLElement>("button, input, textarea, select, a[href]") ?? dialog).focus();
    }
  } else if (dialogReturnFocus !== null) {
    const focus = Array.from(document.querySelectorAll<HTMLElement>("[data-focus]"))
      .find((node) => node.getAttribute("data-focus") === dialogReturnFocus);
    focus?.focus();
    dialogReturnFocus = null;
  }
}

function saveFocus(client: Client): void {
  client.savedFocus = null;
  if (!document.hasFocus()) return;
  const active = document.activeElement;
  if (active === null || active.getAttribute === undefined) return;
  const id = active.getAttribute("data-focus");
  if (id === null || id === "") return;
  client.savedFocus = {
    id,
    start: typeof (active as HTMLInputElement).selectionStart === "number" ? (active as HTMLInputElement).selectionStart : null,
    end: typeof (active as HTMLInputElement).selectionEnd === "number" ? (active as HTMLInputElement).selectionEnd : null,
    signature: active.matches("input, textarea") ? fieldSignature(active) : null,
  };
}

function restoreFocus(client: Client): void {
  const saved = client.savedFocus;
  client.savedFocus = null;
  if (saved === null) return;
  const element = Array.from(document.querySelectorAll("[data-focus]"))
    .find((candidate) => candidate.getAttribute("data-focus") === saved.id);
  if (!(element instanceof HTMLElement)) return;
  if (saved.signature !== null && fieldSignature(element) !== saved.signature) return;
  element.focus();
  const field = element as HTMLInputElement;
  if (saved.start !== null && typeof field.setSelectionRange === "function") {
    try {
      field.setSelectionRange(saved.start, saved.end === null ? saved.start : saved.end);
    } catch {
      // Not a text control.
    }
  }
}

function elementPath(element: Element): string {
  const path: number[] = [];
  let node: Element | null = element;
  while (node !== null && node.parentElement !== null && node !== document.body && node !== document.documentElement) {
    path.unshift(Array.prototype.indexOf.call(node.parentElement.children, node));
    node = node.parentElement;
  }
  return path.join(".");
}

function elementAtPath(path: string): Element | null {
  if (path === "") return document.scrollingElement;
  let node: Element = document.body;
  for (const step of path.split(".")) {
    const next = node.children[Number(step)];
    if (next === undefined) return null;
    node = next;
  }
  return node;
}

function captureScroll(): { path: string; top: number; left: number }[] {
  const found = [document.scrollingElement, ...Array.from(document.querySelectorAll("*"))];
  return found.filter((element): element is Element => element !== null && (element.scrollTop !== 0 || element.scrollLeft !== 0))
    .map((element) => ({ path: elementPath(element), top: element.scrollTop, left: element.scrollLeft }));
}

function restoreScroll(saved: readonly { path: string; top: number; left: number }[]): void {
  for (const entry of saved) {
    const element = elementAtPath(entry.path);
    if (element === null) continue;
    element.scrollTop = entry.top;
    element.scrollLeft = entry.left;
  }
}

/** Re-apply a draft over an unrelated frame's presentation, as the JSON-era client did while its
 * edit was in flight; once a frame shows the draft's own value it has settled. */
function rebaseEdits(client: Client): void {
  for (const [signature, edit] of [...client.edits.entries()]) {
    const matches = fieldsBySignature(signature);
    if (matches.length === 1 && matches[0] instanceof HTMLInputElement) {
      if (matches[0].value === edit.words) {
        client.edits.delete(signature);
        continue;
      }
      if (edit.sent) matches[0].value = edit.words;
    }
    if (matches.length === 0 && edit.sent) client.edits.delete(signature);
  }
}

/** Apply one told frame's markup, preserving focus, drafts and scroll exactly as the JSON-era
 * client did; the pressed control keeps its own node when it can. */
export function applyRender(client: Client, html: string): void {
  flushFields(client);
  saveFocus(client);
  const scroll = captureScroll();
  const root = document.getElementById("osprey-root");
  if (root !== null) {
    const held = client.action.pressed !== null && patchHeldAction(root, html, client.action);
    if (!held) root.innerHTML = html;
  }
  rebaseEdits(client);
  restoreFocus(client);
  focusDialog();
  client.rendered?.();
  restoreScroll(scroll);
  updateBusy(client);
}

function onClick(client: Client, event: MouseEvent): void {
  if (event instanceof PointerEvent && event.detail !== 0 && client.action.pressedPointerId !== null
    && event.pointerId !== client.action.pressedPointerId) return;
  const external = closest(event.target, "a[data-external]");
  if (external !== null) {
    event.preventDefault();
    client.post.external(external.getAttribute("href") ?? "");
    return;
  }
  closeMenus();
  const element = closest(event.target, "[data-event-ordinal], [data-ordinal]");
  if (element === null) return;
  if (element === client.action.cancelled && event.detail !== 0) return;
  if (element.hasAttribute("data-splitter") || element.hasAttribute("data-native-dialog")) return;
  if (element.matches("input, textarea, select, canvas")) return;
  postControl(client, element);
}

function onKeydown(client: Client, event: KeyboardEvent): void {
  if (event.key === "Escape") {
    const dialog = document.querySelector("[data-native-dialog]");
    if (dialog !== null) {
      event.preventDefault();
      postControl(client, dialog);
    }
    closeMenus();
    return;
  }
  const modal = document.querySelector("[data-native-dialog]");
  if (modal instanceof HTMLElement && event.key === "Tab") {
    const fields = Array.from(modal.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), a[href]"));
    const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const index = active === null ? -1 : fields.indexOf(active);
    const next = event.shiftKey ? (index <= 0 ? fields.length - 1 : index - 1) : (index + 1) % fields.length;
    event.preventDefault();
    (fields[next] ?? modal).focus();
    return;
  }
  const element = event.target;
  if (!(element instanceof Element)) return;
  if (element.hasAttribute("data-splitter") && /^(ArrowLeft|ArrowRight)$/.test(event.key)) {
    const current = Number(element.getAttribute("data-value"));
    const min = Number(element.getAttribute("data-min"));
    const max = Number(element.getAttribute("data-max"));
    const side = element.getAttribute("data-side");
    const step = event.key === "ArrowRight" ? 20 : -20;
    const next = Math.max(min, Math.min(max, current + (side === "leading" ? step : -step)));
    element.setAttribute("data-value", String(next));
    const aside = element.closest(".osp-aside");
    if (aside instanceof HTMLElement) aside.style.setProperty("--osp-aside-width", `${next}px`);
    postControl(client, element, next);
    event.preventDefault();
    return;
  }
  const commit = element.getAttribute("data-commit") ?? "";
  if (event.key === "Enter" && element.matches("input, textarea") && (commit === "submit" || commit === "enter") && element.tagName !== "TEXTAREA") {
    postControl(client, element, (element as HTMLInputElement).value);
    event.preventDefault();
  }
}

interface DragState { readonly splitter: HTMLElement; readonly startX: number; readonly startWidth: number; width: number; readonly min: number; readonly max: number; readonly side: string }

let drag: DragState | null = null;

/** Install the interaction listeners on the document. */
export function installClient(client: Client): void {
  installActions({
    state: client.action,
    held: (target) => {
      const button = closest(target, "button[data-event-ordinal], button[data-ordinal]");
      return button instanceof HTMLButtonElement ? button : null;
    },
    flushFields: () => flushFields(client),
    updateBusy: () => updateBusy(client),
  });
  document.addEventListener("click", (event) => onClick(client, event));
  document.addEventListener("input", (event) => onInput(client, event));
  document.addEventListener("change", (event) => onChange(client, event));
  document.addEventListener("keydown", (event) => onKeydown(client, event));
  document.addEventListener("contextmenu", (event) => {
    const host = closest(event.target, "[data-context-host]");
    if (host === null) return;
    event.preventDefault();
    openMenu(host, event.clientX, event.clientY);
  });
  document.addEventListener("keydown", (event) => {
    if (event.key !== "ContextMenu" && !(event.shiftKey && event.key === "F10")) return;
    const host = document.activeElement?.closest("[data-context-host]") ?? null;
    if (host === null) return;
    event.preventDefault();
    const box = host.getBoundingClientRect();
    openMenu(host, box.left + 24, box.top + 24);
  });
  document.addEventListener("pointerdown", (event) => {
    const splitter = closest(event.target, "[data-splitter]");
    if (!(splitter instanceof HTMLElement)) return;
    event.preventDefault();
    drag = {
      splitter, startX: event.clientX, startWidth: Number(splitter.getAttribute("data-value")),
      width: Number(splitter.getAttribute("data-value")), min: Number(splitter.getAttribute("data-min")),
      max: Number(splitter.getAttribute("data-max")), side: splitter.getAttribute("data-side") ?? "trailing",
    };
    splitter.classList.add("osp-dragging");
    splitter.setPointerCapture(event.pointerId);
  });
  document.addEventListener("pointermove", (event) => {
    if (drag === null) return;
    const delta = event.clientX - drag.startX;
    drag.width = Math.max(drag.min, Math.min(drag.max, drag.startWidth + (drag.side === "leading" ? delta : -delta)));
    const aside = drag.splitter.closest(".osp-aside");
    if (aside instanceof HTMLElement) aside.style.setProperty("--osp-aside-width", `${drag.width}px`);
  });
  document.addEventListener("pointerup", () => {
    if (drag === null) return;
    const finished = drag;
    drag = null;
    finished.splitter.classList.remove("osp-dragging");
    if (Number.isFinite(finished.width)) postControl(client, finished.splitter, finished.width);
  });
}

/** The busy and fatal messages the host sends beside its frames. */
export function clientNotice(client: Client, kind: "busy" | "fatal", value?: boolean | string): void {
  if (kind === "busy") {
    client.runtimeTracked = true;
    client.runtimeBusy = value === true;
    updateBusy(client);
    return;
  }
  flushFields(client);
  client.edits.clear();
  client.runtimeBusy = false;
  updateBusy(client);
  client.pendingLoss = typeof value === "string" ? value : undefined;
  notice(client);
}

/** Adopt the diagnostics a frame carries. */
export function clientDiagnostics(client: Client, diagnostics: readonly string[]): void {
  client.hostDiagnostics = diagnostics;
  notice(client);
}

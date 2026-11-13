// Mechanical marked-editor behaviour, restored from the JSON-protocol host: the mirror follows
// the textarea, and the authored caret and focus requests honour their epoch. The cursor reports
// as a typed control event on the editor's place ordinal: the byte offset as decimal words
// ([NATIVE-SHM-EVENT]).

import { type FramePosted } from "./client.js";
import { fieldSignature, type Client } from "./client.js";

interface Position {
  readonly value: string;
  readonly cursor: number;
  readonly focusEpoch: number;
}

/** What the caret reporter needs: where events go and which drafts are in flight. */
export interface Caret {
  readonly post: FramePosted;
  readonly edits: Client["edits"];
}

let positions = new Map<string, Position>();
const epochs = new Map<string, number>();

function mirrorOf(element: HTMLTextAreaElement): HTMLElement {
  const mirror = element.parentElement?.querySelector(".osp-editor-mirror");
  if (!(mirror instanceof HTMLElement)) throw new Error("The marked editor has no presentation surface");
  return mirror;
}

function mirrorEdit(element: HTMLTextAreaElement): void {
  const mirror = mirrorOf(element);
  mirror.textContent = `${element.value}${element.value.endsWith("\n") ? "\u200b" : ""}`;
  mirror.scrollTop = element.scrollTop;
  mirror.scrollLeft = element.scrollLeft;
}

function caretBytes(element: HTMLTextAreaElement): number {
  return new TextEncoder().encode(element.value.slice(0, element.selectionStart)).length;
}

function reportCaret(caret: Caret): void {
  const element = document.activeElement;
  if (!(element instanceof HTMLTextAreaElement) || !element.matches("textarea[data-native-editor]")) return;
  if (caret.edits.has(fieldSignature(element))) return;
  const ordinal = Number(element.dataset.placeOrdinal ?? "-1");
  if (!Number.isSafeInteger(ordinal) || ordinal < 0) throw new Error("The marked editor has no issued provenance");
  const offset = caretBytes(element);
  if (offset === Number(element.dataset.cursorByte)) return;
  element.dataset.cursorByte = String(offset);
  caret.post.control(ordinal, String(offset));
}

function caretBounds(element: HTMLTextAreaElement, prefix: string): DOMRect {
  const mirror = mirrorOf(element);
  const saved = mirror.style.cssText;
  const style = getComputedStyle(element);
  const lines = prefix.split("\n");
  const line = mirror.children[lines.length - 1];
  if (line === undefined || line.firstChild === null) throw new Error("The authored editor caret has no source line");
  mirror.style.setProperty("display", "block", "important");
  mirror.style.setProperty("visibility", "hidden", "important");
  for (const key of ["font", "padding", "letter-spacing", "white-space", "overflow-wrap"]) {
    mirror.style.setProperty(key, style.getPropertyValue(key), "important");
  }
  try {
    const range = document.createRange();
    range.setStart(line.firstChild, lines[lines.length - 1].length);
    range.collapse(true);
    return range.getBoundingClientRect();
  } finally {
    mirror.style.cssText = saved;
  }
}

function revealCaret(element: HTMLTextAreaElement, prefix: string): void {
  const box = caretBounds(element, prefix);
  const editor = element.getBoundingClientRect();
  if (box.top < editor.top) element.scrollTop += box.top - editor.top;
  if (box.bottom > editor.bottom) element.scrollTop += box.bottom - editor.bottom + 12;
  mirrorOf(element).scrollTop = element.scrollTop;
}

/** Place the authored caret; an epoch above the high-water mark is a request for focus. */
function authoredCaret(caret: Caret, element: HTMLTextAreaElement, previous: Position | undefined): void {
  const signature = fieldSignature(element);
  const current: Position = { value: element.value, cursor: Number(element.dataset.cursorByte),
    focusEpoch: Number(element.dataset.focusEpoch || 0) };
  positions.set(signature, current);
  const highest = epochs.get(signature);
  const requested = highest !== undefined && current.focusEpoch > highest;
  epochs.set(signature, highest === undefined ? current.focusEpoch : Math.max(highest, current.focusEpoch));
  if (caret.edits.has(signature) || (document.activeElement === element && !requested)) return;
  const bytes = new TextEncoder().encode(current.value);
  if (current.cursor > bytes.length) throw new Error("The authored editor caret exceeds its source");
  const prefix = new TextDecoder("utf-8", { fatal: true }).decode(bytes.slice(0, current.cursor));
  const dialog = document.querySelector("[data-native-dialog]");
  if (requested && document.hasFocus() && (dialog === null || dialog.contains(element))) element.focus();
  element.setSelectionRange(prefix.length, prefix.length);
  if (previous === undefined || previous.value !== current.value || previous.cursor !== current.cursor) revealCaret(element, prefix);
}

/** Refresh every marked editor after a render: mirrors, authored carets and pending drafts. */
export function refreshEditors(caret: Caret): void {
  const previous = positions;
  positions = new Map();
  for (const element of Array.from(document.querySelectorAll("textarea[data-native-editor]"))) {
    if (!(element instanceof HTMLTextAreaElement)) continue;
    const signature = fieldSignature(element);
    if (caret.edits.has(signature)) mirrorEdit(element);
    authoredCaret(caret, element, previous.get(signature));
    element.addEventListener("scroll", () => {
      const mirror = mirrorOf(element);
      mirror.scrollTop = element.scrollTop;
      mirror.scrollLeft = element.scrollLeft;
    });
  }
  for (const signature of [...epochs.keys()]) if (!positions.has(signature)) epochs.delete(signature);
  queueMicrotask(() => reportCaret(caret));
}

/** Install the mirror-following input and selection listeners. */
export function installEditors(caret: Caret): void {
  document.addEventListener("input", (event) => {
    const element = event.target;
    if (element instanceof HTMLTextAreaElement && element.matches("textarea[data-native-editor]")) mirrorEdit(element);
  });
  document.addEventListener("selectionchange", () => reportCaret(caret));
}

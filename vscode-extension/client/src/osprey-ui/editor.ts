// Implements [UI-COMPONENTS-EDITOR]: draw application-issued line marks and
// preserve two distinct bindings for text and cursor movement.
import { escapeHtml } from "./html";
import { record } from "./protocol";

interface Mark {
  readonly line: number;
  readonly underlined: boolean;
  readonly tone: string;
  readonly badge: string;
  readonly note: string;
}

const TONES = new Set(["primary", "secondary", "accent", "good", "caution", "danger"]);

function text(node: Record<string, unknown>, key: string): string {
  if (typeof node[key] !== "string") throw new Error(`Editor ${key} must be text`);
  return node[key];
}

function offset(node: Record<string, unknown>, key: string): number {
  const value = node[key];
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) throw new Error(`Editor ${key} must be a non-negative integer`);
  return value;
}

function mark(value: unknown, lines: number): Mark {
  if (!record(value) || typeof value.line !== "number" || !Number.isSafeInteger(value.line) || value.line < 1 || value.line > lines) {
    throw new Error("Editor mark must name an existing line");
  }
  if (typeof value.underlined !== "boolean") throw new Error("Editor mark underline must be a flag");
  const tone = text(value, "tone");
  return { line: value.line, underlined: value.underlined, tone: TONES.has(tone) ? tone : "primary", badge: text(value, "badge"), note: text(value, "note") };
}

function marks(node: Record<string, unknown>, lines: number): Mark[] {
  if (!Array.isArray(node.marks) || node.marks.length > 10000) throw new Error("Editor marks must be a bounded list");
  return node.marks.map((value) => mark(value, lines));
}

function line(text: string, at: number, marks: readonly Mark[]): string {
  const style = marks.find((mark) => mark.line === at && mark.underlined);
  const classes = style ? ` osp-editor-underlined osp-tone-${style.tone}` : "";
  return `<span class="osp-editor-line${classes}" data-mark-line="${at}">${escapeHtml(text) || "&#8203;"}</span>`;
}

function feedback(marks: readonly Mark[], id: string): string {
  const content = marks.filter((mark) => mark.badge !== "" || mark.note !== "").map((mark) =>
    `<li class="osp-tone-${mark.tone}" data-mark-line="${mark.line}"><span>Line ${mark.line}</span> <strong>${escapeHtml(mark.badge)}</strong> ${escapeHtml(mark.note)}</li>`).join("");
  return `<ul class="osp-editor-feedback" id="${escapeHtml(id)}">${content}</ul>`;
}

/** Render an editable document with Osprey's existing line annotations; no host parsing or verdicts. */
export function renderEditor(node: Record<string, unknown>, control: string, id: string): string {
  const value = text(node, "value");
  const lines = value.split("\n");
  const annotations = marks(node, lines.length);
  const mirror = lines.map((text, at) => line(text, at + 1, annotations)).join("");
  const inline = node.inline === true;
  const textarea = `<textarea aria-label="${escapeHtml(text(node, "label"))}" placeholder="${inline ? escapeHtml(text(node, "label")) : ""}" spellcheck="false" data-native-editor="true" data-commit="edit" data-cursor-byte="${offset(node, "cursor")}" data-place-ordinal="${offset(node, "placeEvent")}" aria-describedby="${escapeHtml(id)}"${control}>${escapeHtml(value)}</textarea>`;
  return `<div class="osp-editor${inline ? " osp-editor-inline" : ""}"><label><span class="osp-editor-label">${escapeHtml(text(node, "label"))}</span><div class="osp-editor-surface"><pre class="osp-editor-mirror" aria-hidden="true">${mirror}</pre>${textarea}</div></label>${feedback(annotations, id)}</div>`;
}

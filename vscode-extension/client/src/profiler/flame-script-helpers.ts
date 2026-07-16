// Pure helpers shared between the extension host and the flame-graph webview
// ([PROF-VSCODE-FLAME]). FLAME_SCRIPT embeds these functions' COMPILED SOURCE
// via Function.prototype.toString, so the exact code the webview executes is
// type-checked and unit-tested here. Rules for anything in this file: no
// imports, no captured module state, no TS-only syntax that erases to
// different behavior — each function must be a self-contained declaration
// that serializes cleanly (target es2020; the webview is modern Chromium).

/**
 * Fit `name` into `avail` pixels by carving characters out of the middle and
 * inserting an ellipsis, using the caller's text `measure` (canvas
 * measureText in the webview, an injected fake in tests). Returns the widest
 * candidate that fits, or a lone ellipsis when nothing does.
 */
export function fitLabelText(
  name: string,
  avail: number,
  measure: (text: string) => number,
): string {
  if (measure(name) <= avail) {
    return name;
  }
  let out = "…";
  let keep = name.length;
  while (keep > 2) {
    keep -= Math.max(1, Math.ceil(keep * 0.15));
    const half = Math.floor(keep / 2);
    const candidate = name.slice(0, half) + "…" + name.slice(name.length - (keep - half));
    if (measure(candidate) <= avail) {
      out = candidate;
      break;
    }
  }
  return out;
}

/**
 * Clamp a flame-graph view window to config space: span forced into
 * [minSpan, 1], then the window slid back inside [0, 1] preserving the span.
 */
export function clampViewRange(
  x0: number,
  x1: number,
  minSpan: number,
): { x0: number; x1: number } {
  const span = Math.min(Math.max(x1 - x0, minSpan), 1);
  const left = Math.min(Math.max(x0, 0), 1 - span);
  return { x0: left, x1: left + span };
}

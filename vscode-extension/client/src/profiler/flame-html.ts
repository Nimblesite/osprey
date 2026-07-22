// Pure builder for the flame-graph webview document ([PROF-VSCODE-FLAME]).
// Produces the COMPLETE self-contained HTML string: strict CSP with a nonce
// (no external resources), theme-aware chrome via --vscode-* variables, the
// header strip + fiber chips, the Left Heavy / Time Order toolbar, the canvas,
// the tooltip, the hot-functions table, the JSON data blob, and the inline
// renderer script. No vscode imports — unit tested as a string.

import type { FlameModel } from "./flame-model";
import { FLAME_SCRIPT } from "./flame-script";
import { formatSummaryHeader, onCpuPct, type ProfileSummary } from "./summary";

export interface FlameHtmlInput {
  nonce: string;
  title: string;
  model: FlameModel;
  summary: ProfileSummary;
}

const NONCE_ALPHABET =
  "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
const NONCE_LENGTH = 32;

/** A fresh random nonce for the webview CSP. */
export function makeNonce(): string {
  let nonce = "";
  for (let i = 0; i < NONCE_LENGTH; i += 1) {
    nonce += NONCE_ALPHABET.charAt(
      Math.floor(Math.random() * NONCE_ALPHABET.length),
    );
  }
  return nonce;
}

/** Escape a string for safe interpolation into HTML text/attributes. */
export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

/** The final path segment (both separators), for compact locations. */
export function pathBasename(filePath: string): string {
  const parts = filePath.split(/[\\/]/);
  return parts[parts.length - 1] || filePath;
}

function chipsHtml(summary: ProfileSummary): string {
  // Only fibers that produced samples get chips: the speedscope export (and
  // therefore model.fibers) contains exactly those fibers, in the same row
  // order, so the filtered index is what setFiber() needs. Including
  // zero-sample rows would shift every later chip onto the wrong fiber.
  return summary.fibers
    .filter((fiber) => fiber.samples > 0)
    .map(
      (fiber, i) =>
        `<button class="chip" data-fiber="${i}">${escapeHtml(fiber.label)} · ` +
        `${onCpuPct(fiber)}% on-CPU</button>`,
    )
    .join("");
}

function fiberOptionsHtml(model: FlameModel): string {
  return model.fibers
    .map(
      (fiber, i) => `<option value="${i}">${escapeHtml(fiber.name)}</option>`,
    )
    .join("");
}

function hotFunctionRow(fn: ProfileSummary["hotFunctions"][number]): string {
  const location = fn.file ? `${pathBasename(fn.file)}:${fn.line}` : "—";
  const attrs = fn.file
    ? ` data-file="${escapeHtml(fn.file)}" data-line="${fn.line}"`
    : "";
  return (
    `<tr${attrs}><td class="num">${fn.selfPct.toFixed(1)}</td>` +
    `<td class="num">${fn.totalPct.toFixed(1)}</td>` +
    `<td>${escapeHtml(fn.name)}</td><td class="loc">${escapeHtml(location)}</td></tr>`
  );
}

function hotFunctionsTable(summary: ProfileSummary): string {
  const rows = summary.hotFunctions.map(hotFunctionRow).join("");
  return (
    `<h2 class="section">Hot functions</h2>` +
    `<table id="hot-table"><thead><tr><th class="num">SELF%</th><th class="num">TOTAL%</th>` +
    `<th>FUNCTION</th><th>LOCATION</th></tr></thead><tbody>${rows}</tbody></table>`
  );
}

// The webview stylesheet: VS Code theme tokens only — no external fonts.
const FLAME_CSS = `
:root { color-scheme: light dark; }
* { box-sizing: border-box; }
body {
  margin: 0; padding: 10px 14px; font-size: 12px;
  font-family: var(--vscode-font-family, sans-serif);
  color: var(--vscode-foreground); background: var(--vscode-editor-background);
}
.header { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-bottom: 8px; }
.stats { color: var(--vscode-descriptionForeground); }
.chip {
  border: 1px solid var(--vscode-panel-border, #5558); border-radius: 10px;
  background: var(--vscode-editorWidget-background); color: var(--vscode-foreground);
  padding: 1px 9px; cursor: pointer; font-size: 11px; font-family: inherit;
}
.chip.active { border-color: var(--vscode-focusBorder, #3794ff); }
.toolbar { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; margin-bottom: 8px; }
.toggle {
  border: 1px solid transparent; border-radius: 3px; padding: 2px 10px; cursor: pointer;
  background: var(--vscode-button-secondaryBackground, #3a3d41);
  color: var(--vscode-button-secondaryForeground, #ccc); font-family: inherit; font-size: 12px;
}
.toggle.active {
  background: var(--vscode-button-background, #0e639c);
  color: var(--vscode-button-foreground, #fff);
}
#fiber-select {
  background: var(--vscode-dropdown-background); color: var(--vscode-dropdown-foreground);
  border: 1px solid var(--vscode-dropdown-border, transparent); padding: 2px 4px; font-family: inherit;
}
#search {
  flex: 0 1 240px; padding: 2px 6px; font-family: inherit;
  background: var(--vscode-input-background); color: var(--vscode-input-foreground);
  border: 1px solid var(--vscode-input-border, transparent);
}
#flame-wrap { position: relative; border: 1px solid var(--vscode-panel-border, #5554); }
#flame { display: block; }
#tooltip {
  position: fixed; display: none; z-index: 10; pointer-events: none; max-width: 480px;
  padding: 6px 9px; font-size: 11px; border-radius: 3px;
  background: var(--vscode-editorHoverWidget-background, #252526);
  color: var(--vscode-editorHoverWidget-foreground, #ccc);
  border: 1px solid var(--vscode-editorHoverWidget-border, #454545);
}
.tt-name { font-weight: 600; }
.tt-loc, .tt-nums { color: var(--vscode-descriptionForeground); }
h2.section { font-size: 12px; text-transform: uppercase; letter-spacing: 0.06em; margin: 16px 0 6px; }
table { border-collapse: collapse; width: 100%; }
th, td { padding: 2px 10px 2px 0; text-align: left; }
th { color: var(--vscode-descriptionForeground); font-weight: 600; border-bottom: 1px solid var(--vscode-panel-border, #5554); }
td.num, th.num { text-align: right; font-variant-numeric: tabular-nums; width: 64px; }
td.loc { color: var(--vscode-descriptionForeground); }
tr[data-file] { cursor: pointer; }
tr[data-file]:hover { background: var(--vscode-list-hoverBackground, #ffffff12); }
`;

function embedJson(model: FlameModel, summary: ProfileSummary): string {
  // <-escape so "</script>" inside frame names can never close the blob.
  return JSON.stringify({ model, summary }).replace(/</g, "\\u003c");
}

/** Build the complete flame-graph webview HTML document. */
export function buildFlameHtml(input: FlameHtmlInput): string {
  const { nonce, model, summary } = input;
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Profile: ${escapeHtml(input.title)}</title>
<style nonce="${nonce}">${FLAME_CSS}</style>
</head>
<body>
<div class="header"><span class="stats">${escapeHtml(formatSummaryHeader(summary))}</span>${chipsHtml(summary)}</div>
<div class="toolbar">
<button id="btn-left" class="toggle active">Left Heavy</button>
<button id="btn-time" class="toggle">Time Order</button>
<select id="fiber-select">${fiberOptionsHtml(model)}</select>
<input id="search" type="text" placeholder="Search functions (Esc clears)">
<span id="match-label" class="stats"></span>
<button id="btn-reset" class="toggle">Reset zoom (0)</button>
</div>
<div id="flame-wrap"><canvas id="flame"></canvas></div>
<div id="tooltip"></div>
${hotFunctionsTable(summary)}
<script type="application/json" id="flame-data" nonce="${nonce}">${embedJson(model, summary)}</script>
<script nonce="${nonce}">${FLAME_SCRIPT}</script>
</body>
</html>`;
}

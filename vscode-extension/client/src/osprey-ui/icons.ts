// The framework's semantic symbols ([UI-COMPONENTS-SYMBOLS]). Each maps to an
// official Codicon; an unknown symbol falls back to the neutral
// information mark rather than injecting the name into markup.

import { readFileSync } from "fs";

const CSS_PATH = "@vscode/codicons/dist/codicon.css";
const FONT_PATH = "@vscode/codicons/dist/codicon.ttf";

const NAMES: Readonly<Record<string, string>> = {
  add: "add",
  delete: "trash",
  dismiss: "close",
  check: "check",
  warning: "warning",
  info: "info",
  question: "question",
  search: "search",
  settings: "gear",
  star: "star-full",
  play: "play",
  pause: "debug-pause",
  stop: "debug-stop",
  refresh: "refresh",
  folder: "folder",
  document: "symbol-file",
  package: "package",
  inbox: "inbox",
  globe: "globe",
  external: "link-external",
  lock: "lock",
  shield: "shield",
  branch: "type-hierarchy",
  tag: "tag",
  clock: "history",
  person: "person",
  cursor: "debug-step-into",
  chevron: "chevron-right",
};

const FALLBACK = "info";

function codicon(name: string): string {
  return Object.prototype.hasOwnProperty.call(NAMES, name) ? NAMES[name] ?? FALLBACK : FALLBACK;
}

/** Official Codicon markup for `name`; never includes the raw symbol string. */
export function symbolIcon(name: string): string {
  return `<span class="codicon codicon-${codicon(name)}" aria-hidden="true"></span>`;
}

let stylesheet = "";

function rule(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const found = new RegExp(`(?:^|\\n)(${escaped}[^{]*\\{[^}]*\\})`).exec(css);
  if (!found) throw new Error(`The installed Codicon stylesheet has no ${selector} rule`);
  return found[1] ?? "";
}

/**
 * The installed Codicon font and the rules for the symbols this renderer uses.
 * The font travels as a data URI so the webview needs no remote fetch and no
 * additional local resource root; the caller must allow `font-src data:`.
 */
export function codiconStyle(): string {
  if (stylesheet !== "") return stylesheet;
  const css = readFileSync(require.resolve(CSS_PATH), "utf8");
  const font = readFileSync(require.resolve(FONT_PATH)).toString("base64");
  const wanted = [...new Set([FALLBACK, ...Object.values(NAMES)])].map((name) => rule(css, `.codicon-${name}`));
  stylesheet = [
    `@font-face{font-family:"codicon";font-display:block;src:url(data:font/ttf;base64,${font}) format("truetype");}`,
    rule(css, ".codicon"),
    ...wanted,
  ].join("\n");
  return stylesheet;
}

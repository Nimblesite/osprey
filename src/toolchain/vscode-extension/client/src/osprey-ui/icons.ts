// The framework's semantic symbols ([UI-COMPONENTS-SYMBOLS]). Each maps to an
// official Codicon; an unknown symbol falls back to the neutral
// information mark rather than injecting the name into markup.

import { readFileSync } from "fs";
import { escapeHtml } from "./html";

const CSS_PATH = "@vscode/codicons/dist/codicon.css";
const FONT_PATH = "@vscode/codicons/dist/codicon.ttf";

// Symbols that draw the official Codicon of the same name; every other known
// symbol needs an explicit alias row below.
const PLAIN = (
  "add check warning info question search play refresh folder book pencil gear " +
  "package inbox globe lock shield tag person terminal edit sparkle"
).split(" ").map((name): [string, string] => [name, name]);

const ALIASES: Readonly<Record<string, string>> = {
  delete: "trash",
  dismiss: "close",
  settings: "gear",
  star: "star-full",
  pause: "debug-pause",
  stop: "debug-stop",
  document: "symbol-file",
  link: "link-external",
  external: "link-external",
  "play.fill": "play",
  "circle.lefthalf.filled": "color-mode",
  "square.and.arrow.down": "save",
  "rectangle.and.pencil.and.ellipsis": "preview",
  textformat: "symbol-text",
  magnifyingglass: "search",
  "chevron.left.forwardslash.chevron.right": "code",
  "hand.draw": "move",
  "arrow.uturn.backward": "discard",
  "square.and.pencil": "edit",
  "arrow.up.left.and.arrow.down.right": "screen-full",
  "square.and.arrow.up": "export",
  "arrow.left": "arrow-left",
  "arrow.right": "arrow-right",
  minus: "remove",
  "chevron.left": "chevron-left",
  network: "type-hierarchy",
  branch: "type-hierarchy",
  cube: "package",
  clock: "history",
  cursor: "debug-step-into",
  chevron: "chevron-right",
  send: "arrow-up",
  panel: "layout-sidebar-right",
};

/** Symbol name to Codicon for every symbol this renderer knows. */
const SYMBOLS: ReadonlyMap<string, string> = new Map([...PLAIN, ...Object.entries(ALIASES)]);

const FALLBACK = "info";

/** Official Codicon markup for `name`; never includes the raw symbol string. */
export function symbolIcon(name: string, asset?: string): string {
  if (name === "") return "";
  if (asset !== undefined) return `<span class="osp-symbol" aria-hidden="true" data-symbol="${escapeHtml(name)}" style="--osp-symbol-image:url(${escapeHtml(JSON.stringify(asset))})"></span>`;
  return `<span class="codicon codicon-${SYMBOLS.get(name) ?? FALLBACK}" aria-hidden="true"></span>`;
}

let stylesheet = "";

function rule(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const found = new RegExp(`(?:^|\\n)(${escaped}(?![\\w-])[^{]*\\{[^}]*\\})`).exec(css);
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
  const wanted = [...new Set([FALLBACK, ...SYMBOLS.values()])].map((name) => rule(css, `.codicon-${name}`));
  stylesheet = [
    `@font-face{font-family:"codicon";font-display:block;src:url(data:font/ttf;base64,${font}) format("truetype");}`,
    rule(css, ".codicon"),
    ...wanted,
  ].join("\n");
  return stylesheet;
}

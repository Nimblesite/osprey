// [GPUI-HOST-RENDER] [NATIVE-SHM-VIEW]: draws a checked frame as DOM. Every colour, size and
// space comes from the row: this renderer applies the application's look and holds no style of
// its own. A control that delivers an event carries the control number the row names, nothing
// else (mirrors hosts/gpui/src/view.rs).

import { FLAGS, type Control, type Shown } from "./rows.js";

export interface RenderOptions {
  /** Approved image and sprite sources, resolved by the extension host. */
  readonly assets?: ReadonlyMap<string, string>;
}

/** Escape text and attribute values before interpolating application text into markup. */
export function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}

function ink(value: number): string {
  return value === 0 ? "transparent" : `#${value.toString(16).padStart(8, "0")}`;
}

function pixels(value: number): string {
  return `${value}px`;
}

function fixed(control: Control): string[] {
  const { fit, look } = control;
  return [
    "box-sizing:border-box",
    "min-width:0",
    `color:${ink(look.ink)}`,
    ...(look.fill === 0 ? [] : [`background:${ink(look.fill)}`]),
    ...(look.size > 0 ? [`font-size:${pixels(look.size)}`] : []),
    `font-weight:${look.weight}`,
    ...(look.padding > 0 ? [`padding:${pixels(look.padding)}`] : []),
    ...(look.spacing > 0 ? [`gap:${pixels(look.spacing)}`] : []),
    ...(look.radius > 0 ? [`border-radius:${pixels(look.radius)}`] : []),
    ...(fit.width > 0 ? [`width:${pixels(fit.width)}`, "flex-shrink:0"] : []),
    ...(fit.height > 0 ? [`height:${pixels(fit.height)}`, "flex-shrink:0"] : []),
    ...(fit.grow > 0 ? [`flex-grow:${fit.grow}`, "flex-basis:0"] : []),
    ...(fit.border !== 0 ? [`border:1px solid ${ink(fit.border)}`] : []),
    ...(fit.flags & FLAGS.fixedWidth ? ["font-family:var(--vscode-editor-font-family,monospace)"] : []),
    ...(fit.flags & FLAGS.slanted ? ["font-style:italic"] : []),
    ...(fit.flags & FLAGS.centred ? ["text-align:center"] : []),
    ...(fit.flags & FLAGS.disabled ? ["opacity:0.5"] : []),
  ];
}

function style(declarations: readonly string[]): string {
  return escapeHtml(declarations.join(";"));
}

function focus(index: number): string {
  return ` data-focus="row-${index}"`;
}

function provenance(control: Control, index: number, words: string): string {
  if (control.action === null) return focus(index);
  return ` data-ordinal="${control.action}" data-words="${escapeHtml(words)}"${focus(index)}`;
}

const SYMBOLS: Readonly<Record<string, string>> = {
  add: "+", delete: "×", dismiss: "×", check: "✓", warning: "!", info: "i", question: "?",
  search: "⌕", settings: "⚙", star: "★", play: "▶", pause: "❚❚", stop: "■", refresh: "↻",
  folder: "▤", document: "▥", package: "▣", inbox: "▤", globe: "◍", external: "↗", lock: "◉",
  shield: "⛨", branch: "⑂", tag: "#", clock: "◔", person: "☺", cursor: "➤", chevron: "›",
  send: "↑", terminal: "▮", edit: "✎", panel: "▯", sparkle: "✦",
};

/** The icon a label or press is led by, when its hint names one. */
function symbol(name: string): string {
  if (name === "") return "";
  const glyph = SYMBOLS[name] ?? "▢";
  return `<span class="osp-view-symbol" aria-hidden="true"${name in SYMBOLS ? "" : ` data-unknown-symbol="${escapeHtml(name)}"`}>${escapeHtml(glyph)}</span>`;
}

function led(control: Control, body: string): string {
  return control.hint === "" ? body
    : `<span class="osp-view-led">${symbol(control.hint)}${body}</span>`;
}

function words(control: Control): string {
  return `<span class="osp-view-words">${escapeHtml(control.words)}</span>`;
}

function children(shown: Shown, control: Control, options: RenderOptions): string {
  return control.children.map((row) => draw(shown, row, options)).join("");
}

function container(shown: Shown, control: Control, index: number, direction: string, options: RenderOptions): string {
  return `<div class="osp-view-control osp-view-${control.kind}" style="${style([...fixed(control), "display:flex", `flex-direction:${direction}`,
    ...(control.kind === "line" ? ["align-items:center"] : []),
    ...(control.kind === "scroll" ? ["overflow-y:auto", "min-height:0"] : [])])}"${focus(index)}>${children(shown, control, options)}</div>`;
}

function layered(shown: Shown, control: Control, index: number, options: RenderOptions): string {
  const inside = control.children.map((row, at) => {
    const body = draw(shown, row, options);
    return at === 0 ? body : `<div class="osp-view-layer-slot" style="position:absolute;inset:0">${body}</div>`;
  }).join("");
  return `<div class="osp-view-control osp-view-layer" style="${style([...fixed(control), "display:flex", "flex-direction:column", "position:relative"])}"${focus(index)}>${inside}</div>`;
}

function label(control: Control, index: number): string {
  return `<span class="osp-view-control osp-view-label" style="${style([...fixed(control), "display:inline-flex", "align-items:center"])}"${focus(index)}>${led(control, words(control))}</span>`;
}

function press(control: Control, index: number): string {
  const disabled = (control.fit.flags & FLAGS.disabled) !== 0;
  return `<button type="button" class="osp-view-control osp-view-press" style="${style([...fixed(control), "display:inline-flex", "align-items:center", "justify-content:center", "cursor:pointer"])}"${provenance(control, index, "")}${disabled ? " disabled" : ""}>${led(control, words(control))}</button>`;
}

function field(control: Control, index: number): string {
  const flags = control.fit.flags;
  const kind = control.kind === "editor" ? "osp-view-editor" : "osp-view-entry";
  const type = flags & FLAGS.secret ? "password" : "text";
  const commit = flags & FLAGS.onCommit ? "enter" : "change";
  return `<input class="osp-view-control ${kind}" type="${type}" value="${escapeHtml(control.words)}" placeholder="${escapeHtml(control.hint)}" autocomplete="off" spellcheck="false" data-commit="${commit}" style="${style([...fixed(control), "font:inherit"])}"${provenance(control, index, "")}${flags & FLAGS.disabled ? " disabled readonly" : ""} />`;
}

function toggle(control: Control, index: number): string {
  const chosen = (control.fit.flags & FLAGS.chosen) !== 0;
  const disabled = (control.fit.flags & FLAGS.disabled) !== 0;
  return `<button type="button" role="switch" aria-checked="${chosen}" class="osp-view-control osp-view-toggle" style="${style([...fixed(control), "display:inline-flex", "align-items:center", "gap:8px", "cursor:pointer"])}"${provenance(control, index, chosen ? "false" : "true")}${disabled ? " disabled" : ""}><span class="osp-view-switch" aria-hidden="true" data-chosen="${chosen}">${chosen ? "✓" : ""}</span>${words(control)}</button>`;
}

function picker(shown: Shown, control: Control, index: number): string {
  const choices = control.children.map((row) => {
    const option = shown.controls[row];
    if (option === undefined) return "";
    const chosen = (option.fit.flags & FLAGS.chosen) !== 0;
    return `<button type="button" class="osp-view-control osp-view-option" style="${style([...fixed(option), "display:inline-flex", "align-items:center", "cursor:pointer",
      ...(chosen ? ["outline:1px solid currentColor"] : [])])}"${provenance(control, row, option.hint)}>${words(option)}</button>`;
  }).join("");
  return `<div class="osp-view-control osp-view-picker" style="${style([...fixed(control), "display:flex", "flex-wrap:wrap", "align-items:center"])}"${focus(index)}>${words(control)}${choices}</div>`;
}

function rule(control: Control, index: number): string {
  return `<div class="osp-view-control osp-view-rule" role="separator" style="${style([...fixed(control), "height:" + pixels(Math.max(control.look.size, 1)), "width:100%", `background:${ink(control.look.ink)}`])}"${focus(index)}></div>`;
}

function image(control: Control, index: number, options: RenderOptions): string {
  const source = options.assets?.get(control.words);
  if (source === undefined) {
    return `<div class="osp-view-control osp-view-image-missing" role="img" aria-label="${escapeHtml(control.words)}" style="${style([...fixed(control), "display:flex", "align-items:center", "justify-content:center", "font-size:12px", "opacity:0.6"])}"${focus(index)}>${escapeHtml(control.words)}</div>`;
  }
  return `<img class="osp-view-control osp-view-image" src="${escapeHtml(source)}" alt="${escapeHtml(control.words)}" loading="lazy" decoding="async" style="${style([...fixed(control), "object-fit:contain"])}"${focus(index)} />`;
}

function canvas(control: Control, index: number): string {
  const ordinal = control.action === null ? "-1" : String(control.action);
  return `<div class="osp-canvas-frame" style="${style([...fixed(control), "display:flex", "min-height:320px"])}"><canvas class="osp-canvas" tabindex="0" role="img" aria-label="Scene" data-scene="${index}" data-ordinal="${ordinal}"${focus(index)} style="width:100%;height:100%;display:block"></canvas></div>`;
}

function sheet(shown: Shown, control: Control, index: number, options: RenderOptions): string {
  const dismiss = control.action === null ? "" :
    `<button type="button" class="osp-view-dismiss" style="align-self:flex-end;cursor:pointer;background:transparent;border:0;color:inherit" data-ordinal="${control.action}" data-words="" aria-label="Dismiss">${symbol("dismiss")}</button>`;
  return `<div class="osp-view-control osp-view-sheet" style="${style([...fixed(control), "display:flex", "flex-direction:column", "position:relative"])}"${focus(index)}>${dismiss}${children(shown, control, options)}</div>`;
}

function draw(shown: Shown, index: number, options: RenderOptions): string {
  const control = shown.controls[index];
  if (control === undefined) return "";
  switch (control.kind) {
    case "column":
    case "menu":
      return container(shown, control, index, "column", options);
    case "line":
      return container(shown, control, index, "row", options);
    case "scroll":
      return container(shown, control, index, "column", options);
    case "layer":
      return layered(shown, control, index, options);
    case "label":
      return label(control, index);
    case "press":
      return press(control, index);
    case "entry":
    case "editor":
      return field(control, index);
    case "gap":
      return `<div class="osp-view-control osp-view-gap" style="flex-grow:1"${focus(index)}></div>`;
    case "toggle":
      return toggle(control, index);
    case "picker":
      return picker(shown, control, index);
    case "choice":
      return "";
    case "rule":
      return rule(control, index);
    case "image":
      return image(control, index, options);
    case "canvas":
      return canvas(control, index);
    case "sheet":
      return sheet(shown, control, index, options);
    default:
      return "";
  }
}

/** Draws every root row top to bottom. */
export function renderShown(shown: Shown, options: RenderOptions = {}): string {
  return `<div class="osp-view">${shown.roots.map((root) => draw(shown, root, options)).join("")}</div>`;
}

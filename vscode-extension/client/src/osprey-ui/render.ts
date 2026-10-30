// Implements [UI-COMPONENTS]: a generic Node-side renderer for the native
// component vocabulary. It owns no package state; every control carries the
// region, revision and ordinal the framework issued, and every untrusted
// string is escaped before it reaches the webview.

import { symbolIcon } from "./icons";
import { record, type Regions, type RegionSnapshot } from "./protocol";
import { themeStyle, themeDeclarations } from "./theme";
import { escapeHtml } from "./html";
import { renderMath } from "./math";
import { renderEditor } from "./editor";
import { renderBoard } from "./board";
import { renderImage, renderVideo } from "./media";
import { renderFrame } from "./frame";
export { escapeHtml } from "./html";

export interface RenderOptions {
  readonly regions: Regions;
  readonly assetUri?: string;
  readonly symbols?: ReadonlyMap<string, string>;
  readonly theme?: unknown;
  readonly maxNodes?: number;
  readonly slices?: ReadonlyMap<string, string>;
  readonly resolveAsset?: (source: string) => string;
  readonly loadingText?: string;
}

interface Context extends RenderOptions {
  readonly region: string;
  readonly revision: number;
  readonly depth: number;
  readonly trail: ReadonlySet<string>;
  readonly budget: { nodes: number };
}

function symbol(name: string, ctx: Context): string {
  return symbolIcon(name, ctx.symbols?.get(name));
}

const MAX_DEPTH = 64;
const DEFAULT_MAX_NODES = 6000;

const ROLES = new Set(["display", "large", "title", "heading", "lead", "headline", "body", "callout", "caption", "eyebrow", "mono"]);
const TONES = new Set(["primary", "secondary", "accent", "good", "caution", "danger"]);
const BUTTON_ROLES = new Set(["prominent", "plain", "destructive", "filled"]);

function text(node: Record<string, unknown>, key: string): string {
  return typeof node[key] === "string" ? node[key] : "";
}

function number(node: Record<string, unknown>, key: string): number {
  return typeof node[key] === "number" && Number.isSafeInteger(node[key]) ? node[key] : 0;
}

function flag(node: Record<string, unknown>, key: string): boolean {
  return node[key] === true;
}

function items(node: Record<string, unknown>, key: string): unknown[] {
  return Array.isArray(node[key]) ? node[key] : [];
}

function oneOf(value: string, allowed: Set<string>, fallback: string): string {
  return allowed.has(value) ? value : fallback;
}

function eventAttrs(ctx: Context, node: Record<string, unknown>): string {
  if (typeof node.event !== "number" || !Number.isSafeInteger(node.event) || node.event < 0) return "";
  return ` data-event-region="${escapeHtml(ctx.region)}" data-event-revision="${ctx.revision}" data-event-ordinal="${node.event}"`;
}

function focusAttr(ctx: Context, node: Record<string, unknown>): string {
  return typeof node.event === "number" ? ` data-focus="${escapeHtml(ctx.region)}:${node.event}"` : "";
}

function valueAttrs(kind: "string" | "bool" | "number" | "null", value: unknown): string {
  return ` data-value-kind="${kind}" data-value="${escapeHtml(String(value ?? ""))}"`;
}

function control(ctx: Context, node: Record<string, unknown>, kind: "string" | "bool" | "number" | "null", value: unknown): string {
  return eventAttrs(ctx, node) + valueAttrs(kind, value) + focusAttr(ctx, node);
}

function child(value: unknown, ctx: Context): string {
  return renderNode(value, { ...ctx, depth: ctx.depth + 1 });
}

function children(node: Record<string, unknown>, ctx: Context): string {
  return items(node, "children").map((item) => child(item, ctx)).join("");
}

function textNode(node: Record<string, unknown>): string {
  const role = oneOf(text(node, "role"), ROLES, "body");
  const tone = oneOf(text(node, "tone"), TONES, "primary");
  return `<p class="osp-text osp-role-${role} osp-tone-${tone}">${escapeHtml(text(node, "text"))}</p>`;
}

function richText(node: Record<string, unknown>): string {
  const role = oneOf(text(node, "role"), ROLES, "body");
  if (!Array.isArray(node.runs) || node.runs.length > 100000) throw new Error("Rich text requires a bounded run list");
  const content = node.runs.map(run => {
    if (!record(run) || typeof run.text !== "string" || typeof run.italic !== "boolean" || typeof run.ink !== "string") throw new Error("Rich text run is malformed");
    if (run.ink !== "" && !/^[a-z][a-zA-Z0-9-]{0,31}$/.test(run.ink)) throw new Error("Rich text ink must name a theme token");
    const ink = run.ink === "" ? "" : `color:var(--osp-brand-${run.ink});`;
    return `<span style="${ink}font-style:${run.italic ? "italic" : "normal"}">${escapeHtml(run.text)}</span>`;
  }).join("");
  return `<p class="osp-text osp-rich-text osp-role-${role} osp-tone-primary">${content}</p>`;
}

function math(node: Record<string, unknown>, ctx: Context): string {
  return renderMath(node.math, node.spoken, (depth) => consume(ctx, ctx.depth + depth));
}

function editor(node: Record<string, unknown>, ctx: Context): string {
  return renderEditor(node, control(ctx, node, "string", text(node, "value")), `editor-${ctx.region}-${number(node, "event")}-feedback`);
}

function board(node: Record<string, unknown>, ctx: Context): string {
  return renderBoard(node, eventAttrs(ctx, node), ctx.slices, ctx.resolveAsset);
}

function labelValue(node: Record<string, unknown>, kind: "evidence" | "pair"): string {
  return `<div class="osp-${kind}"><span class="osp-${kind}-label">${escapeHtml(text(node, "label"))}</span><span class="osp-${kind}-value">${escapeHtml(text(node, "value"))}</span></div>`;
}

function badge(node: Record<string, unknown>, ctx: Context): string {
  const tone = oneOf(text(node, "tone"), TONES, "primary");
  return `<span class="osp-badge osp-tone-${tone}">${symbol(text(node, "symbol"), ctx)}<span>${escapeHtml(text(node, "label"))}</span></span>`;
}

function label(node: Record<string, unknown>, ctx: Context): string {
  return `<span class="osp-label">${symbol(text(node, "symbol"), ctx)}<span>${escapeHtml(text(node, "label"))}</span></span>`;
}

function icon(node: Record<string, unknown>, ctx: Context): string {
  return `<span class="osp-icon">${symbol(text(node, "symbol"), ctx)}</span>`;
}

function link(node: Record<string, unknown>, ctx: Context): string {
  const address = text(node, "url");
  // The host opens only HTTPS; an HTTP or credential-bearing link renders disabled.
  const safe = /^https:\/\/[^\s]+$/i.test(address) && !/^https:\/\/[^/]*@/i.test(address);
  const body = `${symbol("link", ctx)}<span>${escapeHtml(text(node, "label"))}</span>`;
  return safe
    ? `<a class="osp-link" href="${escapeHtml(address)}" rel="noreferrer noopener" data-external="true">${body}</a>`
    : `<span class="osp-link osp-link-disabled">${body}</span>`;
}

function spacer(): string {
  return '<span class="osp-spacer" aria-hidden="true"></span>';
}

function divider(): string {
  return '<hr class="osp-divider" />';
}

function segments(node: Record<string, unknown>): string {
  const bars = items(node, "tones").map((tone) => `<span class="osp-segment osp-tone-${oneOf(String(tone), TONES, "primary")}"></span>`).join("");
  return `<span class="osp-segments" role="img" aria-label="${escapeHtml(text(node, "label"))}">${bars}</span>`;
}

function empty(node: Record<string, unknown>, ctx: Context): string {
  return `<div class="osp-empty"><span class="osp-empty-symbol">${symbol(text(node, "symbol"), ctx)}</span><h2>${escapeHtml(text(node, "title"))}</h2><p>${escapeHtml(text(node, "detail"))}</p></div>`;
}

function progress(node: Record<string, unknown>): string {
  return `<div class="osp-progress" role="progressbar" aria-label="${escapeHtml(text(node, "label"))}"><div class="osp-progress-track"><span></span></div><span class="osp-progress-label">${escapeHtml(text(node, "label"))}</span><span class="osp-progress-detail">${escapeHtml(text(node, "detail"))}</span></div>`;
}

// [UI-COMPONENTS-METER] A bar filled to value over total in its tone; the label reads it out.
function meter(node: Record<string, unknown>): string {
  const total = Math.max(1, number(node, "total"));
  const value = Math.min(total, Math.max(0, number(node, "value")));
  const tone = oneOf(text(node, "tone"), TONES, "accent");
  const label = escapeHtml(text(node, "label"));
  return `<div class="osp-meter osp-tone-${tone}" role="meter" aria-valuemin="0" aria-valuemax="${total}" aria-valuenow="${value}" aria-label="${label}"><div class="osp-meter-track"><span style="width:${((value / total) * 100).toFixed(1)}%"></span></div><span class="osp-meter-label">${label}</span></div>`;
}

function swatch(node: Record<string, unknown>): string {
  const light = /^#[0-9a-fA-F]{6}$/.test(text(node, "light")) ? text(node, "light") : "transparent";
  const dark = /^#[0-9a-fA-F]{6}$/.test(text(node, "dark")) ? text(node, "dark") : "transparent";
  return `<span class="osp-swatch" style="--osp-swatch-light:${light};--osp-swatch-dark:${dark}"><span></span><span>${escapeHtml(text(node, "name"))}</span></span>`;
}

function brand(ctx: Context): string {
  return ctx.assetUri
    ? `<img class="osp-brand" src="${escapeHtml(ctx.assetUri)}" alt="Osprey" />`
    : '<span class="osp-brand osp-brand-word">Osprey</span>';
}

function slot(node: Record<string, unknown>, ctx: Context): string {
  const key = text(node, "key");
  const region = ctx.regions.get(key);
  if (!region || ctx.trail.has(key)) return `<div class="osp-slot" data-region-slot="${escapeHtml(key)}"></div>`;
  const trail = new Set(ctx.trail);
  trail.add(key);
  const nested: Context = { ...ctx, region: key, revision: region.revision, trail };
  return `<div class="osp-region" data-region-key="${escapeHtml(key)}" data-region-revision="${region.revision}">${renderNode(region.node, { ...nested, depth: ctx.depth + 1 })}</div>`;
}

function stack(node: Record<string, unknown>, ctx: Context): string {
  const axis = text(node, "axis") === "horizontal" ? "horizontal" : "vertical";
  return `<div class="osp-stack osp-axis-${axis}">${children(node, ctx)}</div>`;
}

function section(node: Record<string, unknown>, ctx: Context): string {
  const title = text(node, "title");
  const footer = text(node, "footer");
  const head = title === "" ? "" : `<header class="osp-section-title">${escapeHtml(title)}</header>`;
  const foot = footer === "" ? "" : `<footer class="osp-section-footer">${escapeHtml(footer)}</footer>`;
  return `<section class="osp-section">${head}<div class="osp-section-body">${children(node, ctx)}</div>${foot}</section>`;
}

function screen(node: Record<string, unknown>, ctx: Context): string {
  const title = text(node, "title");
  const subtitle = text(node, "subtitle");
  const toolbar = items(node, "toolbar").map((item) => child(item, ctx)).join("");
  return `<div class="osp-screen"><header class="osp-screen-header"><div class="osp-brand-lockup">${brand(ctx)}<span class="osp-brand-name">Osprey</span></div><div class="osp-screen-titles"><h1>${escapeHtml(title)}</h1><p>${escapeHtml(subtitle)}</p></div><div class="osp-toolbar">${toolbar}</div></header><div class="osp-screen-body">${children(node, ctx)}</div></div>`;
}

function split(node: Record<string, unknown>, ctx: Context): string {
  return `<div class="osp-split">${items(node, "children").map((pane) => `<div class="osp-pane">${child(pane, ctx)}</div>`).join("")}</div>`;
}

function fillsLayer(node: unknown, depth = 0): boolean {
  if (depth > MAX_DEPTH) throw new Error("Native layer exceeds the component depth limit");
  if (!record(node)) return false;
  if (node.kind === "aside") return true;
  return node.kind === "themed" ? fillsLayer(node.body, depth + 1)
    : node.kind === "framed" && record(node.style) && node.style.grow === true;
}

function layer(node: Record<string, unknown>, ctx: Context): string {
  const content = items(node, "children");
  const filling = fillsLayer(content[0]) ? " osp-layer-fill" : "";
  return `<div class="osp-layer${filling}">${content.map(node => child(node, ctx)).join("")}</div>`;
}

function contextMenu(node: Record<string, unknown>, ctx: Context): string {
  const actions = items(node, "items").map((item) => child(item, ctx)).join("");
  return `<div class="osp-context-host" data-context-host><div class="osp-context-body">${child(node.body, ctx)}</div><div class="osp-menu" role="menu" hidden>${actions}</div></div>`;
}

function splitter(node: Record<string, unknown>, ctx: Context): string {
  const least = number(node, "least") || 240;
  const most = number(node, "most") || 720;
  const width = Math.min(Math.max(number(node, "width"), least), Math.max(least, most));
  return `<div class="osp-splitter" role="separator" aria-orientation="vertical" tabindex="0" data-splitter="true" data-side="${escapeHtml(text(node, "side"))}" data-min="${least}" data-max="${most}"${eventAttrs(ctx, node)}${valueAttrs("number", width)}><span></span></div>`;
}

function aside(node: Record<string, unknown>, ctx: Context): string {
  const side = text(node, "side") === "leading" ? "leading" : "trailing";
  const least = number(node, "least") || 240;
  const most = Math.max(least, number(node, "most") || 720);
  const width = Math.min(Math.max(number(node, "width"), least), most);
  const pane = `<div class="osp-aside-pane">${child(node.pane, ctx)}</div>`;
  const column = `<div class="osp-aside-column">${child(node.aside, ctx)}</div>`;
  const dividerNode = splitter(node, ctx);
  const content = side === "leading" ? `${column}${dividerNode}${pane}` : `${pane}${dividerNode}${column}`;
  return `<div class="osp-aside osp-side-${side}" style="--osp-aside-width:${width}px">${content}</div>`;
}

function tabBody(node: Record<string, unknown>, ctx: Context): string {
  const tabs = items(node, "tabs");
  const selected = text(node, "selected");
  const buttons = tabs.map((item) => {
    if (!record(item)) return "";
    const key = text(item, "key");
    return `<button type="button" role="tab" class="osp-tab${key === selected ? " osp-selected" : ""}" aria-selected="${key === selected}"${control(ctx, node, "string", key)}>${symbol(text(item, "symbol"), ctx)}<span>${escapeHtml(text(item, "label"))}</span></button>`;
  }).join("");
  const active = tabs.map((item) => record(item) && text(item, "key") === selected ? child(item.child, ctx) : "").join("");
  return `<div class="osp-tabs"><div class="osp-tablist" role="tablist">${buttons}</div><div class="osp-tabbody" role="tabpanel">${active}</div></div>`;
}

function rowButton(item: Record<string, unknown>, ctx: Context, rowsNode: Record<string, unknown>): string {
  const key = text(item, "key");
  const selected = text(rowsNode, "selected") === key;
  return `<button type="button" role="option" aria-selected="${selected}" class="osp-row${selected ? " osp-selected" : ""}" data-row-key="${escapeHtml(key)}"${control(ctx, rowsNode, "string", key)}>${text(item, "symbol") === "" ? "" : symbol(text(item, "symbol"), ctx)}<span class="osp-row-cell">${child(item.child, ctx)}</span></button>`;
}

function rows(node: Record<string, unknown>, ctx: Context): string {
  let group = "";
  const body = items(node, "rows").map((item) => {
    if (!record(item)) return "";
    const nextGroup = text(item, "group");
    const heading = nextGroup !== "" && nextGroup !== group
      ? `<div class="osp-row-group" role="presentation">${escapeHtml(nextGroup)}</div>`
      : "";
    group = nextGroup;
    return heading + rowButton(item, ctx, node);
  }).join("");
  const label = text(node, "label") || "Items";
  return `<div class="osp-rows" role="listbox" aria-label="${escapeHtml(label)}">${body}</div>`;
}

function button(node: Record<string, unknown>, ctx: Context): string {
  const role = oneOf(text(node, "role"), BUTTON_ROLES, "plain");
  const disabled = flag(node, "enabled") ? "" : " disabled";
  return `<button type="button" class="osp-button osp-button-${role}" aria-label="${escapeHtml(text(node, "label"))}" title="${escapeHtml(text(node, "label"))}"${disabled}${control(ctx, node, "null", null)}>${symbol(text(node, "symbol"), ctx)}<span>${escapeHtml(text(node, "label"))}</span></button>`;
}

function field(node: Record<string, unknown>, ctx: Context): string {
  const multiline = flag(node, "multiline");
  const commit = text(node, "commit") === "submit" ? "submit" : "edit";
  const name = escapeHtml(text(node, "label"));
  const tag = multiline
    ? `<textarea aria-label="${name}" rows="4" placeholder="${escapeHtml(text(node, "prompt"))}" data-commit="${commit}"${control(ctx, node, "string", text(node, "value"))}>${escapeHtml(text(node, "value"))}</textarea>`
    : `<input aria-label="${name}" type="${flag(node, "secure") ? "password" : "text"}" value="${escapeHtml(text(node, "value"))}" placeholder="${escapeHtml(text(node, "prompt"))}" data-commit="${commit}"${control(ctx, node, "string", text(node, "value"))} />`;
  return `<label class="osp-field"><span>${name}</span>${tag}</label>`;
}

function toggle(node: Record<string, unknown>, ctx: Context): string {
  const on = flag(node, "on");
  return `<button type="button" class="osp-toggle${on ? " osp-on" : ""}" role="switch" aria-checked="${on}"${control(ctx, node, "bool", !on)}><span class="osp-switch"><span></span></span><span>${escapeHtml(text(node, "label"))}</span></button>`;
}

function choiceList(node: Record<string, unknown>): string {
  const selected = text(node, "selected");
  return items(node, "choices").map((item) => {
    if (!record(item)) return "";
    const key = text(item, "key");
    return `<option value="${escapeHtml(key)}"${key === selected ? " selected" : ""}>${escapeHtml(text(item, "label"))}</option>`;
  }).join("");
}

function picker(node: Record<string, unknown>, ctx: Context): string {
  return `<label class="osp-field"><span>${escapeHtml(text(node, "label"))}</span><select data-commit="submit"${control(ctx, node, "string", text(node, "selected"))}>${choiceList(node)}</select></label>`;
}

function disclosure(node: Record<string, unknown>, ctx: Context): string {
  const expanded = flag(node, "expanded");
  return `<div class="osp-disclosure"><button type="button" class="osp-disclosure-head" aria-expanded="${expanded}"${control(ctx, node, "bool", !expanded)}><span class="osp-disclosure-arrow${expanded ? " osp-open" : ""}">${symbol("chevron.right", ctx)}</span><span>${escapeHtml(text(node, "title"))}</span></button><div class="osp-disclosure-body"${expanded ? "" : " hidden"}>${children(node, ctx)}</div></div>`;
}

function alert(node: Record<string, unknown>, ctx: Context): string {
  if (!flag(node, "visible")) return "";
  return `<div class="osp-alert" role="alert"${control(ctx, node, "null", null)}><span class="osp-alert-icon">${symbol("warning", ctx)}</span><div><strong>${escapeHtml(text(node, "title"))}</strong><p>${escapeHtml(text(node, "message"))}</p></div></div>`;
}

function dialog(node: Record<string, unknown>, ctx: Context): string {
  return `<section class="osp-dialog" role="dialog" aria-modal="true" aria-label="${escapeHtml(text(node, "title"))}" tabindex="-1" data-native-dialog="true"${control(ctx, node, "null", null)}>${child(node.body, ctx)}</section>`;
}

function themed(node: Record<string, unknown>, ctx: Context): string {
  return `<div class="osp-themed" style="${escapeHtml(themeDeclarations(node.theme))}">${child(node.body, ctx)}</div>`;
}

const RENDERERS: Readonly<Record<string, (node: Record<string, unknown>, ctx: Context) => string>> = {
  text: textNode,
  "rich-text": richText,
  math,
  editor,
  board,
  image: (node, ctx) => renderImage(node, ctx.resolveAsset),
  video: (node, ctx) => renderVideo(node, ctx.resolveAsset),
  evidence: node => labelValue(node, "evidence"),
  pair: node => labelValue(node, "pair"),
  brand: (_node, ctx) => brand(ctx),
  badge,
  label,
  icon,
  link,
  spacer,
  divider,
  segments,
  empty,
  progress,
  meter,
  swatch,
  slot,
  button,
  field,
  toggle,
  picker,
  disclosure,
  alert,
  dialog,
  themed,
  stack,
  section,
  screen,
  split,
  layer,
  framed: (node, ctx) => renderFrame(node, body => child(body, ctx)),
  "context-menu": contextMenu,
  aside,
  tabs: tabBody,
  rows,
};

/** Every node kind this webview draws ([NATIVE-HOSTS-COVERAGE]); any other kind is refused as a protocol defect. */
export const RENDERED_KINDS: readonly string[] = Object.keys(RENDERERS);

/** Render one protocol node; an unknown component is a protocol defect and throws. */
export function renderNode(value: unknown, ctx: Context): string {
  if (!record(value) || typeof value.kind !== "string") throw new Error("Native component is not an object with a kind");
  consume(ctx, ctx.depth);
  const renderer = RENDERERS[value.kind];
  if (!renderer) throw new Error(`Unknown native component: ${value.kind}`);
  return renderer(value, ctx);
}

function consume(ctx: Context, depth: number): void {
  if (depth > MAX_DEPTH) throw new Error("Native component tree is too deep");
  ctx.budget.nodes += 1;
  if (ctx.budget.nodes > (ctx.maxNodes ?? DEFAULT_MAX_NODES)) throw new Error("Native component tree is too large");
}

function rootContext(options: RenderOptions, root: RegionSnapshot): Context {
  return { ...options, region: "root", revision: root.revision, depth: 0, trail: new Set(["root"]), budget: { nodes: 0 } };
}

/** Render the current keyed regions, resolving slot placeholders against their retained regions. */
export function renderWorkspace(options: RenderOptions): string {
  const root = options.regions.get("root");
  const loading = `<div class="osp-loading"><span class="osp-spinner" aria-hidden="true"></span><p>${escapeHtml(options.loadingText ?? "Loading your packages…")}</p></div>`;
  return themeStyle(options.theme) + (root
    ? `<div class="osp-region osp-root" data-region-key="root" data-region-revision="${root.revision}">${renderNode(root.node, rootContext(options, root))}</div>`
    : loading);
}

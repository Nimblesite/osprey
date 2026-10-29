// Implements [UI-COMPONENTS-FRAMED]: application-owned layout metrics are
// drawn mechanically by every host; this module makes no layout decisions.
import { record } from "./wire";

function metric(style: Record<string, unknown>, key: string): number {
  const value = key === "gap" && style[key] === undefined ? 0 : style[key];
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0 || value > 32768) {
    throw new Error(`Frame ${key} must be a bounded non-negative integer`);
  }
  return value;
}

function token(style: Record<string, unknown>, key: string): string {
  const value = key === "ink" && style[key] === undefined ? "" : style[key];
  if (value === "") return "";
  if (typeof value !== "string" || !/^[a-z][a-zA-Z0-9-]{0,31}$/.test(value)) throw new Error(`Frame ${key} must name a theme token`);
  return `var(--osp-brand-${value})`;
}

function validateSwitches(style: Record<string, unknown>): void {
  for (const key of ["italic", "hug", "quietControls", "iconOnly", "flatRows", "fadeBackdrop", "trailingSymbols"]) {
    if (style[key] !== undefined && typeof style[key] !== "boolean") throw new Error(`Frame ${key} must be a boolean`);
  }
}

function presentationValues(style: Record<string, unknown>) {
  const font = style.font ?? "", align = style.align ?? "start", cross = style.cross ?? "stretch";
  if (!["", "serif", "sans", "mono"].includes(String(font))) throw new Error("Frame font must name a supported family");
  if (!["start", "center", "end", "baseline"].includes(String(align))) throw new Error("Frame align must be start, center or end");
  if (!["stretch", "start", "center", "end"].includes(String(cross))) throw new Error("Frame cross must be stretch, start, center or end");
  validateSwitches(style);
  const controlPadding = style.controlPadding === undefined || style.controlPadding === -1 ? -1 : metric(style, "controlPadding");
  const controlHeight = style.controlHeight === undefined ? 0 : metric(style, "controlHeight");
  const tracking = style.tracking ?? 0;
  if (typeof tracking !== "number" || !Number.isFinite(tracking) || Math.abs(tracking) > 32768) throw new Error("Frame tracking must be a bounded number");
  const leading = style.lineHeight ?? 0;
  if (typeof leading !== "number" || !Number.isFinite(leading) || leading < 0 || leading > 32768) throw new Error("Frame lineHeight must be a bounded non-negative number");
  const mathScale = style.mathScale ?? 0;
  if (typeof mathScale !== "number" || !Number.isFinite(mathScale) || mathScale < 0 || mathScale > 16) throw new Error("Frame mathScale must be a bounded non-negative number");
  const size = style.textSize === undefined ? 0 : metric(style, "textSize");
  return { font, align, cross, controlPadding, controlHeight, tracking, leading, mathScale, size };
}

function presentation(style: Record<string, unknown>): string[] {
  const { font, align, cross, controlPadding, controlHeight, tracking, leading, mathScale, size } = presentationValues(style);
  return [...effects(style), `--osp-stack-align:${align === "start" ? "flex-start" : align === "end" ? "flex-end" : align}`, "display:flex", "flex-direction:column", `align-items:${cross}`, `justify-content:${align === "baseline" ? "start" : align}`, `align-content:${align}`, ...(font === "" ? [] : [`--osp-frame-font:var(--osp-font-${font})`]),
    ...(size === 0 ? [] : [`--osp-frame-text-size:${size}px`]),
    ...(mathScale === 0 ? [] : [`--osp-frame-math-scale:${mathScale}`]),
    ...(leading === 0 ? [] : [`--osp-frame-line-height:${leading}px`]),
    ...(tracking === 0 ? [] : [`--osp-frame-tracking:${tracking}px`]),
    ...(font === "" && size === 0 ? [] : [`--osp-frame-font-style:${style.italic ? "italic" : "normal"}`]),
    ...(style.hug ? ["width:fit-content", "flex-shrink:0"] : []),
    ...(controlHeight > 0 ? [`--osp-frame-control-height:${controlHeight}px`] : []),
    ...(controlPadding >= 0 ? [`--osp-frame-control-padding:${controlPadding}px`] : []),
    ...(style.quietControls ? ["--osp-frame-control-fill:transparent"] : []),
    ...(style.iconOnly ? ["--osp-frame-icon-label:none", "--osp-frame-button-padding:0"] : []),
    ...(style.flatRows ? ["--osp-frame-row-padding:0", "--osp-frame-row-radius:3px", "--osp-frame-row-border:0"] : [])];
}

function effects(style: Record<string, unknown>): string[] {
  const mask = "linear-gradient(to right, transparent, #000 5%, #000 60%, transparent), linear-gradient(to bottom, transparent, #000 5%, #000 86%, transparent)";
  return [...shadow(style), `--osp-frame-backdrop-mask:${style.fadeBackdrop ? mask : "none"}`, `--osp-frame-backdrop-blend:${style.fadeBackdrop ? "lighten" : "normal"}`, `--osp-frame-symbol-direction:${style.trailingSymbols ? "row-reverse" : "inherit"}`];
}

function shadow(style: Record<string, unknown>): string[] {
  const value = style.shadow === undefined ? "" : style.shadow;
  if (value !== "" && value !== "floating") throw new Error("Frame shadow must be empty or floating");
  return value === "" ? [] : ["box-shadow:0 20px 50px color-mix(in srgb, var(--osp-brand-primary) 13.33%, transparent)"];
}

function declarations(style: Record<string, unknown>): string[] {
  if (typeof style.grow !== "boolean") throw new Error("Frame grow must be a boolean");
  const padding = ["top", "right", "bottom", "left"].map(key => `${metric(style, key)}px`).join(" ");
  const sizes = ["width", "height"].flatMap(key => metric(style, key) === 0 ? [] : [`${key}:${metric(style, key)}px`]);
  const edge = style.lineEdge ?? "all";
  if (!["all", "top", "right", "bottom", "left"].includes(String(edge))) throw new Error("Frame lineEdge must name an edge");
  const border = edge === "all" ? "border" : `border-${edge}`;
  const surfaces = [["fill", "background"], ["ink", "color"], ["line", border]].flatMap(([key, property]) => {
    const colour = token(style, key);
    return colour === "" ? [] : [`${property}:${key === "line" ? "1px solid " : ""}${colour}`];
  });
  return [...presentation(style), ...sizes, ...surfaces, `padding:${padding}`, `border-radius:${metric(style, "radius")}px`,
    `--osp-stack-gap:${metric(style, "gap")}px`, ...(style.grow ? ["flex:1", "min-height:0", "align-self:stretch"] : [])];
}

/** Draw a shared frame with its application-authored geometry and content. */
export function renderFrame(node: Record<string, unknown>, render: (body: unknown) => string): string {
  if (!record(node.style)) throw new Error("Frame requires its application-authored style");
  const name = node.style.name ?? "";
  if (typeof name !== "string" || (name !== "" && !/^[a-z][a-z0-9-]{0,63}$/.test(name))) throw new Error("Frame name must be a safe semantic role");
  const role = name === "" ? "" : ` osp-frame-${name}`;
  const widthMode = metric(node.style, "width") > 0 ? "fixed" : node.style.hug ? "content" : "fill";
  return `<div class="osp-framed${role}" data-frame-width="${widthMode}" data-frame-grow="${node.style.grow}" style="box-sizing:border-box;min-width:0;${declarations(node.style).join(";")}">${render(node.body)}</div>`;
}

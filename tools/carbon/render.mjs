#!/usr/bin/env node
// Render Osprey source into a Carbon-style PNG.
//
// Highlighting is NOT reinvented here: it reuses the repository's real
// TextMate grammars (vscode-extension/syntaxes/osprey*.json) through Shiki,
// so the colours match VS Code exactly. The framed result is rasterised to
// PNG via @resvg/resvg-js — no headless browser required.

import { readFileSync } from "node:fs";
import { writeFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createHighlighter } from "shiki";
import { Resvg } from "@resvg/resvg-js";

const HERE = dirname(fileURLToPath(import.meta.url));
const SYNTAXES = resolve(HERE, "..", "..", "vscode-extension", "syntaxes");
const GRAMMARS = {
  osprey: join(SYNTAXES, "osprey.tmGrammar.json"),
  "osprey-ml": join(SYNTAXES, "osprey-ml.tmLanguage.json"),
};

const DEFAULTS = {
  theme: "one-dark-pro",
  fontFamily: "Menlo",
  fontSize: 15,
  lineHeightRatio: 1.6,
  charAspect: 0.6, // monospace advance width / font size (Menlo ≈ 0.6)
  padding: 56, // backdrop margin around the window
  radius: 12,
  scale: 2, // retina multiplier
  background: "grad:#4568dc,#b06ab3",
  lineNumbers: false,
  title: undefined, // defaults to the input filename
  showTitle: true,
};

// Chrome geometry (unscaled px).
const TITLE_BAR_H = 44;
const CODE_PAD = { top: 20, right: 26, bottom: 22, left: 26 };
const DOT = { r: 6, gap: 20, firstX: 22, colours: ["#ff5f56", "#ffbd2e", "#27c93f"] };
const SHADOW = { dy: 22, blur: 26, opacity: 0.45 };
const FONT_STYLE = { italic: 1, bold: 2, underline: 4 };

const xmlEscape = (s) =>
  s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");

const expandTabs = (s, size = 2) => s.replace(/\t/g, " ".repeat(size));

function parseArgs(argv) {
  const opts = { ...DEFAULTS };
  let input;
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    const next = () => argv[(i += 1)];
    switch (arg) {
      case "-o": case "--out": opts.out = next(); break;
      case "--theme": opts.theme = next(); break;
      case "--font": opts.fontFamily = next(); break;
      case "--font-size": opts.fontSize = Number(next()); break;
      case "--padding": opts.padding = Number(next()); break;
      case "--radius": opts.radius = Number(next()); break;
      case "--scale": opts.scale = Number(next()); break;
      case "--bg": opts.background = next(); break;
      case "--transparent": opts.background = "transparent"; break;
      case "--flavor": opts.flavor = next(); break;
      case "--line-numbers": opts.lineNumbers = true; break;
      case "--title": opts.title = next(); break;
      case "--no-title": opts.showTitle = false; break;
      case "-h": case "--help": opts.help = true; break;
      default:
        if (arg.startsWith("-")) throw new Error(`Unknown option: ${arg}`);
        input = arg;
    }
  }
  opts.input = input;
  return opts;
}

const HELP = `osprey-carbon — Carbon-style PNG screenshots of Osprey code

Usage:
  node render.mjs <file.osp|file.ospml> [options]
  cat file.ospml | node render.mjs --flavor osprey-ml [options]

Options:
  -o, --out <path>       Output PNG (default: <input>.png, or osprey-code.png)
  --theme <name>         Shiki theme (default: one-dark-pro)
  --font <family>        Monospace family resolvable by the OS (default: Menlo)
  --font-size <px>       Font size (default: 15)
  --scale <n>            Retina multiplier (default: 2)
  --padding <px>         Backdrop margin around the window (default: 56)
  --radius <px>          Window corner radius (default: 12)
  --bg <spec>            "#rrggbb" solid, "grad:#a,#b" gradient, or "transparent"
  --transparent          Transparent backdrop (alpha PNG)
  --line-numbers         Show a line-number gutter
  --title <text>         Title-bar caption (default: filename)
  --no-title             Hide the title caption
  --flavor <name>        Force osprey | osprey-ml (default: from extension)
  -h, --help             Show this help

Themes worth trying: one-dark-pro, github-dark, dracula, night-owl,
catppuccin-mocha, vitesse-dark, material-theme-ocean.`;

function flavorFor(opts) {
  if (opts.flavor) return opts.flavor;
  if (opts.input && opts.input.endsWith(".ospml")) return "osprey-ml";
  return "osprey";
}

function loadGrammar(id) {
  const grammar = JSON.parse(readFileSync(GRAMMARS[id], "utf8"));
  grammar.name = id; // Shiki registers/looks a language up by `name`
  return grammar;
}

async function highlight(code, flavor, theme) {
  const highlighter = await createHighlighter({
    themes: [theme],
    langs: [loadGrammar("osprey"), loadGrammar("osprey-ml")],
  });
  const result = highlighter.codeToTokens(code, { lang: flavor, theme });
  const themeData = highlighter.getTheme(theme);
  return { lines: result.tokens, fg: result.fg, bg: result.bg, themeData };
}

function tokenSpan(token, defaultFg) {
  const content = xmlEscape(expandTabs(token.content));
  if (content.length === 0) return "";
  const fill = token.color || defaultFg;
  const style = token.fontStyle || 0;
  const attrs = [`fill="${fill}"`];
  if (style & FONT_STYLE.italic) attrs.push(`font-style="italic"`);
  if (style & FONT_STYLE.bold) attrs.push(`font-weight="bold"`);
  if (style & FONT_STYLE.underline) attrs.push(`text-decoration="underline"`);
  return `<tspan ${attrs.join(" ")}>${content}</tspan>`;
}

function lineWidthCols(line) {
  return line.reduce((n, t) => n + expandTabs(t.content).length, 0);
}

function metrics(lines, opts) {
  const charW = opts.fontSize * opts.charAspect;
  const lineH = opts.fontSize * opts.lineHeightRatio;
  const maxCols = lines.reduce((m, l) => Math.max(m, lineWidthCols(l)), 0);
  const gutter = opts.lineNumbers
    ? String(lines.length).length * charW + charW * 2
    : 0;
  const codeW = maxCols * charW;
  const windowW = CODE_PAD.left + gutter + codeW + CODE_PAD.right;
  const windowH =
    TITLE_BAR_H + CODE_PAD.top + lines.length * lineH + CODE_PAD.bottom;
  return { charW, lineH, gutter, windowW, windowH };
}

function backdropDefs(opts, svgW, svgH) {
  if (opts.background === "transparent") return { defs: "", rect: "" };
  if (opts.background.startsWith("grad:")) {
    const [a, b] = opts.background.slice(5).split(",");
    const defs =
      `<linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">` +
      `<stop offset="0" stop-color="${a}"/><stop offset="1" stop-color="${b}"/>` +
      `</linearGradient>`;
    return { defs, rect: `<rect width="${svgW}" height="${svgH}" fill="url(#bg)"/>` };
  }
  return {
    defs: "",
    rect: `<rect width="${svgW}" height="${svgH}" fill="${opts.background}"/>`,
  };
}

function chrome(opts, m, title) {
  const { padding: p, radius: r } = opts;
  const dots = DOT.colours
    .map((c, i) => {
      const cx = p + DOT.firstX + i * DOT.gap;
      const cy = p + TITLE_BAR_H / 2;
      return `<circle cx="${cx}" cy="${cy}" r="${DOT.r}" fill="${c}"/>`;
    })
    .join("");
  const caption =
    opts.showTitle && title
      ? `<text x="${p + m.windowW / 2}" y="${p + TITLE_BAR_H / 2}" ` +
        `text-anchor="middle" dominant-baseline="central" ` +
        `font-family="${opts.fontFamily}" font-size="${opts.fontSize * 0.82}" ` +
        `fill="#ffffff" fill-opacity="0.45">${xmlEscape(title)}</text>`
      : "";
  return dots + caption;
}

function codeText(lines, opts, m) {
  const { padding: p } = opts;
  const codeLeft = p + CODE_PAD.left + m.gutter;
  const top = p + TITLE_BAR_H + CODE_PAD.top;
  const ascent = opts.fontSize * 0.8;
  const numFill = "#ffffff";
  return lines
    .map((line, i) => {
      const y = top + i * m.lineH + ascent;
      const gutter = opts.lineNumbers
        ? `<text x="${p + CODE_PAD.left + m.gutter - opts.charAspect * opts.fontSize}" ` +
          `y="${y}" text-anchor="end" fill="${numFill}" fill-opacity="0.28">${i + 1}</text>`
        : "";
      const spans = line.map((t) => tokenSpan(t, opts.fg)).join("");
      const row = `<text x="${codeLeft}" y="${y}" xml:space="preserve">${spans}</text>`;
      return gutter + row;
    })
    .join("");
}

function buildSvg(data, opts, title) {
  const m = metrics(data.lines, opts);
  const p = opts.padding;
  const svgW = m.windowW + p * 2;
  const svgH = m.windowH + p * 2;
  const back = backdropDefs(opts, svgW, svgH);
  const shadow =
    `<filter id="shadow" x="-40%" y="-40%" width="180%" height="180%">` +
    `<feGaussianBlur stdDeviation="${SHADOW.blur}"/></filter>`;
  const shadowRect =
    `<rect x="${p}" y="${p + SHADOW.dy}" width="${m.windowW}" height="${m.windowH}" ` +
    `rx="${opts.radius}" fill="#000000" fill-opacity="${SHADOW.opacity}" filter="url(#shadow)"/>`;
  const windowRect =
    `<rect x="${p}" y="${p}" width="${m.windowW}" height="${m.windowH}" ` +
    `rx="${opts.radius}" fill="${data.bg}"/>`;
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" width="${svgW}" height="${svgH}" ` +
    `viewBox="0 0 ${svgW} ${svgH}" font-family="${opts.fontFamily}, monospace" ` +
    `font-size="${opts.fontSize}">` +
    `<defs>${back.defs}${shadow}</defs>` +
    back.rect +
    shadowRect +
    windowRect +
    chrome(opts, m, title) +
    `<g fill="${opts.fg}">${codeText(data.lines, opts, m)}</g>` +
    `</svg>`
  );
}

function rasterize(svg, opts) {
  const resvg = new Resvg(svg, {
    fitTo: { mode: "zoom", value: opts.scale },
    font: { loadSystemFonts: true, defaultFontFamily: opts.fontFamily },
  });
  return resvg.render().asPng();
}

function outputPath(opts) {
  if (opts.out) return opts.out;
  if (opts.input) return `${opts.input.replace(/\.(osp|ospml)$/, "")}.png`;
  return "osprey-code.png";
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  if (opts.help) return void console.log(HELP);

  const code = (
    opts.input ? readFileSync(opts.input, "utf8") : readFileSync(0, "utf8")
  ).replace(/\r\n/g, "\n").replace(/\n$/, "");
  if (code.trim().length === 0) throw new Error("No Osprey source provided.");

  const flavor = flavorFor(opts);
  const data = await highlight(code, flavor, opts.theme);
  opts.fg = data.fg;
  const title = opts.title ?? (opts.input ? basename(opts.input) : undefined);

  const svg = buildSvg(data, opts, title);
  const png = rasterize(svg, opts);
  const out = outputPath(opts);
  writeFileSync(out, png);
  const { width, height } = new Resvg(svg, {
    fitTo: { mode: "zoom", value: opts.scale },
  }).render();
  console.log(`Wrote ${out} (${width}×${height}px, ${flavor}, ${opts.theme})`);
}

main().catch((err) => {
  console.error(`osprey-carbon: ${err.message}`);
  process.exit(1);
});

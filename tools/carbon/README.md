# osprey-carbon

Carbon-style PNG screenshots of Osprey source — for docs, blog posts, and
social cards.

Highlighting is **not** reinvented: the tool loads the repository's real
TextMate grammars
([`vscode-extension/syntaxes/osprey.tmGrammar.json`](../../vscode-extension/syntaxes/osprey.tmGrammar.json)
and `osprey-ml.tmLanguage.json`) through [Shiki](https://shiki.style), so the
colours match VS Code exactly. The framed result is rasterised to PNG with
[`@resvg/resvg-js`](https://github.com/thx/resvg-js) — no headless browser.

## Setup

```bash
cd tools/carbon
npm install
```

## Usage

```bash
# Flavor is chosen from the extension: .osp → osprey, .ospml → osprey-ml
node render.mjs ../../examples/projects/modules/client/src/main.ospml

# Pick a theme and write somewhere specific
node render.mjs path/to/file.osp --theme github-dark -o out.png

# From stdin (declare the flavor yourself)
cat snippet.ospml | node render.mjs --flavor osprey-ml -o snippet.png
```

## Options

| Flag | Default | Meaning |
|------|---------|---------|
| `-o, --out <path>` | `<input>.png` | Output PNG path |
| `--theme <name>` | `one-dark-pro` | Any Shiki theme |
| `--font <family>` | `Menlo` | OS-resolvable monospace family |
| `--font-size <px>` | `15` | Code font size |
| `--scale <n>` | `2` | Retina multiplier |
| `--padding <px>` | `56` | Backdrop margin around the window |
| `--radius <px>` | `12` | Window corner radius |
| `--bg <spec>` | `grad:#4568dc,#b06ab3` | `#rrggbb`, `grad:#a,#b`, or `transparent` |
| `--transparent` | — | Alpha backdrop |
| `--line-numbers` | off | Show a line-number gutter |
| `--title <text>` | filename | Title-bar caption |
| `--no-title` | — | Hide the caption |
| `--flavor <name>` | from extension | Force `osprey` / `osprey-ml` |

Nice themes: `one-dark-pro`, `github-dark`, `dracula`, `night-owl`,
`catppuccin-mocha`, `vitesse-dark`, `material-theme-ocean`.

## Notes

- Long single-expression lines (common in `.ospml` view code) produce very wide
  images — the canvas grows to fit the longest line, exactly like Carbon.
- Fonts resolve from installed system fonts. On macOS `Menlo` is always present;
  pass `--font "JetBrains Mono"` etc. if you have it installed.

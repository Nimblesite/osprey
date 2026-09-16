// Public-CLI fixtures for [DOC-EXPORT-HTML], [DOC-EXPORT-PAGES], [DOC-EXPORT-CSS].
const fs = require("node:fs");
const path = require("node:path");
const { pathToFileURL } = require("node:url");
const { execFileSync } = require("node:child_process");

const REPO = path.resolve(__dirname, "../..");
const SITE = path.resolve(__dirname, "../_site");
const THEMES = {
  osprey: ["rgb(245, 248, 252)", "#0a58ca"],
  midnight: ["rgb(7, 13, 31)", "#77d7f4"],
  paper: ["rgb(251, 248, 241)", "#9a3412"],
};
const SOURCE = `//! Example library.
module A {
    /// Return the label from [helper].
    export fn read() = helper()
    /// The local label.
    export fn helper() = "local"
    fn privateDetail() = "private"
}
module B {
    /// An unrelated helper with the same name.
    export fn helper() = "other"
}
`;
const CODE_SAMPLES = {
  osprey: 'fn label() = "</script><img src=x onerror=alert(1)>"\n// Keep Ω, &, < and > exactly.\n',
  'osprey-ml': 'label = "<b>plain text</b>"\n(** Keep spacing and Unicode: λ *)\n',
};
const GUIDE = `# Getting started

| Feature | Status |
| --- | --- |
| Guides | Ready |

## Examples

- [x] Documented

Use **strong**, ~~removed~~, and \`code\`.

<script>window.docInjected = true</script>

## Examples

A repeated heading.

## Examples-2

A heading that resembles an allocated suffix.

## Content

An ordinary heading, separate from the page landmark.

## Safe attributes {onclick="window.docInjected=true"}

Attributes must not install event handlers.
`;

function write(file, text) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, text);
}

function generate(root, name, theme, styles = []) {
  execFileSync(path.join(REPO, "target/release/osprey"), [
    "--docs", path.join(root, "source.osp"), "--docs-format", "html",
    "--docs-dir", path.join(root, name), "--docs-theme", theme,
    "--docs-page", path.join(root, "guides"),
    ...styles.flatMap((style) => ["--docs-css", path.join(root, style)]),
  ], { cwd: REPO, stdio: "pipe" });
}

function fixture() {
  fs.mkdirSync(SITE, { recursive: true });
  const root = fs.mkdtempSync(path.join(SITE, "docs-contract-"));
  write(path.join(root, "source.osp"), SOURCE);
  write(path.join(root, "guides/Introduction.md"), "~~~~osprey\n# Not the guide title\n~~~~\n\n# Introduction\n\n[Examples](deep/Get%20Started.md#examples)\n");
  const samples = Object.entries(CODE_SAMPLES).map(([language, source]) => `\n\`\`\`${language}\n${source}\`\`\`\n`).join("");
  write(path.join(root, "guides/deep/Get Started.md"), GUIDE + samples);
  write(path.join(root, "brand.css"), ":root { --accent: rgb(255, 0, 128); }");
  write(path.join(root, "override.css"), "body { background: rgb(1, 2, 3); }");
  for (const theme of Object.keys(THEMES)) generate(root, theme, theme);
  generate(root, "custom", "osprey", ["brand.css", "override.css"]);
  return { root, prefix: `/${path.basename(root)}`, file: (site, page) => pathToFileURL(path.join(root, site, page)).href };
}

module.exports = { fixture, THEMES, CODE_SAMPLES };

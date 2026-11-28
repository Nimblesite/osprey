// [EDITOR-VSCODE-THEME]: the Osprey Midnight theme is the design system's tokens, the same ones the website's
// code blocks read, and it colours every scope the Osprey grammars emit.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { colourTokens, theme } from "./sync-theme.mjs";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const css = read("../../website/src/css/variables.css");
const tokens = colourTokens(css);
const built = theme(tokens);
const scopeColour = (scope) => {
  const rules = built.tokenColors.flatMap((rule) => rule.scope.map((prefix) => [prefix, rule.settings]));
  const matches = rules.filter(([prefix]) => scope === prefix || scope.startsWith(`${prefix}.`));
  return matches.sort(([a], [b]) => b.length - a.length)[0]?.[1];
};

test("the workbench wears the design system's palette, role by role", () => {
  assert.equal(built.name, "Osprey Midnight");
  assert.equal(built.type, "dark");
  for (const [key, role] of [
    ["editor.background", "surface"], ["sideBar.background", "surface-lowest"], ["editorWidget.background", "surface-container"],
    ["editor.foreground", "on-surface"], ["descriptionForeground", "on-surface-variant"], ["focusBorder", "primary-container"],
    ["textLink.foreground", "primary"], ["button.foreground", "on-primary"], ["panel.border", "outline-variant"],
    ["errorForeground", "error"], ["editorWarning.foreground", "tertiary-container"], ["editor.selectionBackground", "secondary-container"],
  ]) assert.equal(built.colors[key], tokens.get(role), `${key} is --${role}`);
  assert.equal(built.colors["editor.background"], "#0c1325", "the midnight canvas of docs/designs/design.md");
  assert.equal(built.colors.focusBorder, "#77d7f4", "the luminous cyan accent");
});

test("the Settings editor's own colours come from the same tokens", () => {
  const settings = Object.keys(built.colors).filter((key) => key.startsWith("settings."));
  assert.ok(settings.length >= 15, settings.join(", "));
  for (const [key, role] of [
    ["settings.headerForeground", "primary"], ["settings.modifiedItemIndicator", "primary-container"],
    ["settings.focusedRowBackground", "surface"], ["settings.rowHoverBackground", "surface-low"],
    ["settings.textInputBackground", "surface-container"], ["settings.textInputForeground", "on-surface"],
    ["settings.textInputBorder", "outline-variant"], ["settings.checkboxBackground", "surface-container"],
    ["settings.dropdownBorder", "outline-variant"], ["settings.numberInputForeground", "on-surface"],
  ]) assert.equal(built.colors[key], tokens.get(role), `${key} is --${role}`);
});

test("every colour is a design token and no key is set twice", () => {
  const palette = new Set(tokens.values());
  for (const [key, value] of Object.entries(built.colors)) assert.ok(palette.has(value), `${key} ${value} is a token`);
  for (const rule of built.tokenColors) assert.ok(palette.has(rule.settings.foreground), `${rule.scope} is a token`);
  const source = read("./sync-theme.mjs");
  const keys = [...source.matchAll(/"([a-zA-Z]+\.[a-zA-Z0-9.]+)"/gu)].map(([, key]) => key).filter((key) => key in built.colors);
  assert.equal(new Set(keys).size, keys.length, "a workbench key belongs to one role");
  const scopes = built.tokenColors.flatMap((rule) => rule.scope);
  assert.equal(new Set(scopes).size, scopes.length, "a scope belongs to one syntax token");
});

test("code in the editor is coloured like code on the website", () => {
  const syntax = read("../../website/src/css/syntax.css");
  assert.doesNotMatch(syntax, /#[0-9a-fA-F]{3,8}\b/u, "syntax.css reads the tokens, never its own hex");
  for (const [selector, role, scope] of [
    [".token.comment", "syntax-comment", "comment.line.double-slash.osprey"],
    [".token.keyword", "syntax-keyword", "keyword.control.osprey"],
    [".token.function", "syntax-function", "entity.name.function.effect.osprey"],
    [".token.string", "syntax-string", "string.quoted.double.osprey"],
    [".token.number", "syntax-number", "constant.numeric.integer.osprey"],
    [".token.type", "syntax-type", "entity.name.type.module.osprey"],
    [".token.operator", "syntax-operator", "keyword.operator.logical.osprey"],
  ]) {
    assert.match(syntax, new RegExp(`\\${selector}[^{]*\\{[^}]*var\\(--${role}\\)`, "u"), `${selector} uses --${role}`);
    assert.equal(scopeColour(scope)?.foreground, tokens.get(role), `${scope} is --${role}`);
  }
  assert.equal(scopeColour("comment.block.osprey")?.fontStyle, "italic");
  assert.equal(scopeColour("storage.modifier.static.osprey")?.foreground, tokens.get("syntax-keyword"));
  assert.equal(scopeColour("punctuation.definition.template-expression.begin.osprey")?.foreground, tokens.get("syntax-operator"));
  assert.equal(scopeColour("punctuation.accessor.osprey")?.foreground, tokens.get("on-surface-variant"));
});

test("every scope the Osprey grammars emit has a colour", () => {
  for (const grammar of ["osprey.tmGrammar.json", "osprey-ml.tmLanguage.json"]) {
    const scopes = [...read(`../syntaxes/${grammar}`).matchAll(/"name":\s*"([^"]+)"/gu)].flatMap(([, names]) => names.split(/\s+/u))
      .filter((scope) => scope.includes(".") && !scope.startsWith("meta.") && !scope.startsWith("source."));
    assert.ok(scopes.length > 20, `${grammar} emits scopes`);
    const uncoloured = [...new Set(scopes.filter((scope) => !scopeColour(scope)))];
    assert.deepEqual(uncoloured, [], `${grammar}: ${uncoloured.join(", ")}`);
  }
});

test("the extension contributes the file the build writes", () => {
  const manifest = JSON.parse(read("../package.json"));
  assert.deepEqual(manifest.contributes.themes, [{ label: "Osprey Midnight", uiTheme: "vs-dark", path: "./themes/osprey-midnight.json" }]);
  assert.match(manifest.scripts.precompile, /node scripts\/sync-theme\.mjs/u);
  assert.match(read("../.gitignore"), /^themes\/$/mu, "the generated theme is never committed");
});

test("a palette missing a role fails the build by name", () => {
  for (const role of ["surface", "primary-container", "syntax-keyword", "error"]) {
    const without = new Map(tokens);
    without.delete(role);
    assert.throws(() => theme(without), new RegExp(`--${role}\\b`, "u"));
  }
  assert.throws(() => theme(new Map()), /--surface, --surface-lowest/u);
  assert.equal(colourTokens(":root {\n  --a: #ABCDEF;\n  --b: rgba(0,0,0,1);\n  --c:#123456 ;\n}").size, 2);
  assert.equal(colourTokens(":root { --a: #abcdef; }").get("a"), undefined, "one token per line, as variables.css writes them");
});

// Implements [EDITOR-VSCODE-THEME]: the Osprey Midnight colour theme, built from the design system's one set
// of tokens (src/toolchain/website/src/css/variables.css, the palette in docs/designs/design.md). VS Code reads
// a theme from a JSON file inside the package, so it is generated at build time (precompile) and gitignored,
// never a second copy of the palette.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";

const TOKENS = new URL("../../website/src/css/variables.css", import.meta.url);
const THEME = new URL("../themes/osprey-midnight.json", import.meta.url);

// Workbench colour keys by design role: canvas and inset surfaces, text, the cyan accent, fine outlines.
const WORKBENCH = {
  surface: ["editor.background", "tab.activeBackground", "breadcrumb.background", "editorGutter.background", "settings.focusedRowBackground"],
  "surface-lowest": ["sideBar.background", "activityBar.background", "statusBar.background", "statusBar.noFolderBackground", "titleBar.activeBackground", "titleBar.inactiveBackground", "editorGroupHeader.tabsBackground", "tab.inactiveBackground", "panel.background", "terminal.background", "editorPane.background", "sideBarSectionHeader.background"],
  "surface-low": ["editor.lineHighlightBackground", "list.inactiveSelectionBackground", "settings.rowHoverBackground"],
  "surface-container": ["editorWidget.background", "editorSuggestWidget.background", "editorHoverWidget.background", "input.background", "dropdown.background", "quickInput.background", "menu.background", "notifications.background", "peekViewEditor.background", "settings.textInputBackground", "settings.numberInputBackground", "settings.dropdownBackground", "settings.checkboxBackground"],
  "surface-high": ["list.hoverBackground", "editor.wordHighlightBackground", "editorBracketMatch.background", "toolbar.hoverBackground"],
  "surface-highest": ["scrollbarSlider.background", "editorIndentGuide.activeBackground1"],
  "on-surface": ["editor.foreground", "foreground", "tab.activeForeground", "titleBar.activeForeground", "input.foreground", "dropdown.foreground", "list.activeSelectionForeground", "editorLineNumber.activeForeground", "menu.foreground", "settings.textInputForeground", "settings.numberInputForeground", "settings.dropdownForeground", "settings.checkboxForeground", "terminal.foreground"],
  "on-surface-variant": ["sideBar.foreground", "statusBar.foreground", "descriptionForeground", "tab.inactiveForeground", "titleBar.inactiveForeground", "panelTitle.inactiveForeground", "activityBar.inactiveForeground", "input.placeholderForeground"],
  "primary-container": ["focusBorder", "editorCursor.foreground", "activityBar.foreground", "activityBar.activeBorder", "activityBarBadge.background", "tab.activeBorderTop", "panelTitle.activeBorder", "button.background", "progressBar.background", "settings.modifiedItemIndicator", "textLink.activeForeground", "editorLink.activeForeground", "statusBarItem.remoteBackground", "badge.background"],
  primary: ["textLink.foreground", "button.hoverBackground", "settings.headerForeground", "panelTitle.activeForeground", "pickerGroup.foreground"],
  "on-primary": ["button.foreground", "activityBarBadge.foreground", "statusBarItem.remoteForeground", "badge.foreground"],
  "secondary-container": ["editor.selectionBackground", "list.activeSelectionBackground", "quickInputList.focusBackground", "button.secondaryBackground", "editor.findMatchHighlightBackground", "terminal.selectionBackground"],
  "on-secondary-container": ["button.secondaryForeground"],
  "outline-variant": ["editorIndentGuide.background1", "editorGroup.border", "panel.border", "sideBar.border", "tab.border", "input.border", "dropdown.border", "editorWidget.border", "statusBar.border", "titleBar.border", "settings.textInputBorder", "settings.numberInputBorder", "settings.dropdownBorder", "settings.checkboxBorder", "editorRuler.foreground"],
  outline: ["editorLineNumber.foreground", "editorWhitespace.foreground", "icon.foreground"],
  "tertiary-container": ["editorWarning.foreground", "list.warningForeground", "editorOverviewRuler.warningForeground"],
  error: ["errorForeground", "editorError.foreground", "list.errorForeground", "editorOverviewRuler.errorForeground", "inputValidation.errorBorder"],
};

// TextMate scopes by syntax token, matching the website's code blocks (syntax.css reads the same tokens).
const SYNTAX = [
  ["syntax-comment", ["comment", "punctuation.definition.comment"], "italic"],
  ["syntax-keyword", ["keyword", "storage", "keyword.declaration"], "bold"],
  ["syntax-function", ["entity.name.function", "support.function", "meta.function-call"]],
  ["syntax-string", ["string", "punctuation.definition.string"]],
  ["syntax-number", ["constant.numeric", "constant.language", "constant.character"]],
  ["syntax-type", ["entity.name.type", "support.type", "entity.name.namespace", "entity.other.inherited-class"]],
  ["syntax-operator", ["keyword.operator", "punctuation.definition.template-expression"]],
  ["on-surface-variant", ["punctuation"]],
  ["on-surface", ["variable", "variable.parameter", "variable.other.member"]],
  ["error", ["invalid"]],
];

/** Every `--name: #rrggbb;` colour token in the stylesheet, by name. */
export function colourTokens(css) {
  return new Map([...css.matchAll(/^\s*--([a-z][a-z0-9-]*):\s*(#[0-9a-fA-F]{6})\s*;/gmu)].map(([, name, value]) => [name, value.toLowerCase()]));
}

/** The VS Code theme for the tokens; throws naming every role the stylesheet no longer defines. */
export function theme(tokens) {
  const roles = [...Object.keys(WORKBENCH), ...SYNTAX.map(([role]) => role)];
  const missing = [...new Set(roles.filter((role) => !tokens.has(role)))];
  if (missing.length > 0) throw new Error(`sync-theme: the design tokens define no ${missing.map((role) => `--${role}`).join(", ")}`);
  return {
    name: "Osprey Midnight",
    type: "dark",
    semanticHighlighting: true,
    colors: Object.fromEntries(Object.entries(WORKBENCH).flatMap(([role, keys]) => keys.map((key) => [key, tokens.get(role)]))),
    tokenColors: SYNTAX.map(([role, scope, fontStyle]) => ({ scope, settings: { foreground: tokens.get(role), ...(fontStyle ? { fontStyle } : {}) } })),
  };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const css = readFileSync(TOKENS, "utf8");
  mkdirSync(dirname(fileURLToPath(THEME)), { recursive: true });
  writeFileSync(THEME, `${JSON.stringify(theme(colourTokens(css)), null, 2)}\n`);
  console.log(`sync-theme: ${fileURLToPath(TOKENS)} -> ${fileURLToPath(THEME)}`);
}

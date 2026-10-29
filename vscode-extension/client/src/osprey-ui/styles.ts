// The package workspace's complete stylesheet. It follows the editor theme for
// surfaces, text and borders; the navy/cyan identity lives in the brand
// lockup and accents so the page belongs in any VS Code colour theme.

export const WORKSPACE_STYLES = `
:root {
  color-scheme: light dark;
  --osp-canvas: var(--vscode-editor-background, light-dark(#f6f8fb, #0c1325));
  --osp-surface: var(--vscode-sideBar-background, light-dark(#ffffff, #161d2e));
  --osp-raised: var(--vscode-editorWidget-background, light-dark(#ffffff, #1d2538));
  --osp-text: var(--vscode-foreground, light-dark(#17223a, #dce1fb));
  --osp-muted: var(--vscode-descriptionForeground, light-dark(#5b6884, #9aa6c8));
  --osp-border: var(--vscode-panel-border, light-dark(#dce3ef, #2b354c));
  --osp-accent: var(--vscode-textLink-foreground, light-dark(#006b86, #77d7f4));
  --osp-accent-strong: var(--osp-brand-accent, light-dark(#006b86, #22b8cf));
  --osp-danger: var(--vscode-errorForeground, #d9534f);
  --osp-good: var(--vscode-testing-iconPassed, #3fb950);
  --osp-caution: var(--vscode-editorWarning-foreground, #d29922);
  --osp-radius: 12px;
  --osp-motion: 160ms;
  --osp-aside-width: 340px;
}
* { box-sizing: border-box; }
html, body { height: 100%; }
body {
  margin: 0;
  background: var(--osp-canvas);
  color: var(--osp-text);
  font-family: var(--vscode-font-family, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif);
  font-size: var(--vscode-font-size, 13px);
  line-height: 1.45;
  overflow: hidden;
}
button, input, textarea, select { font: inherit; color: inherit; }
.osp-math { font-family: "STIX Two Math", "Latin Modern Math", math; font-size: 1.25em; line-height: normal; max-width: 100%; }
.osp-editor { min-width: 0; }
.osp-editor-label { display: block; color: var(--osp-muted); margin-bottom: 12px; }
.osp-editor-surface { position: relative; min-height: 320px; }
.osp-editor-mirror, .osp-editor textarea { margin: 0; padding: 12px; width: 100%; min-height: 320px; font: 20px/1.7 "STIX Two Text", Georgia, serif; letter-spacing: normal; white-space: pre-wrap; overflow-wrap: break-word; tab-size: 4; border: 0; }
.osp-editor-mirror { position: absolute; inset: 0; pointer-events: none; overflow: hidden; }
.osp-editor-line { display: block; }
.osp-editor-underlined { text-decoration: underline wavy; text-underline-offset: 5px; }
.osp-editor textarea { display: block; position: relative; resize: vertical; background: transparent; color: transparent; caret-color: var(--osp-text); }
.osp-editor textarea:focus-visible { outline-offset: -2px; }
.osp-editor-feedback { margin: 8px 0 0; padding: 0; list-style: none; font-size: 14px; }
.osp-editor-feedback:empty { display: none; }
.osp-editor-feedback li { margin-top: 8px; }
.osp-board { position: relative; width: 100%; height: min(70vh, 900px); min-height: 320px; overflow: hidden; background: var(--osp-board, #202723); }
.osp-board canvas { display: block; width: 100%; height: 100%; touch-action: none; color: var(--osp-chalk, #ECEDE5); cursor: crosshair; }
.osp-media-image, .osp-media-video { display: block; width: 100%; max-width: 100%; height: auto; border-radius: 4px; }
.osp-media-video { background: var(--osp-board, #202723); }
@media (forced-colors: active) { .osp-editor-mirror { display: none; } .osp-editor textarea { color: CanvasText; } }
button { cursor: pointer; }
button:disabled { cursor: default; opacity: .48; }
:focus-visible { outline: 2px solid var(--osp-accent); outline-offset: 2px; }
.osp-shell { height: 100vh; display: flex; flex-direction: column; min-height: 0; }
#osprey-root { flex: 1 1 auto; min-height: 0; overflow: hidden; }
.osp-root { height: 100%; }
.osp-region { min-height: 0; }
.osp-screen { display: flex; flex-direction: column; height: 100%; min-height: 0; }
.osp-screen-header {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 16px;
  padding: 14px 24px;
  border-bottom: 1px solid var(--osp-border);
  background:
    linear-gradient(100deg, color-mix(in srgb, var(--osp-brand-canvas, #0b1b33) 16%, transparent), transparent 46%),
    linear-gradient(0deg, var(--osp-surface), var(--osp-surface));
}
.osp-brand-lockup { display: flex; align-items: center; gap: 10px; min-width: 88px; }
.osp-brand { width: 28px; height: 28px; border-radius: 7px; object-fit: contain; }
.osp-brand-word { font-weight: 700; letter-spacing: .02em; color: var(--osp-accent-strong); }
.osp-brand-name { font-weight: 700; letter-spacing: .035em; text-transform: uppercase; font-size: 10px; color: var(--osp-muted); }
.osp-screen-titles h1 { margin: 0; font-size: 19px; line-height: 1.2; letter-spacing: -.01em; }
.osp-screen-titles p { margin: 3px 0 0; color: var(--osp-muted); font-size: 12px; }
.osp-toolbar { display: flex; align-items: center; justify-content: flex-end; gap: 8px; flex-wrap: wrap; }
.osp-screen-body {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 12px 16px 16px;
  overflow: auto;
}
.osp-screen-body > .osp-split, .osp-screen-body > .osp-aside { flex: 1; min-height: 0; }
.osp-stack { display: flex; gap: 12px; min-width: 0; }
.osp-axis-vertical { flex-direction: column; }
.osp-axis-horizontal { flex-direction: row; align-items: center; flex-wrap: wrap; }
.osp-split { display: flex; gap: 18px; min-height: 0; min-width: 0; }
.osp-pane { flex: 1 1 0; min-width: 0; min-height: 0; overflow: auto; padding-right: 2px; }
.osp-aside { display: flex; gap: 0; min-height: 0; min-width: 0; }
.osp-aside-pane { flex: 1 1 0; min-width: 0; min-height: 0; overflow: auto; }
.osp-aside-column { flex: 0 0 var(--osp-aside-width); width: var(--osp-aside-width); min-width: 0; overflow: auto; }
.osp-side-leading .osp-aside-column { order: 0; }
.osp-side-trailing .osp-aside-column { order: 2; }
.osp-splitter { flex: 0 0 9px; width: 9px; display: flex; align-items: center; justify-content: center; cursor: col-resize; border-radius: 6px; }
.osp-splitter span { width: 2px; height: 42px; border-radius: 2px; background: var(--osp-border); transition: background var(--osp-motion); }
.osp-splitter:hover span, .osp-splitter:focus-visible span, .osp-splitter.osp-dragging span { background: var(--osp-accent); }
.osp-section {
  background: var(--osp-surface);
  border: 1px solid var(--osp-border);
  border-radius: var(--osp-radius);
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 0;
}
.osp-section-title { font-size: 11px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; color: var(--osp-muted); }
.osp-section-body { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
.osp-section-footer { color: var(--osp-muted); font-size: 11px; border-top: 1px solid var(--osp-border); padding-top: 10px; }
.osp-text { margin: 0; }
.osp-role-display { font-size: 30px; line-height: 1.1; font-weight: 700; letter-spacing: -.02em; }
.osp-role-large { font-size: 22px; line-height: 1.2; font-weight: 650; letter-spacing: -.01em; }
.osp-role-title { font-size: 18px; font-weight: 650; }
.osp-role-heading { font-size: 15px; font-weight: 650; }
.osp-role-lead { font-size: 15px; color: var(--osp-muted); }
.osp-role-headline { font-size: 14px; font-weight: 600; }
.osp-role-callout { font-size: 13px; font-weight: 550; }
.osp-role-caption { font-size: 11.5px; color: var(--osp-muted); }
.osp-role-eyebrow { font-size: 10px; font-weight: 700; letter-spacing: .1em; text-transform: uppercase; }
.osp-role-mono { font-family: var(--vscode-editor-font-family, "SFMono-Regular", Consolas, monospace); font-size: 12px; }
.osp-tone-secondary { color: var(--osp-muted); }
.osp-tone-accent { color: var(--osp-accent); }
.osp-tone-good { color: var(--osp-good); }
.osp-tone-caution { color: var(--osp-caution); }
.osp-tone-danger { color: var(--osp-danger); }
.codicon { flex: 0 0 auto; }
.osp-icon { display: inline-flex; color: var(--osp-muted); }
.osp-label { display: inline-flex; align-items: center; gap: 7px; }
.osp-evidence { display: grid; grid-template-columns: minmax(90px, .5fr) 1fr; gap: 12px; padding: 7px 0; border-bottom: 1px solid var(--osp-border); }
.osp-evidence-label { color: var(--osp-muted); }
.osp-evidence-value { font-family: var(--vscode-editor-font-family, monospace); overflow-wrap: anywhere; }
.osp-badge { display: inline-flex; align-items: center; gap: 6px; width: fit-content; padding: 3px 9px 3px 7px; border-radius: 999px; font-size: 11px; font-weight: 600; border: 1px solid color-mix(in srgb, currentColor 35%, transparent); background: color-mix(in srgb, currentColor 10%, transparent); }
.osp-badge .codicon { font-size: 13px; }
.osp-badge.osp-tone-primary { color: var(--osp-text); }
.osp-button { display: inline-flex; align-items: center; gap: 7px; border: 1px solid var(--osp-border); background: var(--osp-raised); color: var(--osp-text); border-radius: 8px; padding: 7px 12px; min-height: 32px; transition: background var(--osp-motion), border-color var(--osp-motion); }
.osp-button:hover:not(:disabled) { border-color: var(--osp-accent); background: color-mix(in srgb, var(--osp-accent) 8%, var(--osp-raised)); }
.osp-button-prominent { background: var(--osp-accent-strong); border-color: transparent; color: light-dark(#ffffff, #06111f); font-weight: 650; }
.osp-button-prominent:hover:not(:disabled) { background: color-mix(in srgb, var(--osp-brand-accent, #22b8cf) 82%, white); }
.osp-button-destructive { color: var(--osp-danger); border-color: color-mix(in srgb, var(--osp-danger) 55%, transparent); background: transparent; }
.osp-button-destructive:hover:not(:disabled) { background: color-mix(in srgb, var(--osp-danger) 10%, transparent); border-color: var(--osp-danger); }
.osp-button .codicon { font-size: 15px; }
.osp-field { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.osp-field > span { font-size: 11px; font-weight: 650; letter-spacing: .02em; color: var(--osp-muted); }
.osp-field input, .osp-field textarea, .osp-field select {
  width: 100%; border: 1px solid var(--vscode-input-border, var(--osp-border));
  background: var(--vscode-input-background, var(--osp-raised)); color: var(--vscode-input-foreground, var(--osp-text));
  border-radius: 8px; padding: 8px 10px; outline: none; transition: border-color var(--osp-motion), box-shadow var(--osp-motion);
}
.osp-field input:focus, .osp-field textarea:focus, .osp-field select:focus { border-color: var(--osp-accent); box-shadow: 0 0 0 1px var(--osp-accent); }
.osp-field textarea { resize: vertical; min-height: 72px; }
.osp-toggle { display: inline-flex; align-items: center; gap: 10px; border: 0; background: transparent; padding: 5px 0; }
.osp-switch { width: 32px; height: 18px; border-radius: 999px; border: 1px solid var(--osp-border); background: var(--osp-canvas); display: inline-flex; align-items: center; padding: 2px; transition: background var(--osp-motion); }
.osp-switch span { width: 12px; height: 12px; border-radius: 50%; background: var(--osp-muted); transition: transform var(--osp-motion), background var(--osp-motion); }
.osp-toggle.osp-on .osp-switch { background: var(--osp-accent-strong); border-color: transparent; }
.osp-toggle.osp-on .osp-switch span { transform: translateX(14px); background: var(--osp-canvas); }
.osp-rows { display: flex; flex-direction: column; gap: 2px; }
.osp-row-group { font-size: 10px; font-weight: 700; letter-spacing: .09em; text-transform: uppercase; color: var(--osp-muted); padding: 12px 8px 5px; }
.osp-row { display: flex; align-items: flex-start; gap: 10px; width: 100%; text-align: left; border: 1px solid transparent; background: transparent; border-radius: 9px; padding: 9px 10px; }
.osp-row:hover { background: color-mix(in srgb, var(--osp-accent) 7%, transparent); }
.osp-row.osp-selected { background: color-mix(in srgb, var(--osp-accent) 12%, var(--osp-surface)); border-color: color-mix(in srgb, var(--osp-accent) 45%, transparent); }
.osp-row .codicon { margin-top: 2px; color: var(--osp-muted); }
.osp-row-cell { min-width: 0; flex: 1; }
.osp-tabs { display: flex; flex-direction: column; gap: 12px; }
.osp-tablist { display: flex; gap: 4px; border-bottom: 1px solid var(--osp-border); }
.osp-tab { display: inline-flex; align-items: center; gap: 6px; border: 0; border-bottom: 2px solid transparent; background: transparent; color: var(--osp-muted); padding: 8px 11px; margin-bottom: -1px; }
.osp-tab.osp-selected { color: var(--osp-text); border-bottom-color: var(--osp-accent); font-weight: 600; }
.osp-tabbody { min-height: 0; }
.osp-disclosure { border: 1px solid var(--osp-border); border-radius: 10px; background: var(--osp-surface); overflow: hidden; }
.osp-disclosure-head { display: flex; align-items: center; gap: 8px; width: 100%; text-align: left; border: 0; background: transparent; padding: 10px 12px; font-weight: 600; }
.osp-disclosure-arrow { display: inline-flex; transition: transform var(--osp-motion); color: var(--osp-muted); }
.osp-disclosure-arrow.osp-open { transform: rotate(90deg); }
.osp-disclosure-body { padding: 0 12px 12px; display: flex; flex-direction: column; gap: 10px; }
.osp-alert { display: flex; gap: 10px; padding: 10px 12px; border: 1px solid color-mix(in srgb, var(--osp-caution) 45%, transparent); background: color-mix(in srgb, var(--osp-caution) 9%, transparent); border-radius: 9px; }
.osp-alert strong { display: block; }
.osp-alert p { margin: 2px 0 0; color: var(--osp-muted); }
.osp-alert-icon { color: var(--osp-caution); }
.osp-menu { position: fixed; z-index: 20; min-width: 220px; display: flex; flex-direction: column; gap: 2px; padding: 6px; background: var(--vscode-menu-background, var(--osp-raised)); border: 1px solid var(--vscode-menu-border, var(--osp-border)); border-radius: 9px; box-shadow: 0 10px 26px rgba(0, 0, 0, .28); }
.osp-menu[hidden], .osp-disclosure-body[hidden] { display: none; }
.osp-context-body { min-width: 0; }
.osp-context-host { min-width: 0; }
.osp-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; text-align: center; gap: 8px; padding: 44px 24px; border: 1px dashed var(--osp-border); border-radius: var(--osp-radius); background: color-mix(in srgb, var(--osp-surface) 70%, transparent); min-height: 240px; }
.osp-empty-symbol { color: var(--osp-accent); }
.osp-empty-symbol .codicon { font-size: 32px; }
.osp-empty h2 { margin: 4px 0 0; font-size: 16px; }
.osp-empty p { margin: 0; color: var(--osp-muted); max-width: 46ch; }
.osp-link { display: inline-flex; align-items: center; gap: 6px; color: var(--osp-accent); text-decoration: none; }
.osp-link:hover { text-decoration: underline; }
.osp-link-disabled { color: var(--osp-muted); }
.osp-divider { border: 0; border-top: 1px solid var(--osp-border); width: 100%; margin: 2px 0; }
.osp-spacer { flex: 1 1 auto; }
.osp-segments { display: inline-flex; gap: 2px; }
.osp-segment { width: 16px; height: 5px; border-radius: 999px; background: currentColor; }
.osp-segment.osp-tone-primary { color: var(--osp-muted); }
.osp-progress { display: grid; grid-template-columns: 1fr auto; gap: 6px 10px; align-items: center; }
.osp-progress-track { grid-column: 1 / -1; height: 5px; border-radius: 999px; background: var(--osp-border); overflow: hidden; }
.osp-progress-track span { display: block; width: 35%; height: 100%; background: var(--osp-accent); border-radius: inherit; animation: osp-pulse 1.4s ease-in-out infinite alternate; }
.osp-progress-label { font-weight: 600; }
.osp-progress-detail { color: var(--osp-muted); font-size: 11px; }
.osp-swatch { display: inline-flex; align-items: center; gap: 7px; }
.osp-swatch > span:first-child { width: 18px; height: 18px; border-radius: 5px; border: 1px solid var(--osp-border); background: linear-gradient(135deg, var(--osp-swatch-light) 50%, var(--osp-swatch-dark) 50%); }
.osp-slot { display: contents; }
.osp-loading { height: 100%; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 12px; color: var(--osp-muted); }
.osp-spinner { width: 24px; height: 24px; border-radius: 50%; border: 2px solid var(--osp-border); border-top-color: var(--osp-accent); animation: osp-spin .8s linear infinite; }
#osprey-diagnostics { flex: 0 0 auto; margin: 12px 24px 0; padding: 9px 12px; border: 1px solid var(--osp-danger); border-radius: 8px; color: var(--osp-danger); white-space: pre-wrap; overflow-wrap: anywhere; background: color-mix(in srgb, var(--osp-danger) 8%, transparent); }
#osprey-diagnostics[hidden] { display: none; }
@keyframes osp-spin { to { transform: rotate(360deg); } }
@keyframes osp-pulse { from { opacity: .45; } to { opacity: 1; } }
@media (max-width: 900px) {
  .osp-split { flex-direction: column; flex: 0 0 auto !important; }
  .osp-split > .osp-pane { flex: 0 0 auto; overflow: visible; }
  .osp-aside { flex-direction: column; }
  .osp-aside-column { flex-basis: auto; width: 100%; }
  .osp-splitter { display: none; }
  .osp-brand-name { display: none; }
  .osp-brand-lockup { min-width: 0; }
}
@media (prefers-reduced-motion: reduce) {
  *, *::before, *::after { animation-duration: .01ms !important; transition-duration: .01ms !important; }
}
body.vscode-high-contrast .osp-section, body.vscode-high-contrast .osp-button { border-width: 2px; }
body.vscode-high-contrast .osp-row.osp-selected { outline: 1px solid var(--osp-accent); }
`;

// The webview document's structural styles only: every colour, size and space of a control is
// an inline declaration the application's row carries. Keeping this file small keeps the row the
// single source of a control's look.

export const WORKSPACE_STYLES = `
:root { color-scheme: light dark; }
html, body { margin: 0; padding: 0; height: 100%; overflow: hidden;
  font-family: var(--vscode-font-family); font-size: var(--vscode-font-size); color: var(--vscode-foreground);
  background: var(--vscode-editor-background); }
.osp-shell { display: flex; flex-direction: column; height: 100vh; min-height: 0; }
#osprey-diagnostics { padding: 6px 12px; white-space: pre-wrap; color: var(--vscode-errorForeground);
  border-bottom: 1px solid var(--vscode-panel-border); }
#osprey-runtime-status { padding: 4px 12px; font-size: 90%; opacity: 0.8; }
#osprey-root { flex: 1 1 auto; min-height: 0; overflow: auto; display: flex; flex-direction: column; }
.osp-rows { display: flex; flex-direction: column; flex-grow: 1; min-height: 0; }
.osp-control { min-width: 0; }
.osp-control.osp-press, .osp-control.osp-toggle, .osp-control.osp-option { font: inherit; color: inherit; border: 0; }
.osp-control.osp-entry, .osp-control.osp-editor { outline: none; }
.osp-control.osp-entry:focus, .osp-control.osp-editor:focus { outline: 1px solid var(--vscode-focusBorder); }
.osp-led { display: inline-flex; align-items: center; gap: 6px; }
.osp-symbol { display: inline-flex; width: 1em; justify-content: center; }
.osp-switch { display: inline-flex; width: 1.4em; height: 1.4em; align-items: center; justify-content: center;
  border: 1px solid currentColor; border-radius: 3px; }
.osp-switch[data-chosen="true"] { background: currentColor; color: var(--vscode-editor-background); }
.osp-canvas-frame { position: relative; display: flex; min-height: 0; }
.osp-canvas { outline: none; }
.osp-canvas:focus-visible { outline: 1px solid var(--vscode-focusBorder); }
.osp-access { border: 0; background: transparent; }
.osp-loading { display: flex; align-items: center; gap: 10px; padding: 18px; }
.osp-spinner { width: 14px; height: 14px; border: 2px solid currentColor; border-right-color: transparent;
  border-radius: 50%; animation: osp-spin 900ms linear infinite; }
@keyframes osp-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .osp-spinner { animation: none; } }
`;

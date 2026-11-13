// The single, nonce-bound webview document. It loads no remote code, no command URIs and no
// inline event handlers; the module it loads is this extension's own compiled webview, and every
// dynamic value in the markup it draws is escaped by that module or the Node renderer.

import { codiconStyle } from "./icons";
import { WORKSPACE_STYLES, VIEW_STYLES } from "./styles";
import { escapeHtml } from "./html";

export interface WorkspaceDocument {
  readonly nonce: string;
  readonly assetUri: string;
  readonly scriptUri: string;
  readonly cspSource: string;
  readonly loadingText?: string;
  /** The application's own stylesheet, resolved inside its approved asset root. */
  readonly stylesheetUri?: string;
}

function csp(document: WorkspaceDocument): string {
  return [
    "default-src 'none'",
    `img-src ${document.cspSource} data: blob:`,
    `media-src ${document.cspSource} data: blob:`,
    `style-src ${document.cspSource} 'unsafe-inline'`,
    `script-src ${document.cspSource} 'nonce-${document.nonce}'`,
    "connect-src 'none'",
    `font-src ${document.cspSource} data:`,
  ].join("; ");
}

/** The initial document; the webview draws every later frame into `#osprey-root`. */
export function workspaceHtml(document: WorkspaceDocument): string {
  const asset = document.assetUri === "" ? "" : `<link rel="icon" href="${escapeHtml(document.assetUri)}" />`;
  const stylesheet = document.stylesheetUri ? `<link rel="stylesheet" href="${escapeHtml(document.stylesheetUri)}" />` : "";
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta http-equiv="Content-Security-Policy" content="${csp(document)}" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
${asset}
<style>${codiconStyle()}${WORKSPACE_STYLES}${VIEW_STYLES}</style>
${stylesheet}
</head>
<body>
<div class="osp-shell">
<div id="osprey-diagnostics" hidden></div>
<div id="osprey-runtime-status" role="status" hidden>Updating · previous results shown</div>
<div id="osprey-root"><div class="osp-loading"><span class="osp-spinner" aria-hidden="true"></span><p>${escapeHtml(document.loadingText ?? "Loading…")}</p></div></div>
</div>
<script type="module" nonce="${escapeHtml(document.nonce)}" src="${escapeHtml(document.scriptUri)}"></script>
</body>
</html>`;
}

// The single, nonce-bound webview document. It loads no remote code, no
// command URIs and no inline event handlers; all dynamic markup arrives from
// the Node renderer with application strings escaped.

import { codiconStyle } from "./icons";
import { WORKSPACE_STYLES } from "./styles";
import { WEBVIEW_SCRIPT } from "./webview-client";
import { escapeHtml } from "./html";

export interface WorkspaceDocument {
  readonly nonce: string;
  readonly assetUri: string;
  readonly cspSource: string;
  readonly loadingText?: string;
  readonly stylesheetUri?: string;
}

function csp(document: WorkspaceDocument): string {
  return [
    "default-src 'none'",
    `img-src ${document.cspSource} data: blob:`,
    `media-src ${document.cspSource} data: blob:`,
    `style-src ${document.cspSource} 'unsafe-inline'`,
    `script-src 'nonce-${document.nonce}'`,
    "connect-src 'none'",
    `font-src ${document.cspSource} data:`,
  ].join("; ");
}

/** The initial doc; the renderer replaces only `#osprey-root` afterwards. */
export function workspaceHtml(document: WorkspaceDocument): string {
  const asset = document.assetUri === "" ? "" : `<link rel="icon" href="${document.assetUri}" />`;
  const stylesheet = document.stylesheetUri ? `<link rel="stylesheet" href="${escapeHtml(document.stylesheetUri)}" />` : "";
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta http-equiv="Content-Security-Policy" content="${csp(document)}" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
${asset}
<style>${codiconStyle()}${WORKSPACE_STYLES}</style>
${stylesheet}
</head>
<body>
<div class="osp-shell">
<div id="osprey-diagnostics" hidden></div>
<div id="osprey-root"><div class="osp-loading"><span class="osp-spinner" aria-hidden="true"></span><p>${escapeHtml(document.loadingText ?? "Loading your packages…")}</p></div></div>
</div>
<script nonce="${document.nonce}">${WEBVIEW_SCRIPT}</script>
</body>
</html>`;
}

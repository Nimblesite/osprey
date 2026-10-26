// The single, nonce-bound webview document. It loads no remote code, no
// command URIs and no inline event handlers; all dynamic markup arrives from
// the Node renderer with application strings escaped.

import { codiconStyle } from "./icons";
import { WORKSPACE_STYLES } from "./styles";
import { WEBVIEW_SCRIPT } from "./webview-client";

export interface WorkspaceDocument {
  readonly nonce: string;
  readonly assetUri: string;
  readonly cspSource: string;
}

function csp(document: WorkspaceDocument): string {
  return [
    "default-src 'none'",
    `img-src ${document.cspSource} data:`,
    `style-src ${document.cspSource} 'unsafe-inline'`,
    `script-src 'nonce-${document.nonce}'`,
    "connect-src 'none'",
    "font-src data:",
  ].join("; ");
}

/** The initial doc; the renderer replaces only `#osprey-root` afterwards. */
export function workspaceHtml(document: WorkspaceDocument): string {
  const asset = document.assetUri === "" ? "" : `<link rel="icon" href="${document.assetUri}" />`;
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8" />
<meta http-equiv="Content-Security-Policy" content="${csp(document)}" />
<meta name="viewport" content="width=device-width, initial-scale=1.0" />
${asset}
<style>${codiconStyle()}${WORKSPACE_STYLES}</style>
</head>
<body>
<div class="osp-shell">
<div id="osprey-diagnostics" hidden></div>
<div id="osprey-root"><div class="osp-loading"><span class="osp-spinner" aria-hidden="true"></span><p>Loading your packages…</p></div></div>
</div>
<script nonce="${document.nonce}">${WEBVIEW_SCRIPT}</script>
</body>
</html>`;
}

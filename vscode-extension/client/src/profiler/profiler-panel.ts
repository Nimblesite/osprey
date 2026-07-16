// The singleton flame-graph WebviewPanel ([PROF-VSCODE-FLAME]): thin vscode
// shell around flame-html. Click-to-source messages from the canvas/table are
// parsed by the pure `parseSelectMessage` and revealed via `revealSource`.

import * as path from "path";
import * as vscode from "vscode";
import { buildFlameHtml, makeNonce } from "./flame-html";
import type { FlameModel } from "./flame-model";
import type { ProfileSummary } from "./summary";

export interface SelectMessage {
  type: "select";
  file: string;
  line: number;
}

/** Validate a webview message; only well-formed selects navigate. Pure. */
export function parseSelectMessage(message: unknown): SelectMessage | undefined {
  const select = message as SelectMessage;
  const valid =
    typeof select === "object" &&
    select !== null &&
    select.type === "select" &&
    typeof select.file === "string" &&
    select.file !== "" &&
    typeof select.line === "number";
  return valid ? select : undefined;
}

/** Open the clicked frame's source beside the panel, centered on its line. */
export async function revealSource(message: SelectMessage): Promise<void> {
  const document = await vscode.workspace.openTextDocument(message.file);
  const editor = await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
  const line = Math.min(Math.max(message.line - 1, 0), document.lineCount - 1);
  const range = new vscode.Range(line, 0, line, 0);
  editor.revealRange(range, vscode.TextEditorRevealType.InCenter);
  editor.selection = new vscode.Selection(line, 0, line, 0);
}

/** The webview message handler (exported so it is directly testable). */
export async function handlePanelMessage(message: unknown): Promise<void> {
  const select = parseSelectMessage(message);
  if (select === undefined) {
    return;
  }
  try {
    await revealSource(select);
  } catch {
    void vscode.window.showWarningMessage(`Cannot open ${select.file}`);
  }
}

let panel: vscode.WebviewPanel | undefined;

function createPanel(title: string): vscode.WebviewPanel {
  const created = vscode.window.createWebviewPanel(
    "ospreyProfiler",
    title,
    vscode.ViewColumn.Beside,
    { enableScripts: true },
  );
  created.onDidDispose(() => {
    panel = undefined;
  });
  created.webview.onDidReceiveMessage((message) => void handlePanelMessage(message));
  return created;
}

/** Show (or refresh) the singleton profiler panel for a new run. */
export function showFlamePanel(
  model: FlameModel,
  summary: ProfileSummary,
  sourcePath: string,
): vscode.WebviewPanel {
  const title = `Profile: ${path.basename(sourcePath)}`;
  if (panel === undefined) {
    panel = createPanel(title);
  } else {
    panel.title = title;
    panel.reveal(vscode.ViewColumn.Beside, true);
  }
  panel.webview.html = buildFlameHtml({
    nonce: makeNonce(),
    title: path.basename(sourcePath),
    model,
    summary,
  });
  return panel;
}

import { CodeAction, commands, ExtensionContext, Range, TextDocument, window, workspace, WorkspaceEdit } from "vscode";
import { Middleware } from "vscode-languageclient/node";
import { CodeAction as ProtocolAction, CodeActionParams, Command } from "vscode-languageserver-protocol";

export const applyWarningFixCommand = "osprey.applyWarningFix";
interface WarningFix {
  uri: string;
  version: number;
  source: string;
  title: string;
  kind: string;
  range: [number, number, number, number];
  dependencies: { uri: string; version: number; source: string }[];
  edits: { range: [number, number, number, number]; newText: string }[];
}

function openWarningDocuments(): WarningFix["dependencies"] {
  return workspace.textDocuments.filter((document) => ["osprey", "osprey-ml"].includes(document.languageId))
    .map((document) => ({ uri: document.uri.toString(), version: document.version, source: document.getText() }))
    .sort((left, right) => left.uri.localeCompare(right.uri));
}

export type WarningFixRequest = (params: CodeActionParams) => Promise<(ProtocolAction | Command)[] | null>;

function guardedAction(action: CodeAction, document: TextDocument, snapshot: Pick<WarningFix, "version" | "source" | "dependencies" | "range">): CodeAction {
  const { version, source, dependencies, range } = snapshot;
  const data = (action as CodeAction & { data?: { uri?: string; version?: number } }).data;
  const entries = action.edit?.entries();
  if (data?.uri !== document.uri.toString() || data.version !== version || entries?.length !== 1 || entries[0][0].toString() !== data.uri) {
    action.edit = undefined;
    action.disabled = { reason: "The document changed. Request the quick fix again." };
    return action;
  }
  const edits = entries[0][1].map(({ range, newText }) => ({ newText,
    range: [range.start.line, range.start.character, range.end.line, range.end.character] }));
  action.command = { command: applyWarningFixCommand, title: action.title,
    arguments: [{ uri: data.uri, version, source, dependencies, edits, range, title: action.title, kind: action.kind?.value ?? "quickfix" }] };
  action.edit = undefined;
  return action;
}

// vscode-languageclient converts versioned TextDocumentEdits to WorkspaceEdit
// and drops their versions. Keep the source version until the user invokes the
// action, so a cached lightbulb cannot delete text from a newer document.
// Implements [LSP-CODE-ACTIONS-ANNOTATIONS].
export const warningFixMiddleware: Pick<Middleware, "provideCodeActions"> = {
  provideCodeActions: async (document, range, context, token, next) => {
    const snapshot = { version: document.version, source: document.getText(), dependencies: openWarningDocuments(),
      range: [range.start.line, range.start.character, range.end.line, range.end.character] as WarningFix["range"] };
    const actions = await next(document, range, context, token);
    return actions?.map((action) => action instanceof CodeAction &&
      action.diagnostics?.some((diagnostic) => diagnostic.code === "redundant-annotation")
      ? guardedAction(action, document, snapshot) : action);
  },
};

function currentDocument(fix: WarningFix): TextDocument | undefined {
  const document = workspace.textDocuments.find((item) => item.uri.toString() === fix.uri);
  return document?.version === fix.version && document.getText() === fix.source &&
    JSON.stringify(openWarningDocuments()) === JSON.stringify(fix.dependencies) ? document : undefined;
}

function matchesFreshAction(action: ProtocolAction | Command, fix: WarningFix): boolean {
  if (!ProtocolAction.is(action) || action.disabled || action.title !== fix.title || action.kind !== fix.kind ||
    !action.diagnostics?.some((item) => item.code === "redundant-annotation")) return false;
  const changes = action.edit?.documentChanges;
  if (changes?.length !== 1 || !("textDocument" in changes[0]) ||
    changes[0].textDocument.uri !== fix.uri || changes[0].textDocument.version !== fix.version) return false;
  const edits = changes[0].edits.map(({ range, newText }) => ({ newText,
    range: [range.start.line, range.start.character, range.end.line, range.end.character] }));
  return JSON.stringify(edits) === JSON.stringify(fix.edits);
}

export async function applyWarningFix(fix: WarningFix, request?: WarningFixRequest): Promise<boolean> {
  let approved = false;
  if (currentDocument(fix) && request) {
    try {
      // Closed siblings and project manifests can change without a document
      // version change. Re-run the compiler's project proof at invocation.
      const [startLine, startCharacter, endLine, endCharacter] = fix.range;
      const fresh = await request({ textDocument: { uri: fix.uri },
        range: { start: { line: startLine, character: startCharacter }, end: { line: endLine, character: endCharacter } },
        context: { diagnostics: [], only: [fix.kind], triggerKind: 1 } });
      approved = fresh?.some((action) => matchesFreshAction(action, fix)) ?? false;
    } catch {
      // A failed proof must not apply the cached edit.
    }
  }
  const document = currentDocument(fix);
  if (!approved || !document) {
    void window.showWarningMessage(document
      ? "The quick fix could not be revalidated. Request it again."
      : "The document changed. Request the quick fix again.");
    return false;
  }
  const edit = new WorkspaceEdit();
  for (const change of fix.edits) edit.replace(document.uri, new Range(...change.range), change.newText);
  return workspace.applyEdit(edit);
}

export function registerWarningFixes(context: ExtensionContext, request: WarningFixRequest): void {
  context.subscriptions.push(commands.registerCommand(applyWarningFixCommand, (fix: WarningFix) => applyWarningFix(fix, request)));
}

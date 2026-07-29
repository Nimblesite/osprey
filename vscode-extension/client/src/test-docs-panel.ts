// `Osprey: Show Test Documentation` ([TESTING-DOC-VSCODE]). VS Code's TestItem API
// carries no tooltip of its own — the tree row's hover shows its label and
// description — so a case's full `///` documentation is surfaced three ways:
// the whole block collapsed to one line rides in the tree row's description,
// where the row truncates it and the hover shows it entire
// (test-explorer-docs.ts); a hover over the `test(...)` call renders it as
// Markdown (the language server); and this command opens it as a Markdown
// preview from either the Testing view's context menu or the cursor's position
// in a test file.
//
// The argument VS Code hands a `testing/item/context` command is not part of
// the stable API surface, so resolution is defensive: a TestItem-shaped object
// wins, a plain id string is looked up next, and otherwise the active editor's
// cursor line decides. Every step is a pure exported helper.

import * as vscode from "vscode";
import { documentedTestIds, testDocFor } from "./test-explorer";
import { leafTestId } from "./test-explorer-parse";
import { testDocMarkdown, type TestDoc } from "./test-explorer-docs";

/** The command id that opens a case's documentation. */
export const SHOW_TEST_DOCS_COMMAND = "osprey.showTestDocumentation";

/** The subset of vscode.TestItem this command needs from its argument. */
interface TestItemArg {
  readonly id?: unknown;
  readonly uri?: unknown;
  readonly label?: unknown;
}

/**
 * The leaf TestItem id an invocation targets. A TestItem-shaped argument
 * supplies its own id; a bare string is taken as an id; otherwise the active
 * editor's file + cursor line resolve to the case declared on that line.
 * Returns undefined when nothing matches.
 */
export function resolveTargetId(
  argument: unknown,
  activeUri: vscode.Uri | undefined,
  cursorLine: number | undefined,
  knownIds: readonly string[] = documentedTestIds(),
  docOf: (id: string) => TestDoc | undefined = testDocFor,
): string | undefined {
  if (typeof argument === "string" && knownIds.includes(argument)) {
    return argument;
  }
  const item = argument as TestItemArg | null | undefined;
  if (
    item !== null &&
    item !== undefined &&
    typeof item.id === "string" &&
    knownIds.includes(item.id)
  ) {
    return item.id;
  }
  if (activeUri === undefined || cursorLine === undefined) {
    return undefined;
  }
  const prefix = leafTestId(activeUri.toString(), "");
  // The nearest case declared at or above the cursor owns the position — the
  // same rule the language server's enclosing-declaration hover uses.
  const candidates = knownIds
    .filter((id) => id.startsWith(prefix))
    .map((id) => ({ id, line: docOf(id)?.line ?? 0 }))
    .filter((entry) => entry.line > 0 && entry.line <= cursorLine + 1)
    .sort((a, b) => b.line - a.line);
  return candidates[0]?.id;
}

/** The message shown when no case could be resolved from the invocation. */
export const NO_TEST_MESSAGE =
  "No Osprey test case here. Put the cursor on a `test(...)` call, or run this from the Testing view.";

/** Renders Markdown; injectable so tests observe the content without a webview. */
export type MarkdownPresenter = (markdown: string, title: string) => void;

/** The default presenter: an untitled Markdown document, previewed beside. */
export const previewMarkdown: MarkdownPresenter = (markdown, _title) => {
  void vscode.workspace
    .openTextDocument({ content: markdown, language: "markdown" })
    .then((doc) =>
      vscode.window.showTextDocument(doc, {
        preview: true,
        viewColumn: vscode.ViewColumn.Beside,
      }),
    );
};

/**
 * The command body. Resolves the target case, renders its documentation, and
 * returns the Markdown it presented (undefined when nothing resolved) so tests
 * can assert the exact content.
 */
export function showTestDocumentation(
  argument: unknown,
  present: MarkdownPresenter = previewMarkdown,
  editor: vscode.TextEditor | undefined = vscode.window.activeTextEditor,
  notify: (message: string) => void = (message) =>
    void vscode.window.showInformationMessage(message),
): string | undefined {
  const id = resolveTargetId(
    argument,
    editor?.document.uri,
    editor?.selection.active.line,
  );
  const doc = id === undefined ? undefined : testDocFor(id);
  if (id === undefined || doc === undefined) {
    notify(NO_TEST_MESSAGE);
    return undefined;
  }
  const filePath = vscode.Uri.parse(id.split(" ")[0] ?? "").fsPath;
  const markdown = testDocMarkdown(doc, filePath);
  present(markdown, `${doc.name} — documentation`);
  return markdown;
}

/** Register the command; `commandId` is overridable only so tests can isolate. */
export function registerTestDocsCommand(
  context: vscode.ExtensionContext,
  commandId: string = SHOW_TEST_DOCS_COMMAND,
): vscode.Disposable {
  const disposable = vscode.commands.registerCommand(
    commandId,
    (argument: unknown) => showTestDocumentation(argument),
  );
  context.subscriptions.push(disposable);
  return disposable;
}

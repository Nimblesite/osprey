import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";

export interface ExpectedDiagnostic {
  code: string;
  message: string;
  text: string;
  occurrence?: number;
  unnecessary?: boolean;
}

const published = new Set<string>();
vscode.languages.onDidChangeDiagnostics((event) => {
  for (const uri of event.uris) published.add(uri.toString());
});

export async function waitFor<T>(read: () => T, accepts: (value: T) => boolean, description: string): Promise<T> {
  const deadline = Date.now() + 20000;
  let value = read();
  while (!accepts(value) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 50));
    value = read();
  }
  assert.ok(accepts(value), `${description}: ${JSON.stringify(value)}`);
  return value;
}

export function rangeOf(document: vscode.TextDocument, text: string, occurrence = 0): vscode.Range {
  const source = document.getText();
  let offset = -1;
  for (let index = 0; index <= occurrence; index++) offset = source.indexOf(text, offset + 1);
  assert.notStrictEqual(offset, -1, `Missing source fragment ${JSON.stringify(text)}`);
  return new vscode.Range(document.positionAt(offset), document.positionAt(offset + text.length));
}

export async function openSource(name: string, source: string): Promise<vscode.TextEditor> {
  const root = process.env.OSPREY_VSIX_TEST_ROOT;
  assert.ok(root, "Installed VSIX runner must provide its isolated root");
  const file = path.join(root, "workspace", name);
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, source);
  const document = await vscode.workspace.openTextDocument(file);
  const editor = await vscode.window.showTextDocument(document, { preview: false });
  assert.strictEqual(document.languageId, name.endsWith(".ospml") ? "osprey-ml" : "osprey");
  assert.strictEqual(document.getText(), source);
  assert.strictEqual(document.isDirty, false);
  return editor;
}

function summary(diagnostic: vscode.Diagnostic): object {
  return { code: diagnostic.code, message: diagnostic.message, severity: diagnostic.severity,
    source: diagnostic.source, range: diagnostic.range, tags: diagnostic.tags ?? [] };
}

export async function diagnostics(document: vscode.TextDocument, expected: ExpectedDiagnostic[]): Promise<vscode.Diagnostic[]> {
  const wanted = expected.map((item) => ({ code: item.code, message: item.message,
    severity: vscode.DiagnosticSeverity.Warning, source: "osprey",
    range: rangeOf(document, item.text, item.occurrence),
    tags: item.unnecessary ? [vscode.DiagnosticTag.Unnecessary] : [] }));
  const actual = await waitFor(() => vscode.languages.getDiagnostics(document.uri),
    (items) => published.has(document.uri.toString()) && JSON.stringify(items.map(summary)) === JSON.stringify(wanted),
    `Exact diagnostics for ${document.fileName}; expected ${JSON.stringify(wanted)}`);
  assert.deepStrictEqual(actual.map(summary), wanted);
  return actual;
}

export async function actions(document: vscode.TextDocument, range: vscode.Range, kind = vscode.CodeActionKind.QuickFix): Promise<vscode.CodeAction[]> {
  const found = await vscode.commands.executeCommand<vscode.CodeAction[]>(
    "vscode.executeCodeActionProvider", document.uri, range, kind.value);
  assert.ok(Array.isArray(found), "Code action provider must answer the request");
  return found;
}

export function assertAction(action: vscode.CodeAction, title: string, diagnostic: vscode.Diagnostic): void {
  assert.strictEqual(action.title, title);
  assert.strictEqual(action.kind?.value, "quickfix");
  assert.strictEqual(action.isPreferred, true);
  assert.strictEqual(action.disabled, undefined);
  assert.strictEqual(action.edit, undefined, "VSIX must guard edits until invocation");
  assert.strictEqual(action.command?.command, "osprey.applyWarningFix");
  assert.strictEqual(action.command?.title, title);
  // VS Code's executeCodeActionProvider omits diagnostics from its public
  // return object. The wire suite checks that attachment; here the source edit
  // must overlap the exact published diagnostic the user requested a fix for.
  const edits = action.command?.arguments?.[0].edits as { range: [number, number, number, number] }[];
  assert.ok(edits.some((edit) => new vscode.Range(...edit.range).intersection(diagnostic.range)),
    "The guarded edit must address this diagnostic's exact source range");
}

export function assertEdit(action: vscode.CodeAction, document: vscode.TextDocument, expected: string, movedExport = false): void {
  assert.strictEqual(action.edit, undefined, "No unguarded workspace edit may bypass the version check");
  assert.strictEqual(action.command?.command, "osprey.applyWarningFix");
  assert.strictEqual(action.command?.arguments?.length, 1);
  const payload = action.command.arguments[0];
  assert.strictEqual(payload.uri, document.uri.toString(), "Only the current document is edited");
  assert.strictEqual(payload.version, document.version);
  assert.strictEqual(payload.source, document.getText());
  assert.deepStrictEqual(payload.dependencies, vscode.workspace.textDocuments
    .filter((item) => ["osprey", "osprey-ml"].includes(item.languageId))
    .map((item) => ({ uri: item.uri.toString(), version: item.version, source: item.getText() }))
    .sort((left, right) => left.uri.localeCompare(right.uri)));
  const edits = payload.edits.map((edit: { range: [number, number, number, number]; newText: string }) => ({
    start: document.offsetAt(new vscode.Position(edit.range[0], edit.range[1])),
    end: document.offsetAt(new vscode.Position(edit.range[2], edit.range[3])), text: edit.newText,
  })).sort((a: { start: number }, b: { start: number }) => b.start - a.start);
  const changed = edits.reduce((text: string, edit: { start: number; end: number; text: string }) =>
    text.slice(0, edit.start) + edit.text + text.slice(edit.end), document.getText());
  assert.ok(edits.length > 0);
  assert.ok(edits.every((edit: { text: string }) => edit.text === "" || (movedExport && edit.text === "export ")),
    "Only annotation deletion and an explicitly checked export relocation are allowed");
  assert.strictEqual(changed, expected, "Exact edit must preserve body, comments and surrounding text");
}

export async function invokeFix(editor: vscode.TextEditor, range: vscode.Range, expected: string, kind = "quickfix"): Promise<void> {
  await vscode.window.showTextDocument(editor.document, { preserveFocus: false });
  await vscode.commands.executeCommand("workbench.action.focusActiveEditorGroup");
  assert.strictEqual(vscode.window.activeTextEditor?.document.uri.toString(), editor.document.uri.toString());
  editor.selection = new vscode.Selection(range.start, range.start);
  // Setting selection sends an asynchronous RPC. Exercise native cursor
  // movement and observe its return before asking the focused editor for a
  // quick fix; activeTextEditor alone does not prove main-thread readiness.
  await vscode.commands.executeCommand("cursorMove", { to: "right", by: "character", value: 1 });
  await waitFor(() => editor.selection.active, (position) => position.isEqual(range.start.translate(0, 1)),
    "Native cursor movement reaches the requested source editor");
  await vscode.commands.executeCommand("cursorMove", { to: "left", by: "character", value: 1 });
  await waitFor(() => editor.selection.active, (position) => position.isEqual(range.start),
    "Native cursor returns to the exact annotation range");
  assert.ok(editor.selection.isEmpty);
  await vscode.commands.executeCommand("editor.action.codeAction", { kind, apply: "first" });
  await waitFor(() => editor.document.getText(), (text) => text === expected, "Editor applied exact quick fix");
  assert.strictEqual(editor.document.isDirty, true);
}

export async function save(document: vscode.TextDocument): Promise<void> {
  assert.strictEqual(await document.save(), true);
  assert.strictEqual(document.isDirty, false);
  assert.strictEqual(fs.readFileSync(document.uri.fsPath, "utf8"), document.getText());
}

export async function replace(editor: vscode.TextEditor, text: string, replacement: string): Promise<void> {
  assert.strictEqual(await editor.edit((edit) => edit.replace(rangeOf(editor.document, text), replacement)), true);
  assert.strictEqual(editor.document.isDirty, true);
}

export async function closeEditors(): Promise<void> {
  // Failed assertions must not leave a modal Save prompt blocking later tests.
  await vscode.workspace.saveAll(false);
  const tabs = vscode.window.tabGroups.all.flatMap((group) => group.tabs);
  if (tabs.length) assert.strictEqual(await vscode.window.tabGroups.close(tabs), true);
  await vscode.commands.executeCommand("workbench.action.closePanel");
  await waitFor(() => vscode.window.visibleTextEditors.map((editor) => editor.document.uri.toString()),
    (uris) => uris.length === 0, "All editor documents closed");
}

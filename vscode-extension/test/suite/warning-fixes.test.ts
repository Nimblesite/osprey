import * as assert from "assert";
import * as vscode from "vscode";
import { CodeAction as ProtocolAction } from "vscode-languageserver-protocol";
import { applyWarningFix, applyWarningFixCommand, warningFixMiddleware } from "../../client/src/warning-fixes";

function openSources() {
  return vscode.workspace.textDocuments.filter((document) => ["osprey", "osprey-ml"].includes(document.languageId))
    .map((document) => ({ uri: document.uri.toString(), version: document.version, source: document.getText() }))
    .sort((left, right) => left.uri.localeCompare(right.uri));
}

function annotated(document: vscode.TextDocument): vscode.CodeAction & { data?: object } {
  const action: vscode.CodeAction & { data?: object } = new vscode.CodeAction("Remove redundant type signature", vscode.CodeActionKind.QuickFix);
  const diagnostic = new vscode.Diagnostic(new vscode.Range(0, 0, 0, 3), "redundant", vscode.DiagnosticSeverity.Warning);
  diagnostic.code = "redundant-annotation";
  action.diagnostics = [diagnostic];
  action.isPreferred = true;
  action.data = { uri: document.uri.toString(), version: document.version };
  action.edit = new vscode.WorkspaceEdit();
  action.edit.delete(document.uri, new vscode.Range(0, 0, 0, 3));
  return action;
}

async function wrap(document: vscode.TextDocument, actions: (vscode.CodeAction | vscode.Command)[] | null) {
  const middleware = warningFixMiddleware.provideCodeActions;
  assert.ok(middleware);
  const cancellation = new vscode.CancellationTokenSource();
  try {
    return await middleware(document, new vscode.Range(0, 0, 0, 0),
      { diagnostics: [], only: undefined, triggerKind: vscode.CodeActionTriggerKind.Invoke }, cancellation.token, async () => actions);
  } finally {
    cancellation.dispose();
  }
}

function freshAction(document: vscode.TextDocument): ProtocolAction {
  return { title: "Remove redundant type signature", kind: "quickfix", diagnostics: [{
    range: { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } }, message: "redundant", code: "redundant-annotation",
  }], edit: { documentChanges: [{ textDocument: { uri: document.uri.toString(), version: document.version }, edits: [{
    range: { start: { line: 0, character: 0 }, end: { line: 0, character: 3 } }, newText: "",
  }] }] } };
}

suite("Warning fix document-version guard", () => {
  test("converted warning carries plain command payload with the original document snapshot", async () => {
    const document = await vscode.workspace.openTextDocument({ content: "abc body", language: "plaintext" });
    const action = annotated(document);
    const originalDiagnostics = action.diagnostics;
    assert.deepStrictEqual(await wrap(document, [action]), [action]);
    assert.strictEqual(action.edit, undefined);
    assert.strictEqual(action.isPreferred, true);
    assert.strictEqual(action.diagnostics, originalDiagnostics);
    assert.deepStrictEqual(action.command, { command: applyWarningFixCommand, title: action.title, arguments: [{
      uri: document.uri.toString(), version: document.version, source: "abc body", dependencies: openSources(),
      title: action.title, kind: "quickfix", range: [0, 0, 0, 0] as [number, number, number, number],
      edits: [{ newText: "", range: [0, 0, 0, 3] as [number, number, number, number] }],
    }] });
    assert.strictEqual(await applyWarningFix(action.command.arguments?.[0], async (params) => {
      assert.deepStrictEqual(params, { textDocument: { uri: document.uri.toString() },
        range: { start: { line: 0, character: 0 }, end: { line: 0, character: 0 } },
        context: { diagnostics: [], only: ["quickfix"], triggerKind: 1 } });
      return [freshAction(document)];
    }), true);
    assert.strictEqual(document.getText(), " body");
  });

  test("ordinary server commands and actions retain their original behavior", async () => {
    const document = await vscode.workspace.openTextDocument({ content: "content", language: "plaintext" });
    const command = { command: "other.command", title: "Other" };
    const noDiagnostic = new vscode.CodeAction("No diagnostic");
    const unrelated = annotated(document);
    assert.ok(unrelated.diagnostics);
    unrelated.diagnostics[0].code = "other-rule";
    const edit = unrelated.edit;
    assert.deepStrictEqual(await wrap(document, [command, noDiagnostic, unrelated]), [command, noDiagnostic, unrelated]);
    assert.strictEqual(unrelated.edit, edit);
    assert.deepStrictEqual(await wrap(document, []), []);
    assert.strictEqual(await wrap(document, null), undefined);
  });

  test("missing or inconsistent version metadata disables the unsafe edit", async () => {
    const document = await vscode.workspace.openTextDocument({ content: "content", language: "plaintext" });
    for (const data of [undefined, {}, { uri: "file:///wrong", version: document.version },
      { uri: document.uri.toString(), version: document.version + 1 }]) {
      const action = annotated(document);
      action.data = data;
      await wrap(document, [action]);
      assert.strictEqual(action.edit, undefined);
      assert.strictEqual(action.command, undefined);
      assert.deepStrictEqual(action.disabled, { reason: "The document changed. Request the quick fix again." });
    }
  });

  test("absent, empty or cross-document workspace edits cannot bypass the guard", async () => {
    const document = await vscode.workspace.openTextDocument({ content: "content", language: "plaintext" });
    const other = await vscode.workspace.openTextDocument({ content: "other", language: "plaintext" });
    const foreign = new vscode.WorkspaceEdit();
    foreign.delete(other.uri, new vscode.Range(0, 0, 0, 1));
    const multiple = new vscode.WorkspaceEdit();
    multiple.delete(document.uri, new vscode.Range(0, 0, 0, 1));
    multiple.delete(other.uri, new vscode.Range(0, 0, 0, 1));
    for (const edit of [undefined, new vscode.WorkspaceEdit(), foreign, multiple]) {
      const action = annotated(document);
      action.edit = edit;
      await wrap(document, [action]);
      assert.strictEqual(action.edit, undefined);
      assert.strictEqual(action.command, undefined);
      assert.strictEqual(action.disabled?.reason, "The document changed. Request the quick fix again.");
    }
  });

  test("stale versions, reopened content and closed documents are rejected without edits", async () => {
    const document = await vscode.workspace.openTextDocument({ content: "content", language: "plaintext" });
    const fix = { uri: document.uri.toString(), version: document.version, source: document.getText(), dependencies: openSources(),
      title: "Remove redundant type signature", kind: "quickfix", range: [0, 0, 0, 0] as [number, number, number, number],
      edits: [{ newText: "", range: [0, 0, 0, 3] as [number, number, number, number] }] };
    for (const changed of [{ ...fix, version: fix.version + 1 }, { ...fix, source: "old content" },
      { ...fix, uri: "file:///not-open-anywhere.ospml" }]) {
      assert.strictEqual(await applyWarningFix(changed), false);
      assert.strictEqual(document.getText(), "content");
    }
  });

  test("changed or newly opened Osprey siblings invalidate a cached fix", async () => {
    const document = await vscode.workspace.openTextDocument({ content: "content", language: "plaintext" });
    const action = annotated(document);
    await wrap(document, [action]);
    assert.ok(action.command);
    const sibling = await vscode.workspace.openTextDocument({ content: "value = 1", language: "osprey-ml" });
    assert.strictEqual(await applyWarningFix(action.command.arguments?.[0]), false);
    assert.strictEqual(document.getText(), "content");
    const fresh = annotated(document);
    await wrap(document, [fresh]);
    assert.ok(fresh.command);
    const edit = new vscode.WorkspaceEdit();
    edit.insert(sibling.uri, new vscode.Position(0, 9), "0");
    assert.strictEqual(await vscode.workspace.applyEdit(edit), true);
    assert.strictEqual(await applyWarningFix(fresh.command.arguments?.[0]), false);
    assert.strictEqual(document.getText(), "content");
  });

  test("absent, changed, unversioned and failed fresh compiler proofs reject cached edits", async () => {
    const document = await vscode.workspace.openTextDocument({ content: "abc body", language: "plaintext" });
    const action = annotated(document);
    await wrap(document, [action]);
    const payload = action.command?.arguments?.[0];
    const original = freshAction(document);
    const changed = structuredClone(original);
    assert.ok(changed.edit?.documentChanges?.[0] && "textDocument" in changed.edit.documentChanges[0]);
    changed.edit.documentChanges[0].edits[0].newText = "different";
    const stale = structuredClone(original);
    assert.ok(stale.edit?.documentChanges?.[0] && "textDocument" in stale.edit.documentChanges[0]);
    stale.edit.documentChanges[0].textDocument.version = document.version + 1;
    const foreign = structuredClone(original);
    assert.ok(foreign.edit?.documentChanges?.[0] && "textDocument" in foreign.edit.documentChanges[0]);
    foreign.edit.documentChanges[0].textDocument.uri = "file:///foreign.ospml";
    for (const result of [null, [], [changed], [stale], [foreign], [{ ...original, edit: undefined }],
      [{ ...original, diagnostics: [] }], [{ ...original, title: "Different fix" }],
      [{ ...original, kind: "source.fixAll.osprey" }], [{ ...original, disabled: { reason: "unsafe" } }],
      [{ title: "Other command", command: "other.command" }]]) {
      assert.strictEqual(await applyWarningFix(payload, async () => result), false);
      assert.strictEqual(document.getText(), "abc body");
    }
    assert.strictEqual(await applyWarningFix(payload, async () => { throw new Error("server disconnected"); }), false);
    assert.strictEqual(await applyWarningFix(payload), false, "No validator must fail closed");
    assert.strictEqual(document.getText(), "abc body");
  });

  test("an editor change while the fresh compiler proof is pending invalidates the returned proof", async () => {
    const document = await vscode.workspace.openTextDocument({ content: "abc body", language: "plaintext" });
    const action = annotated(document);
    await wrap(document, [action]);
    const response = freshAction(document);
    assert.strictEqual(await applyWarningFix(action.command?.arguments?.[0], async () => {
      const edit = new vscode.WorkspaceEdit();
      edit.insert(document.uri, new vscode.Position(0, 8), " changed");
      assert.strictEqual(await vscode.workspace.applyEdit(edit), true);
      return [response];
    }), false);
    assert.strictEqual(document.getText(), "abc body changed");
  });
});

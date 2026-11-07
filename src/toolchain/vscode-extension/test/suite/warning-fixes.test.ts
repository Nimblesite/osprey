import * as assert from "assert";
import * as vscode from "vscode";
import {
  CodeAction as ProtocolAction,
  type TextDocumentEdit,
} from "vscode-languageserver-protocol";
import {
  applyWarningFix,
  applyWarningFixCommand,
  warningFixMiddleware,
} from "../../client/src/warning-fixes";

function openSources() {
  return vscode.workspace.textDocuments
    .filter((document) => ["osprey", "osprey-ml"].includes(document.languageId))
    .map((document) => ({
      uri: document.uri.toString(),
      version: document.version,
      source: document.getText(),
    }))
    .sort((left, right) => left.uri.localeCompare(right.uri));
}

/**
 * An unsaved document to hang a guarded action on: `plaintext` by default so
 * the Osprey guard sees a foreign document, with the content the action edits.
 */
function untitled(
  content = "content",
  language = "plaintext",
): Thenable<vscode.TextDocument> {
  return vscode.workspace.openTextDocument({ content, language });
}

/** An annotated action for `document` after the guard middleware wrapped it. */
async function wrappedAction(
  document: vscode.TextDocument,
): Promise<vscode.CodeAction & { data?: object }> {
  const action = annotated(document);
  await wrap(document, [action]);
  return action;
}

function annotated(
  document: vscode.TextDocument,
): vscode.CodeAction & { data?: object } {
  const action: vscode.CodeAction & { data?: object } = new vscode.CodeAction(
    "Remove redundant type signature",
    vscode.CodeActionKind.QuickFix,
  );
  const diagnostic = new vscode.Diagnostic(
    new vscode.Range(0, 0, 0, 3),
    "redundant",
    vscode.DiagnosticSeverity.Warning,
  );
  diagnostic.code = "redundant-annotation";
  action.diagnostics = [diagnostic];
  action.isPreferred = true;
  action.data = { uri: document.uri.toString(), version: document.version };
  action.edit = new vscode.WorkspaceEdit();
  action.edit.delete(document.uri, new vscode.Range(0, 0, 0, 3));
  return action;
}

async function wrap(
  document: vscode.TextDocument,
  actions: (vscode.CodeAction | vscode.Command)[] | null,
) {
  const middleware = warningFixMiddleware.provideCodeActions;
  assert.ok(middleware);
  const cancellation = new vscode.CancellationTokenSource();
  try {
    return await middleware(
      document,
      new vscode.Range(0, 0, 0, 0),
      {
        diagnostics: [],
        only: undefined,
        triggerKind: vscode.CodeActionTriggerKind.Invoke,
      },
      cancellation.token,
      async () => actions,
    );
  } finally {
    cancellation.dispose();
  }
}

function freshAction(document: vscode.TextDocument): ProtocolAction {
  return {
    title: "Remove redundant type signature",
    kind: "quickfix",
    diagnostics: [
      {
        range: {
          start: { line: 0, character: 0 },
          end: { line: 0, character: 3 },
        },
        message: "redundant",
        code: "redundant-annotation",
      },
    ],
    edit: {
      documentChanges: [
        {
          textDocument: {
            uri: document.uri.toString(),
            version: document.version,
          },
          edits: [
            {
              range: {
                start: { line: 0, character: 0 },
                end: { line: 0, character: 3 },
              },
              newText: "",
            },
          ],
        },
      ],
    },
  };
}

/** The dirty untitled documents still open in the host. */
function dirtyUntitledDocuments(): string[] {
  return vscode.workspace.textDocuments
    .filter((document) => document.isUntitled && document.isDirty)
    .map((document) => document.uri.toString());
}

/** Discard every untitled document. Reverting first closes it with no save prompt. */
async function discardUntitledDocuments(): Promise<void> {
  for (let attempt = 0; attempt < 20; attempt++) {
    const untitled = vscode.workspace.textDocuments.find(
      (document) => document.isUntitled,
    );
    if (!untitled) return;
    await vscode.window.showTextDocument(untitled);
    await vscode.commands.executeCommand(
      "workbench.action.revertAndCloseActiveEditor",
    );
  }
}

/**
 * A clone of `action` together with its first document change, which is
 * asserted to be the versioned edit a fresh proof always carries.
 */
function alteredProof(action: ProtocolAction): {
  proof: ProtocolAction;
  change: TextDocumentEdit;
} {
  const proof = structuredClone(action);
  const change = proof.edit?.documentChanges?.[0];
  assert.ok(
    change && "textDocument" in change,
    "the fresh proof carries a versioned document change",
  );
  return { proof, change };
}

/** The single delete-the-annotation edit every annotated action carries. */
const DELETE_ANNOTATION_EDITS = [
  {
    newText: "",
    range: [0, 0, 0, 3] as [number, number, number, number],
  },
];

suite("Warning fix document-version guard", () => {
  teardown(discardUntitledDocuments);

  // A dirty untitled document outlives this suite. When a later suite starts a
  // debug session, VS Code saves the active group's dirty editors first, and
  // saving an untitled file raises a native Save As sheet that blocks the test
  // host until a human dismisses it. This suite must leave none behind.
  suiteTeardown(() => assert.deepStrictEqual(dirtyUntitledDocuments(), []));

  /** Wrap `action` and assert the guard stripped its edit and command. */
  async function assertGuarded(
    document: vscode.TextDocument,
    action: vscode.CodeAction,
  ): Promise<vscode.CodeAction> {
    await wrap(document, [action]);
    assert.strictEqual(action.edit, undefined);
    assert.strictEqual(action.command, undefined);
    return action;
  }

  /** Insert `text` at `character` on line 0, asserting the edit lands. */
  async function insertOnFirstLine(
    document: vscode.TextDocument,
    character: number,
    text: string,
  ): Promise<void> {
    const edit = new vscode.WorkspaceEdit();
    edit.insert(document.uri, new vscode.Position(0, character), text);
    assert.strictEqual(await vscode.workspace.applyEdit(edit), true);
  }

  test("converted warning carries plain command payload with the original document snapshot", async () => {
    const document = await untitled("abc body");
    const action = annotated(document);
    const originalDiagnostics = action.diagnostics;
    assert.deepStrictEqual(await wrap(document, [action]), [action]);
    assert.strictEqual(action.edit, undefined);
    assert.strictEqual(action.isPreferred, true);
    assert.strictEqual(action.diagnostics, originalDiagnostics);
    assert.deepStrictEqual(action.command, {
      command: applyWarningFixCommand,
      title: action.title,
      arguments: [
        {
          uri: document.uri.toString(),
          version: document.version,
          source: "abc body",
          dependencies: openSources(),
          title: action.title,
          kind: "quickfix",
          range: [0, 0, 0, 0] as [number, number, number, number],
          edits: DELETE_ANNOTATION_EDITS,
        },
      ],
    });
    assert.strictEqual(
      await applyWarningFix(action.command.arguments?.[0], async (params) => {
        assert.deepStrictEqual(params, {
          textDocument: { uri: document.uri.toString() },
          range: {
            start: { line: 0, character: 0 },
            end: { line: 0, character: 0 },
          },
          context: { diagnostics: [], only: ["quickfix"], triggerKind: 1 },
        });
        return [freshAction(document)];
      }),
      true,
    );
    assert.strictEqual(document.getText(), " body");
  });

  test("ordinary server commands and actions retain their original behavior", async () => {
    const document = await untitled();
    const command = { command: "other.command", title: "Other" };
    const noDiagnostic = new vscode.CodeAction("No diagnostic");
    const unrelated = annotated(document);
    assert.ok(unrelated.diagnostics);
    unrelated.diagnostics[0].code = "other-rule";
    const edit = unrelated.edit;
    assert.deepStrictEqual(
      await wrap(document, [command, noDiagnostic, unrelated]),
      [command, noDiagnostic, unrelated],
    );
    assert.strictEqual(unrelated.edit, edit);
    assert.deepStrictEqual(await wrap(document, []), []);
    assert.strictEqual(await wrap(document, null), undefined);
  });

  test("missing or inconsistent version metadata disables the unsafe edit", async () => {
    const document = await untitled();
    for (const data of [
      undefined,
      {},
      { uri: "file:///wrong", version: document.version },
      { uri: document.uri.toString(), version: document.version + 1 },
    ]) {
      const action = annotated(document);
      action.data = data;
      assert.deepStrictEqual((await assertGuarded(document, action)).disabled, {
        reason: "The document changed. Request the quick fix again.",
      });
    }
  });

  test("absent, empty or cross-document workspace edits cannot bypass the guard", async () => {
    const document = await untitled();
    const other = await untitled("other");
    const foreign = new vscode.WorkspaceEdit();
    foreign.delete(other.uri, new vscode.Range(0, 0, 0, 1));
    const multiple = new vscode.WorkspaceEdit();
    multiple.delete(document.uri, new vscode.Range(0, 0, 0, 1));
    multiple.delete(other.uri, new vscode.Range(0, 0, 0, 1));
    for (const edit of [
      undefined,
      new vscode.WorkspaceEdit(),
      foreign,
      multiple,
    ]) {
      const action = annotated(document);
      action.edit = edit;
      assert.strictEqual(
        (await assertGuarded(document, action)).disabled?.reason,
        "The document changed. Request the quick fix again.",
      );
    }
  });

  test("stale versions, reopened content and closed documents are rejected without edits", async () => {
    const document = await untitled();
    const fix = {
      uri: document.uri.toString(),
      version: document.version,
      source: document.getText(),
      dependencies: openSources(),
      title: "Remove redundant type signature",
      kind: "quickfix",
      range: [0, 0, 0, 0] as [number, number, number, number],
      edits: DELETE_ANNOTATION_EDITS,
    };
    for (const changed of [
      { ...fix, version: fix.version + 1 },
      { ...fix, source: "old content" },
      { ...fix, uri: "file:///not-open-anywhere.ospml" },
    ]) {
      assert.strictEqual(await applyWarningFix(changed), false);
      assert.strictEqual(document.getText(), "content");
    }
  });

  test("changed or newly opened Osprey siblings invalidate a cached fix", async () => {
    const document = await untitled();
    const action = await wrappedAction(document);
    assert.ok(action.command);
    const sibling = await untitled("value = 1", "osprey-ml");
    assert.strictEqual(
      await applyWarningFix(action.command.arguments?.[0]),
      false,
    );
    assert.strictEqual(document.getText(), "content");
    const fresh = await wrappedAction(document);
    assert.ok(fresh.command);
    await insertOnFirstLine(sibling, 9, "0");
    assert.strictEqual(
      await applyWarningFix(fresh.command.arguments?.[0]),
      false,
    );
    assert.strictEqual(document.getText(), "content");
  });

  test("absent, changed, unversioned and failed fresh compiler proofs reject cached edits", async () => {
    const document = await untitled("abc body");
    const action = await wrappedAction(document);
    const payload = action.command?.arguments?.[0];
    const original = freshAction(document);
    const changed = alteredProof(original);
    Object.assign(changed.change.edits[0], { newText: "different" });
    const stale = alteredProof(original);
    stale.change.textDocument.version = document.version + 1;
    const foreign = alteredProof(original);
    foreign.change.textDocument.uri = "file:///foreign.ospml";
    for (const result of [
      null,
      [],
      [changed.proof],
      [stale.proof],
      [foreign.proof],
      [{ ...original, edit: undefined }],
      [{ ...original, diagnostics: [] }],
      [{ ...original, title: "Different fix" }],
      [{ ...original, kind: "source.fixAll.osprey" }],
      [{ ...original, disabled: { reason: "unsafe" } }],
      [{ title: "Other command", command: "other.command" }],
    ]) {
      assert.strictEqual(
        await applyWarningFix(payload, async () => result),
        false,
      );
      assert.strictEqual(document.getText(), "abc body");
    }
    assert.strictEqual(
      await applyWarningFix(payload, async () => {
        throw new Error("server disconnected");
      }),
      false,
    );
    assert.strictEqual(
      await applyWarningFix(payload),
      false,
      "No validator must fail closed",
    );
    assert.strictEqual(document.getText(), "abc body");
  });

  test("an editor change while the fresh compiler proof is pending invalidates the returned proof", async () => {
    const document = await untitled("abc body");
    const action = await wrappedAction(document);
    const response = freshAction(document);
    assert.strictEqual(
      await applyWarningFix(action.command?.arguments?.[0], async () => {
        await insertOnFirstLine(document, 8, " changed");
        return [response];
      }),
      false,
    );
    assert.strictEqual(document.getText(), "abc body changed");
  });
});

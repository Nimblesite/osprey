import * as assert from "assert";
import * as vscode from "vscode";
import { actions, closeEditors, diagnostics, ExpectedDiagnostic, openSource,
  rangeOf, replace, save, waitFor } from "./editor";

function unused(code: string, name: string, label: string, occurrence = 0): ExpectedDiagnostic {
  return { code: `unused-${code}`, message: `unused ${label} \`${name}\``, text: name,
    occurrence, unnecessary: true };
}

async function noRemoval(editor: vscode.TextEditor, warnings: ExpectedDiagnostic[]): Promise<void> {
  const raised = await diagnostics(editor.document, warnings);
  for (const diagnostic of raised) {
    assert.deepStrictEqual(await actions(editor.document, diagnostic.range), [],
      "Unused warnings must not offer annotation deletion or erase side effects");
  }
}

suite("Installed VSIX unused symbol diagnostics and editing", () => {
  teardown(closeEditors);

  test("repeated top-level pattern binders in both flavors retain distinct ranges through rename, use, undo and redo", async () => {
    for (const [name, source, body, read] of [
      ["top-level-patterns.osp", 'match 1 { unused => print("a") }\nmatch 2 { unused => print("b") }\n', 'print("b")', "print(unused)"],
      ["top-level-patterns.ospml", 'match 1\n    unused => print "a"\nmatch 2\n    unused => print "b"\n', 'print "b"', "print unused"],
    ]) {
      const editor = await openSource(name, source);
      const first = unused("pattern-binding", "unused", "pattern binding", 0);
      const second = unused("pattern-binding", "unused", "pattern binding", 1);
      await noRemoval(editor, [first, second]);
      await replace(editor, "unused", "_unused");
      await noRemoval(editor, [second]);
      assert.strictEqual(editor.document.getText(), source.replace("unused", "_unused"));
      await replace(editor, body, read);
      await diagnostics(editor.document, []);
      assert.strictEqual(editor.document.getText(), source.replace("unused", "_unused").replace(body, read));
      await vscode.commands.executeCommand("undo");
      await noRemoval(editor, [second]);
      await vscode.commands.executeCommand("undo");
      await noRemoval(editor, [first, second]);
      assert.strictEqual(editor.document.getText(), source);
      await vscode.commands.executeCommand("redo");
      await noRemoval(editor, [second]);
      await vscode.commands.executeCommand("redo");
      await diagnostics(editor.document, []);
      await save(editor.document);
    }
  });

  test("Default local and parameter warnings have exact name ranges; intentional names clear and undo restores them", async () => {
    const source = "fn choose(used, ignored) = { let spare = 1\nused }\nlet result = choose(2, 3)\n";
    const warnings = [unused("parameter", "ignored", "parameter"), unused("variable", "spare", "variable")];
    const editor = await openSource("unused-default.osp", source);
    await noRemoval(editor, warnings);
    assert.strictEqual(await editor.edit((edit) => {
      edit.insert(rangeOf(editor.document, "ignored").start, "_");
      edit.insert(rangeOf(editor.document, "spare").start, "_");
    }), true);
    await diagnostics(editor.document, []);
    assert.strictEqual(editor.document.getText(), source.replace("ignored", "_ignored").replace("spare", "_spare"));
    await save(editor.document);
    await vscode.commands.executeCommand("undo");
    await waitFor(() => editor.document.getText(), (text) => text === source, "Undo restores both unused names");
    await diagnostics(editor.document, warnings);
    await vscode.commands.executeCommand("redo");
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("ML currying never leaks an invented lambda owner and local use clears only its own warning", async () => {
    const source = "choose used ignored =\n    spare = 1\n    used\nresult = choose 2 3\n";
    const editor = await openSource("unused-curried.ospml", source);
    const parameter = unused("parameter", "ignored", "parameter");
    await noRemoval(editor, [parameter, unused("variable", "spare", "variable")]);
    await replace(editor, "    used\n", "    (used + spare) ?: 0\n");
    await diagnostics(editor.document, [parameter]);
    await replace(editor, "ignored", "_ignored");
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("a local initialization is retained; reading it clears the warning and undo restores it", async () => {
    const source = "fn work() = { let spare = 1\n2 }\nlet result = work()\n";
    const editor = await openSource("unused-local.osp", source);
    const warning = unused("variable", "spare", "variable");
    await noRemoval(editor, [warning]);
    await replace(editor, "\n2 }", "\nspare }");
    await diagnostics(editor.document, []);
    await save(editor.document);
    await vscode.commands.executeCommand("undo");
    await waitFor(() => editor.document.getText(), (text) => text === source, "Undo restores the unread local");
    await diagnostics(editor.document, [warning]);
    await save(editor.document);
  });

  test("shadowing warns on the outer declaration but an initializer reading it is a real use", async () => {
    const source = "fn shadow(value) = { let value = 7\nvalue }\nlet result = shadow(1)\n";
    const editor = await openSource("unused-shadow.osp", source);
    const warning = unused("parameter", "value", "parameter", 0);
    const [diagnostic] = await diagnostics(editor.document, [warning]);
    assert.ok(diagnostic.range.isEqual(rangeOf(editor.document, "value", 0)));
    assert.ok(!diagnostic.range.isEqual(rangeOf(editor.document, "value", 1)));
    await replace(editor, "= 7", "= value");
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("closure capture uses the outer parameter and only the written unused lambda parameter is faded", async () => {
    const editor = await openSource("unused-lambda.osp", "fn outer(value) = fn(other) => value\nlet result = outer(1)(2)\n");
    await noRemoval(editor, [unused("parameter", "other", "parameter")]);
    await replace(editor, "=> value", "=> (value + other) ?: 0");
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("record pattern warnings select the binder rather than a matching field at the call site", async () => {
    const editor = await openSource("unused-pattern.osp",
      "type Pair = { left: int, right: int }\nfn first(pair: Pair) = match pair { { left, right } => left }\nlet result = first(Pair { left: 1, right: 2 })\n");
    await noRemoval(editor, [unused("pattern-binding", "right", "pattern binding", 1)]);
    assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, ": Pair")), []);
    await replace(editor, "=> left", "=> (left + right) ?: 0");
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("list rest binding warns independently and a real tail read clears it", async () => {
    const editor = await openSource("unused-list-pattern.osp",
      "fn first(items) = match items { [head, ...tail] => head\n[] => 0 }\nlet result = first([1, 2])\n");
    await noRemoval(editor, [unused("pattern-binding", "tail", "pattern binding")]);
    await replace(editor, "=> head", "=> (head + listLength(tail)) ?: 0");
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("handler parameters carry their operation name while fiber captures remain used", async () => {
    const source = "effect Pick { choose: fn(int, int) -> int }\nfn work(seed) = handle Pick\n choose first second => resume(first)\nin await (spawn (perform Pick.choose(seed, 2)))\nlet result = work(7)\n";
    const editor = await openSource("unused-handler-fiber.osp", source);
    const warning = { ...unused("handler-parameter", "second", "handler parameter"),
      message: "unused handler parameter `second` of `Pick.choose`" };
    await noRemoval(editor, [warning]);
    await replace(editor, "resume(first)", "resume((first + second) ?: 0)");
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("writes to a mutable local do not count as reads and fixing its use leaves assignments intact", async () => {
    const source = "effect Set { put: fn(int) -> Unit }\nfn work() = { mut scratch = 1\nhandle Set\n put value => { scratch = value }\nin { perform Set.put(2) }\n3 }\nlet result = work()\n";
    const editor = await openSource("unused-mut.osp", source);
    await noRemoval(editor, [unused("variable", "scratch", "variable")]);
    await replace(editor, "\n3 }", "\nscratch }");
    await diagnostics(editor.document, []);
    assert.ok(editor.document.getText().includes("scratch = value"));
    assert.ok(editor.document.getText().includes("perform Set.put(2)"));
    await save(editor.document);
  });

  test("public declarations, extern contracts and intentional names remain unflagged in both flavors", async () => {
    for (const [name, source] of [
      ["unused-exempt.osp", "extern fn native(input: int) -> int\nfn public(_unused, _) = { let _spare = 1\n2 }\nlet externallyVisible = 3\n"],
      ["unused-exempt.ospml", "keep _unused _ =\n    _spare = 1\n    2\nextern native (input : int) -> int\nexternallyVisible = 3\n"],
    ]) {
      const editor = await openSource(name, source);
      await diagnostics(editor.document, []);
      assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, "input")), []);
      assert.strictEqual(editor.document.getText(), source);
      assert.strictEqual(editor.document.isDirty, false);
    }
  });
});

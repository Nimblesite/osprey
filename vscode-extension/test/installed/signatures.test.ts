import * as assert from "assert";
import { createHash } from "crypto";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { actions, assertAction, assertEdit, closeEditors, diagnostics, ExpectedDiagnostic,
  invokeFix, openSource, rangeOf, replace, save, waitFor } from "./editor";

const signatureTitle = "Remove redundant type signature";
const annotationTitle = "Remove redundant type annotation";
const fixAllKind = vscode.CodeActionKind.SourceFixAll.append("osprey");
const greetBody = 'greet name = "hi " + name\nmain () = print (greet "Ada")\n';
const greetHeader = "greet : string -> string";

function signature(name: string, type: string, text: string): ExpectedDiagnostic {
  return { code: "redundant-annotation", text,
    message: `redundant type signature on \`${name}\`: inference derives \`${type}\` without it` };
}

function inline(message: string, text: string, occurrence = 0): ExpectedDiagnostic {
  return { code: "redundant-annotation", message, text, occurrence };
}

async function singleFix(name: string, source: string, expected: string, warning: ExpectedDiagnostic): Promise<vscode.TextEditor> {
  const editor = await openSource(name, source);
  const [diagnostic] = await diagnostics(editor.document, [warning]);
  const fixes = await actions(editor.document, diagnostic.range);
  assert.strictEqual(fixes.length, 1);
  assertAction(fixes[0], signatureTitle, diagnostic);
  assertEdit(fixes[0], editor.document, expected);
  await invokeFix(editor, diagnostic.range, expected);
  await diagnostics(editor.document, []);
  assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, "greet")), []);
  await save(editor.document);
  return editor;
}

suite("Installed VSIX compiler warnings and signature edits", () => {
  teardown(closeEditors);

  test("runs the installed production extension with this build's bundled compiler", async () => {
    const extension = vscode.extensions.getExtension("nimblesite.osprey");
    assert.ok(extension, "The VSIX must be installed");
    const root = process.env.OSPREY_VSIX_EXTENSIONS;
    assert.ok(root);
    assert.ok(fs.realpathSync(extension.extensionPath).startsWith(fs.realpathSync(root) + path.sep));
    assert.notStrictEqual(fs.realpathSync(extension.extensionPath), fs.realpathSync(process.env.OSPREY_VSIX_SOURCE_EXTENSION ?? ""));
    assert.strictEqual(fs.existsSync(path.join(extension.extensionPath, "out/test")), false);
    const compiler = path.join(extension.extensionPath, "bin", `${process.platform}-${process.arch}`,
      process.platform === "win32" ? "osprey.exe" : "osprey");
    assert.strictEqual(createHash("sha256").update(fs.readFileSync(compiler)).digest("hex"), process.env.OSPREY_VSIX_COMPILER_SHA256);
    const clients = JSON.parse(process.env.OSPREY_VSIX_CLIENT_HASHES ?? "{}") as Record<string, string>;
    assert.deepStrictEqual(Object.keys(clients).sort(), ["extension", "warning-fixes"]);
    for (const [name, hash] of Object.entries(clients)) {
      assert.strictEqual(createHash("sha256").update(fs.readFileSync(path.join(extension.extensionPath,
        "out", "client", "src", `${name}.js`))).digest("hex"), hash, `Installed ${name} matches this build`);
    }
    assert.strictEqual(vscode.workspace.getConfiguration("osprey").get("server.compilerPath"), "");
    assert.strictEqual(vscode.workspace.getConfiguration("osprey").get("server.path"), "");
    assert.strictEqual(vscode.workspace.getConfiguration("chat").get("disableAIFeatures"), true);
    const editor = await openSource("activation.osp", 'fn greet(name) = "hi " + name\n');
    await extension.activate();
    assert.strictEqual(extension.isActive, true);
    await diagnostics(editor.document, []);
    assert.strictEqual(vscode.window.activeTextEditor?.document.uri.toString(), editor.document.uri.toString(),
      "Activation must leave the user's source editor focused");
    assert.ok(vscode.window.visibleTextEditors.every((item) => item.document.uri.scheme !== "output"),
      "Activation must not reveal the Debug output editor");
    const trap = process.env.OSPREY_VSIX_PATH_TRAP;
    assert.ok(trap);
    const calls = fs.existsSync(trap) ? fs.readFileSync(trap, "utf8").trim().split(/\r?\n/) : [];
    assert.ok(calls.every((args) => args === "--version"), `Only Shipwright version probes may use PATH: ${JSON.stringify(calls)}`);
  });

  test("ML Quick Fix deletes the signature; save, undo and redo preserve the body and warning lifecycle", async () => {
    const original = `${greetHeader}\n${greetBody}`;
    const warning = signature("greet", "(string) -> string", greetHeader);
    const editor = await singleFix("signature-lifecycle.ospml", original, greetBody, warning);
    await vscode.commands.executeCommand("undo");
    await waitFor(() => editor.document.getText(), (text) => text === original, "Undo restores the whole signature");
    await diagnostics(editor.document, [warning]);
    assert.strictEqual(editor.document.isDirty, true);
    await vscode.commands.executeCommand("redo");
    await waitFor(() => editor.document.getText(), (text) => text === greetBody, "Redo reapplies the same deletion");
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("one curried signature produces one action without invented lambda annotations", async () => {
    const header = "greet : string -> string -> string";
    const body = 'greet first second = first + " " + second\nmain () = print (greet "Ada" "Lovelace")\n';
    await singleFix("curried.ospml", `${header}\n${body}`, body,
      signature("greet", "(string) -> (string) -> string", header));
  });

  test("CRLF and Unicode comments survive removal and UTF-16 range translation", async () => {
    const prefix = "// 🦅 café signature\r\n";
    const body = greetBody.replace(/\n/g, "\r\n");
    const editor = await singleFix("unicode-crlf.ospml", `${prefix}${greetHeader}\r\n${body}`,
      prefix + body, signature("greet", "(string) -> string", greetHeader));
    assert.strictEqual(editor.document.eol, vscode.EndOfLine.CRLF);
    assert.strictEqual(editor.document.lineAt(0).text, "// 🦅 café signature");
  });

  test("a multiline signature is removed as one source header", async () => {
    const header = "greet : (string\n    -> string)";
    await singleFix("multiline.ospml", `${header}\n${greetBody}`, greetBody,
      signature("greet", "(string) -> string", header));
  });

  test("a same-line non-BMP string before a lambda annotation keeps UTF-16 offsets exact", async () => {
    const source = 'prefix = "🦅é" + (\\(value: string) => value + "!") "x"\n';
    const expected = source.replace(": string", "");
    const editor = await openSource("unicode-lambda.ospml", source);
    const [diagnostic] = await diagnostics(editor.document, [inline(
      "redundant type annotation on parameter `value` of `<lambda>`: inference derives `string` without it", ": string")]);
    assert.strictEqual(diagnostic.range.start.character, source.indexOf(": string"));
    const fixes = await actions(editor.document, diagnostic.range);
    assert.strictEqual(fixes.length, 1);
    assertAction(fixes[0], annotationTitle, diagnostic);
    assertEdit(fixes[0], editor.document, expected);
    await invokeFix(editor, diagnostic.range, expected);
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("escaped interpolation prefixes retain precise annotation and unused-name ranges through a quick fix", async () => {
    const source = 'fn greet() = "\\n🦅 ${(fn(value: int, ignored) => (value + 1) ?: 0)(7, 9)}"\nfn main() = print(greet())\n';
    const expected = source.replace(": int", "");
    const editor = await openSource("interpolated-warning.osp", source);
    const unused = { code: "unused-parameter", message: "unused parameter `ignored`", text: "ignored", unnecessary: true };
    const raised = await diagnostics(editor.document, [inline(
      "redundant type annotation on parameter `value` of `<lambda>`: inference derives `int` without it", ": int"), unused]);
    assert.strictEqual(raised[0].range.start.line, 0, "The escaped newline does not create a source line");
    const fixes = await actions(editor.document, raised[0].range);
    assert.strictEqual(fixes.length, 1);
    assertAction(fixes[0], annotationTitle, raised[0]);
    assertEdit(fixes[0], editor.document, expected);
    await invokeFix(editor, raised[0].range, expected);
    await diagnostics(editor.document, [unused]);
    await save(editor.document);
  });

  test("a trailing signature comment stays in the document at the same indentation", async () => {
    await singleFix("trailing-comment.ospml", `${greetHeader} // keep this explanation\n${greetBody}`,
      `// keep this explanation\n${greetBody}`, signature("greet", "(string) -> string", greetHeader));
  });

  test("a multiline header containing a nested block comment preserves every comment byte", async () => {
    const header = "greet : (string\n    (* outer (* nested *) comment *)\n    -> string)";
    await singleFix("nested-comment.ospml", `${header}\n${greetBody}`,
      `    (* outer (* nested *) comment *)\n${greetBody}`,
      signature("greet", "(string) -> string", header));
  });

  test("removing an exported signature retains the export on its definition", async () => {
    const body = '    greet name = "hi " + name\n';
    const source = `namespace warnings\nmodule M\n    export ${greetHeader}\n${body}`;
    const editor = await openSource("exported-header.ospml", source);
    const [diagnostic] = await diagnostics(editor.document, [signature("warnings::M::greet", "(string) -> string", greetHeader)]);
    const fixes = await actions(editor.document, diagnostic.range);
    assert.strictEqual(fixes.length, 1);
    assertAction(fixes[0], signatureTitle, diagnostic);
    const expected = `namespace warnings\nmodule M\n    export ${body.trimStart()}`;
    assertEdit(fixes[0], editor.document, expected, true);
    await invokeFix(editor, diagnostic.range, expected);
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("module implementation fix preserves the public signature contract", async () => {
    const contract = "namespace warnings\nsignature Api\n    greet : string -> string\n\nmodule M : Api\n";
    const header = "    greet : string -> string\n";
    const body = '    greet name = "hi " + name\n';
    const editor = await openSource("module-contract.ospml", contract + header + body);
    const warning = { ...signature("warnings::M::greet", "(string) -> string", greetHeader), occurrence: 1 };
    const [diagnostic] = await diagnostics(editor.document, [warning]);
    assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, greetHeader, 0)), []);
    const fixes = await actions(editor.document, diagnostic.range);
    assert.strictEqual(fixes.length, 1);
    assertAction(fixes[0], signatureTitle, diagnostic);
    assertEdit(fixes[0], editor.document, contract + body);
    await invokeFix(editor, diagnostic.range, contract + body);
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("Default parameter and result actions delete only their own annotation, then fix-all removes the joint safe set", async () => {
    const source = 'fn greet(name: string) -> string = "hi " + name\nfn main() = print(greet("Ada"))\n';
    const result = 'fn greet(name) = "hi " + name\nfn main() = print(greet("Ada"))\n';
    const editor = await openSource("default-pieces.osp", source);
    const warnings = [
      inline("redundant type annotation on parameter `name` of `greet`: inference derives `string` without it", ": string"),
      inline("redundant return type annotation on `greet`: inference derives `string` without it", "-> string"),
    ];
    const raised = await diagnostics(editor.document, warnings);
    for (const [index, diagnostic] of raised.entries()) {
      const fixes = await actions(editor.document, diagnostic.range);
      assert.strictEqual(fixes.length, 1);
      assertAction(fixes[0], annotationTitle, diagnostic);
      assertEdit(fixes[0], editor.document, source.replace(index === 0 ? ": string" : " -> string", ""));
    }
    const fixes = await actions(editor.document, rangeOf(editor.document, "greet"), fixAllKind);
    assert.strictEqual(fixes.length, 1);
    assert.strictEqual(fixes[0].title, "Remove all redundant type annotations");
    assert.strictEqual(fixes[0].kind?.value, fixAllKind.value);
    assertEdit(fixes[0], editor.document, result);
    await invokeFix(editor, raised[0].range, result, fixAllKind.value);
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("fix-all removes two signatures atomically and one undo restores both", async () => {
    const secondHeader = "shout : string -> string";
    const secondBody = 'shout name = name + "!"\n';
    const body = greetBody.replace('greet "Ada"', 'greet (shout "Ada")');
    const source = `${greetHeader}\n${body}${secondHeader}\n${secondBody}`;
    const expected = body + secondBody;
    const editor = await openSource("multiple.ospml", source);
    const warnings = [signature("greet", "(string) -> string", greetHeader),
      signature("shout", "(string) -> string", secondHeader)];
    await diagnostics(editor.document, warnings);
    const fixes = await actions(editor.document, rangeOf(editor.document, greetHeader), fixAllKind);
    assert.strictEqual(fixes.length, 1);
    assertEdit(fixes[0], editor.document, expected);
    await invokeFix(editor, rangeOf(editor.document, greetHeader), expected, fixAllKind.value);
    await diagnostics(editor.document, []);
    await vscode.commands.executeCommand("undo");
    await waitFor(() => editor.document.getText(), (text) => text === source, "One undo restores both headers");
    await diagnostics(editor.document, warnings);
    await save(editor.document);
  });

  test("fix-all retains the necessary return constraint when parameter annotations depend on it", async () => {
    const source = "fn smaller(a: int, b: int) -> int = match a <= b {\n  true => a\n  false => b\n}\nlet chosen = smaller(1, 2)\n";
    const expected = source.replace("a: int, b: int", "a, b");
    const editor = await openSource("jointly-safe.osp", source);
    await diagnostics(editor.document, [
      inline("redundant type annotation on parameter `a` of `smaller`: inference derives `int` without it", ": int", 0),
      inline("redundant type annotation on parameter `b` of `smaller`: inference derives `int` without it", ": int", 1),
    ]);
    assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, "-> int")), []);
    const fixes = await actions(editor.document, rangeOf(editor.document, "smaller"), fixAllKind);
    assert.strictEqual(fixes.length, 1);
    assert.strictEqual(fixes[0].command?.arguments?.[0].edits.length, 2, "Only the two parameter annotations are deleted");
    assertEdit(fixes[0], editor.document, expected);
    await invokeFix(editor, rangeOf(editor.document, "smaller"), expected, fixAllKind.value);
    await diagnostics(editor.document, []);
    assert.ok(editor.document.getText().includes("-> int"));
    assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, "-> int"), fixAllKind), []);
    await save(editor.document);
  });

  test("inline Unicode, locals and lambdas are edited together while an unused parameter warning survives", async () => {
    const source = '// 🦅é\nfn greet(name: string, ignored) -> string = { let prefix: string = "hi "\nlet decorate = fn(value: string) => prefix + value\ndecorate(name) }\nfn main() = print(greet("Ada", 7))\n';
    const expected = '// 🦅é\nfn greet(name, ignored) = { let prefix = "hi "\nlet decorate = fn(value) => prefix + value\ndecorate(name) }\nfn main() = print(greet("Ada", 7))\n';
    const editor = await openSource("mixed-warnings.osp", source);
    const unused = { code: "unused-parameter", message: "unused parameter `ignored`", text: "ignored", unnecessary: true };
    const warnings = [
      inline("redundant type annotation on parameter `name` of `greet`: inference derives `string` without it", ": string", 0),
      inline("redundant return type annotation on `greet`: inference derives `string` without it", "-> string"),
      inline("redundant type annotation on `prefix`: inference derives `string` without it", ": string", 1),
      inline("redundant type annotation on parameter `value` of `<lambda>`: inference derives `string` without it", ": string", 2),
      unused,
    ];
    await diagnostics(editor.document, warnings);
    const fixes = await actions(editor.document, rangeOf(editor.document, "greet"), fixAllKind);
    assert.strictEqual(fixes.length, 1);
    assert.strictEqual(fixes[0].command?.arguments?.[0].edits.length, 4, "Fix-all deletes only the four removable annotations");
    assertEdit(fixes[0], editor.document, expected);
    await invokeFix(editor, rangeOf(editor.document, "greet"), expected, fixAllKind.value);
    await diagnostics(editor.document, [unused]);
    assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, "ignored")), []);
    await save(editor.document);
  });

  test("editing a redundant body into a load-bearing one withdraws the fix before invocation", async () => {
    const editor = await openSource("edited-signature.ospml", `${greetHeader}\n${greetBody}`);
    const [warning] = await diagnostics(editor.document, [signature("greet", "(string) -> string", greetHeader)]);
    const cached = await actions(editor.document, warning.range);
    assert.strictEqual(cached.length, 1);
    await replace(editor, '"hi " + name', "name");
    await diagnostics(editor.document, []);
    assert.deepStrictEqual(await actions(editor.document, warning.range), []);
    assert.deepStrictEqual(await actions(editor.document, warning.range, fixAllKind), []);
    const changed = editor.document.getText();
    assert.ok(cached[0].command);
    assert.strictEqual(await vscode.commands.executeCommand(cached[0].command.command,
      ...(cached[0].command.arguments ?? [])), false, "A cached pre-edit action must reject the newer document");
    assert.strictEqual(editor.document.getText(), changed);
    assert.ok(editor.document.getText().includes(greetHeader));
    await save(editor.document);
  });

  test("the required smaller signature has no warning or destructive action", async () => {
    const source = "smaller : int -> int -> int\nsmaller a b = match a <= b\n    true => a\n    false => b\nmain () = print (smaller 1 2)\n";
    const editor = await openSource("required-smaller.ospml", source);
    await diagnostics(editor.document, []);
    assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, "smaller : int -> int -> int")), []);
    assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, "smaller"), fixAllKind), []);
    assert.strictEqual(editor.document.getText(), source);
    assert.strictEqual(editor.document.isDirty, false);
  });

  test("an unsaved sibling change invalidates cached and newly requested project fixes", async () => {
    const project = path.join(process.env.OSPREY_VSIX_TEST_ROOT ?? "", "workspace", "project");
    fs.mkdirSync(path.join(project, "src"), { recursive: true });
    fs.writeFileSync(path.join(project, "osprey.toml"), '[project]\nname = "warnings"\nsource_roots = ["src"]\ndefault_namespace = "warning_project"\nentry = "src/main.ospml"\n');
    const helper = 'decorate text = text + "!"\n';
    const source = `${greetHeader}\ngreet name = decorate name\nmain () = print (greet "Ada")\n`;
    fs.writeFileSync(path.join(project, "src", "helper.ospml"), helper);
    fs.writeFileSync(path.join(project, "src", "main.ospml"), source);
    const sibling = await openSource("project/src/helper.ospml", helper);
    await diagnostics(sibling.document, []);
    const editor = await openSource("project/src/main.ospml", source);
    const [warning] = await diagnostics(editor.document, [signature("warning_project::greet", "(string) -> string", greetHeader)]);
    const cached = await actions(editor.document, warning.range);
    assert.strictEqual(cached.length, 1);
    assert.ok(cached[0].command);
    const siblingEditor = await vscode.window.showTextDocument(sibling.document);
    await replace(siblingEditor, 'text + "!"', "text");
    await diagnostics(sibling.document, []);
    await diagnostics(editor.document, []);
    assert.strictEqual(await vscode.commands.executeCommand(cached[0].command.command, ...(cached[0].command.arguments ?? [])), false);
    assert.strictEqual(editor.document.getText(), source, "Sibling edits never permit an old annotation deletion");
    assert.deepStrictEqual(await actions(editor.document, warning.range), [], "Current project proof must use the unsaved sibling");
    assert.deepStrictEqual(await actions(editor.document, warning.range, fixAllKind), []);
    await vscode.commands.executeCommand("undo");
    await waitFor(() => sibling.document.getText(), (text) => text === helper, "Undo restores the sibling's concrete inference");
    await diagnostics(editor.document, [signature("warning_project::greet", "(string) -> string", greetHeader)]);
    const refreshed = await actions(editor.document, warning.range);
    assert.strictEqual(refreshed.length, 1);
    await vscode.commands.executeCommand("redo");
    await waitFor(() => sibling.document.getText(), (text) => text === helper.replace('text + "!"', "text"), "Redo changes the sibling again");
    await diagnostics(editor.document, []);
    assert.ok(refreshed[0].command);
    assert.strictEqual(await vscode.commands.executeCommand(refreshed[0].command.command, ...(refreshed[0].command.arguments ?? [])), false);
    assert.strictEqual(editor.document.getText(), source);
    await save(sibling.document);
    await save(editor.document);
  });

  test("declared generic binders and effect contracts never receive signature deletion actions", async () => {
    const sources = [
      ["required-generics.ospml", "identity<T> : T -> T\nidentity x = x\nresult = identity<int> 7\n", "identity<T> : T -> T"],
      ["required-effects.ospml", "effect Trace\n    read : Unit => int\ntraced : Unit -> int ! Trace\ntraced () = perform Trace.read ()\nmain () = handle Trace\n    read => 7\nin print (traced ())\n", "traced : Unit -> int ! Trace"],
    ];
    for (const [name, source, header] of sources) {
      const editor = await openSource(name, source);
      await diagnostics(editor.document, []);
      assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, header)), []);
      assert.deepStrictEqual(await actions(editor.document, rangeOf(editor.document, header), fixAllKind), []);
      assert.strictEqual(editor.document.getText(), source);
    }
  });

  test("a closed project sibling changed on disk is rechecked when a cached quick fix runs", async () => {
    const project = path.join(process.env.OSPREY_VSIX_TEST_ROOT ?? "", "workspace", "closed-project");
    fs.mkdirSync(path.join(project, "src"), { recursive: true });
    fs.writeFileSync(path.join(project, "osprey.toml"), '[project]\nname = "closed"\nsource_roots = ["src"]\ndefault_namespace = "closed_project"\nentry = "src/main.ospml"\n');
    const helperPath = path.join(project, "src", "helper.ospml");
    const helper = 'decorate text = text + "!"\n';
    fs.writeFileSync(helperPath, helper);
    const source = `${greetHeader}\ngreet name = decorate name\nmain () = print (greet "Ada")\n`;
    const editor = await openSource("closed-project/src/main.ospml", source);
    const [warning] = await diagnostics(editor.document, [signature("closed_project::greet", "(string) -> string", greetHeader)]);
    const cached = await actions(editor.document, warning.range);
    assert.strictEqual(cached.length, 1);
    assert.ok(cached[0].command);
    const version = editor.document.version;
    assert.ok(!vscode.workspace.textDocuments.some((document) => document.uri.fsPath === helperPath));
    fs.writeFileSync(helperPath, "decorate text = text\n");
    assert.strictEqual(await vscode.commands.executeCommand(cached[0].command.command, ...(cached[0].command.arguments ?? [])), false);
    assert.strictEqual(editor.document.version, version, "Target version did not change with its closed sibling");
    assert.strictEqual(editor.document.getText(), source);
    assert.strictEqual(editor.document.isDirty, false);
    assert.deepStrictEqual(await actions(editor.document, warning.range), []);
    assert.deepStrictEqual(await actions(editor.document, warning.range, fixAllKind), []);
    fs.writeFileSync(helperPath, helper);
    const refreshed = await actions(editor.document, warning.range);
    assert.strictEqual(refreshed.length, 1, "Restoring the dependency restores the compiler proof");
    assert.ok(refreshed[0].command);
    assert.strictEqual(await vscode.commands.executeCommand(refreshed[0].command.command, ...(refreshed[0].command.arguments ?? [])), true);
    assert.strictEqual(editor.document.getText(), source.replace(`${greetHeader}\n`, ""));
    await diagnostics(editor.document, []);
    await save(editor.document);
  });

  test("manifest source-root changes revalidate both cached quick fixes and fix-all against the new project", async () => {
    const project = path.join(process.env.OSPREY_VSIX_TEST_ROOT ?? "", "workspace", "manifest-project");
    for (const directory of ["src", "concrete", "generic"]) fs.mkdirSync(path.join(project, directory), { recursive: true });
    const manifestPath = path.join(project, "osprey.toml");
    const manifest = '[project]\nname = "manifest"\nsource_roots = ["src", "concrete"]\ndefault_namespace = "manifest_project"\nentry = "src/main.ospml"\n';
    fs.writeFileSync(manifestPath, manifest);
    fs.writeFileSync(path.join(project, "concrete", "helper.ospml"), 'decorate text = text + "!"\n');
    fs.writeFileSync(path.join(project, "generic", "helper.ospml"), "decorate text = text\n");
    const source = `${greetHeader}\ngreet name = decorate name\nmain () = print (greet "Ada")\n`;
    const editor = await openSource("manifest-project/src/main.ospml", source);
    const [warning] = await diagnostics(editor.document, [signature("manifest_project::greet", "(string) -> string", greetHeader)]);
    const quick = await actions(editor.document, warning.range);
    const all = await actions(editor.document, warning.range, fixAllKind);
    assert.strictEqual(quick.length, 1);
    assert.strictEqual(all.length, 1);
    const version = editor.document.version;
    assert.ok(!vscode.workspace.textDocuments.some((document) => document.uri.fsPath === manifestPath));
    fs.writeFileSync(manifestPath, manifest.replace('"concrete"', '"generic"'));
    for (const action of [quick[0], all[0]]) {
      assert.ok(action.command);
      assert.strictEqual(await vscode.commands.executeCommand(action.command.command, ...(action.command.arguments ?? [])), false);
      assert.strictEqual(editor.document.version, version);
      assert.strictEqual(editor.document.getText(), source);
      assert.strictEqual(editor.document.isDirty, false);
    }
    assert.deepStrictEqual(await actions(editor.document, warning.range), []);
    assert.deepStrictEqual(await actions(editor.document, warning.range, fixAllKind), []);
    fs.writeFileSync(manifestPath, manifest);
    const restored = await actions(editor.document, warning.range, fixAllKind);
    assert.strictEqual(restored.length, 1);
    assertEdit(restored[0], editor.document, source.replace(`${greetHeader}\n`, ""));
    await invokeFix(editor, warning.range, source.replace(`${greetHeader}\n`, ""), fixAllKind.value);
    await diagnostics(editor.document, []);
    await save(editor.document);
  });
});

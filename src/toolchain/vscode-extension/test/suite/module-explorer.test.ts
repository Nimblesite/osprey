import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import {
  clientPackageGraph, compilerSourceGraph, isContained, ModuleExplorerProvider, openSource, parsePackageGraph,
  parseSourceGraph, projectChildren, type ModuleExplorerDeps, type ModuleNode, type PackageGraph,
} from "../../client/src/module-explorer";
import { runFrontendCommand } from "../../client/src/package-frontend";
import { extensionRoot, present, resolveBuiltOsprey, uriPath, writeStubExecutable } from "./osprey-test-env";
import { activateWithServer, connectEditorBrowser } from "./profiler-kit";

const labels = (nodes: readonly ModuleNode[]) => nodes.map((node) => node.label);

function project(files: Record<string, string>): string {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "osprey-modules-")));
  for (const [file, text] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    fs.writeFileSync(path.join(root, file), text);
  }
  return root;
}

const fileEditors = () => vscode.window.visibleTextEditors
  .filter((editor) => editor.document.uri.scheme === "file")
  .map((editor) => editor.document.uri.fsPath);

const LOCKED = '{"key":"@org/left","version":"1.0.0","present":true,"dependencies":[],"findings":[]}';
const MANIFEST = '[project]\nname = "demo"\nsource_roots = ["src"]\nentry = "src/main.ospml"\n';

suite("Module Explorer [EDITOR-MODULE-EXPLORER]", () => {
  const compiler = () => {
    const built = resolveBuiltOsprey();
    assert.ok(built, "build the Osprey compiler before this suite");
    return built;
  };
  const NO_LOCK = (direct: readonly string[]): PackageGraph => ({ direct, packages: [], findings: ["tree.no-lock"] });
  const provider = (roots: string[], packages: readonly string[] | string, overrides: Partial<ModuleExplorerDeps> = {}) =>
    new ModuleExplorerProvider({
      projectRoots: () => roots,
      sourceGraph: compilerSourceGraph(compiler),
      packageGraph: async () => (typeof packages === "string" ? { error: packages } : { ok: NO_LOCK(packages) }),
      plainSources: async () => [],
      hasUnsavedEdits: () => false,
      trusted: () => true,
      ...overrides,
    });

  test("keyboard navigation selects projects, expands groups and returns to the parent", async function () {
    this.timeout(60000);
    await activateWithServer();
    const browser = await connectEditorBrowser();
    try {
      await vscode.commands.executeCommand("osprey.moduleExplorer.focus");
      const page = browser.contexts().flatMap(context => context.pages()).find(page => page.url().includes("workbench"));
      assert.ok(page, "real VS Code workbench");
      const first = page.getByRole("treeitem", { name: /^first(?:,| |$)/ });
      const second = page.getByRole("treeitem", { name: /^second(?:,| |$)/ });
      await first.waitFor();
      await second.waitFor();
      assert.strictEqual(await page.getByRole("treeitem", { name: /^discipline(?:,| |$)/ }).count(), 0,
        "a manifest with no [project] table is a package, never a project");
      await page.keyboard.press("Home");
      await page.keyboard.press("ArrowDown");
      await second.and(page.locator(".focused")).waitFor();
      await page.keyboard.press("ArrowUp");
      await first.and(page.locator(".focused")).waitFor();
      await page.keyboard.press("ArrowRight");
      await first.and(page.locator('[aria-expanded="true"]')).waitFor();
      const tree = page.getByRole("tree").filter({ has: first });
      const children = tree.getByRole("treeitem", { level: 2 });
      await children.first().waitFor({ timeout: 15000 });
      await page.keyboard.press("ArrowDown");
      await children.and(page.locator(".focused")).waitFor();
      await page.keyboard.press("ArrowLeft");
      await first.and(page.locator(".focused")).waitFor();
      await page.keyboard.press("ArrowLeft");
      await first.and(page.locator('[aria-expanded="false"]')).waitFor();
    } finally { await browser.close(); }
  });

  test("the real compiler's code tree: namespace, module and members with coloured icons and the editor's hover, across flavors", async () => {
    const root = project({
      "osprey.toml": MANIFEST,
      "src/main.ospml": 'namespace app\n\nmodule Outer\n    export module Inner\n        export one = 1\n    export keep x = x\n\neffect Counter\n    increment : int => Unit\n\nlimit = 3\n\nprint "${Outer::Inner::one} ${Outer::keep limit}"\n',
      "src/sub/deep.osp": "namespace app;\n\nmodule Deep {\n    export fn three() = 3\n}\n",
      "src/more.osp": "namespace app;\n\n\nmodule Extra {\n    export fn two() = 2\n    export type Shape = Circle | Square\n}\n\nextern fn strlen(s: string) -> int\n\nlet total = 0\n",
    });
    const tree = provider([root], ["@org/left", "@org/right"]);
    const [node] = await tree.getChildren();
    assert.ok(node, "the project node");
    assert.strictEqual(node.label, path.basename(root));
    assert.strictEqual(tree.getTreeItem(node).collapsibleState, vscode.TreeItemCollapsibleState.Collapsed);

    const groups = await tree.getChildren(node);
    assert.deepStrictEqual(labels(groups), ["Namespaces", "Sources", "Dependencies"]);
    const [modules, sources, dependencies] = groups;
    assert.ok(modules && sources && dependencies, "the three groups");
    const child = async (parent: ModuleNode, label: string) => {
      const found = (await tree.getChildren(parent)).find((item) => item.label === label);
      assert.ok(found, `${label} under ${parent.label}`);
      return found;
    };
    const icon = (item: ModuleNode) => {
      const shown = tree.getTreeItem(item).iconPath as vscode.ThemeIcon;
      return [shown.id, shown.color?.id];
    };

    // Namespaces -> namespace -> module -> members: a module sits inside its namespace, never the reverse; folders and files are not code
    // elements, so a namespace three files in two folders share is one node.
    assert.deepStrictEqual(labels(await tree.getChildren(modules)), ["app"]);
    const mainApp = await child(modules, "app");
    assert.strictEqual(tree.getParent(mainApp), modules);
    const moreApp = mainApp;
    assert.deepStrictEqual(icon(mainApp), ["symbol-namespace", "symbolIcon.namespaceForeground"]);
    assert.deepStrictEqual(labels(await tree.getChildren(mainApp)), ["Counter", "Deep", "Extra", "limit", "Outer", "strlen", "total"]);
    const shown = async (parent: ModuleNode): Promise<ModuleNode[]> => {
      const kids = await tree.getChildren(parent);
      return [...kids, ...(await Promise.all(kids.map(shown))).flat()];
    };
    assert.ok((await shown(modules)).every((item) => item.icon?.startsWith("symbol-")), "only code elements in the code tree");
    const deep = await child(mainApp, "Deep");
    assert.deepStrictEqual(labels(await tree.getChildren(deep)), ["three"]);
    assert.strictEqual(deep.location?.file, path.join(root, "src", "sub", "deep.osp"));
    const outer = await child(mainApp, "Outer");
    assert.deepStrictEqual(icon(outer), ["symbol-structure", "symbolIcon.moduleForeground"]);
    assert.notStrictEqual(icon(outer)[0], icon(mainApp)[0], "a module never wears the namespace glyph");
    assert.strictEqual(modules.icon, "symbol-namespace", "the group holds namespaces and says so");
    assert.deepStrictEqual(labels(await tree.getChildren(outer)), ["Inner", "keep"]);
    const inner = await child(outer, "Inner");
    assert.deepStrictEqual(labels(await tree.getChildren(inner)), ["one"]);
    assert.deepStrictEqual(icon(await child(inner, "one")), ["symbol-variable", "symbolIcon.variableForeground"]);
    assert.deepStrictEqual(icon(await child(outer, "keep")), ["symbol-function", "symbolIcon.functionForeground"]);
    assert.deepStrictEqual(icon(await child(mainApp, "Counter")), ["symbol-event", "symbolIcon.eventForeground"]);
    assert.strictEqual((await child(mainApp, "Counter")).location?.line, 8);
    assert.strictEqual(tree.getTreeItem(await child(inner, "one")).collapsibleState, vscode.TreeItemCollapsibleState.None);

    const extra = await child(moreApp, "Extra");
    assert.deepStrictEqual(labels(await tree.getChildren(extra)), ["Shape", "two"]);
    assert.deepStrictEqual(icon(await child(extra, "Shape")), ["symbol-class", "symbolIcon.classForeground"]);
    assert.deepStrictEqual(icon(await child(moreApp, "strlen")), ["symbol-interface", "symbolIcon.interfaceForeground"]);
    assert.strictEqual(extra.location?.file, path.join(root, "src", "more.osp"));
    assert.strictEqual(extra.location?.line, 4);
    assert.strictEqual(tree.getParent(extra), moreApp);
    assert.strictEqual(tree.getParent(moreApp), modules);

    // The sort button cycles alphabetical -> source order -> kind, as SharpLsp's does.
    assert.strictEqual(tree.sortOrder, "alphabetical");
    assert.strictEqual(tree.nextSort(), "source");
    assert.deepStrictEqual(labels(await tree.getChildren(mainApp)), ["Outer", "Counter", "limit", "Extra", "strlen", "total", "Deep"]);
    assert.deepStrictEqual(labels(await tree.getChildren(outer)), ["Inner", "keep"]);
    assert.strictEqual(tree.nextSort(), "kind");
    assert.deepStrictEqual(labels(await tree.getChildren(mainApp)), ["Deep", "Extra", "Outer", "Counter", "strlen", "limit", "total"]);
    assert.deepStrictEqual(labels(await tree.getChildren(extra)), ["Shape", "two"]);
    assert.strictEqual(tree.nextSort(), "alphabetical");
    assert.deepStrictEqual(labels(await tree.getChildren(mainApp)), ["Counter", "Deep", "Extra", "limit", "Outer", "strlen", "total"]);

    // The filter keeps what matches, case-insensitively, and the scopes that lead to it.
    tree.filterBy("THR");
    assert.strictEqual(tree.filterText, "thr");
    assert.deepStrictEqual(labels(await tree.getChildren(modules)), ["app"]);
    assert.deepStrictEqual(labels(await tree.getChildren(mainApp)), ["Deep"]);
    assert.deepStrictEqual(labels(await tree.getChildren(deep)), ["three"]);
    tree.filterBy("o");
    assert.deepStrictEqual(labels(await tree.getChildren(mainApp)), ["Counter", "Extra", "Outer", "total"], "Extra stays for its member two");
    assert.deepStrictEqual(labels(await tree.getChildren(outer)), ["Inner"]);
    tree.nextSort();
    assert.deepStrictEqual(labels(await tree.getChildren(mainApp)), ["Outer", "Counter", "Extra", "total"], "sort applies to filtered rows");
    tree.nextSort();
    tree.nextSort();
    tree.filterBy("zzz");
    const none = present((await tree.getChildren(modules))[0], "the no-matches row");
    assert.deepStrictEqual([none.label, none.description, none.icon], ["No matches", "zzz", "info"], "a filter that hides all says so");
    tree.filterBy("  ");
    assert.strictEqual(tree.filterText, "");
    assert.deepStrictEqual(labels(await tree.getChildren(mainApp)), ["Counter", "Deep", "Extra", "limit", "Outer", "strlen", "total"]);
    assert.deepStrictEqual(labels(await tree.getChildren(node)), ["Namespaces", "Sources", "Dependencies"], "the filter leaves groups alone");
    assert.strictEqual((await child(moreApp, "strlen")).location?.file, path.join(root, "src", "more.osp"));
    assert.strictEqual((await child(mainApp, "limit")).location?.file, path.join(root, "src", "main.ospml"));

    // A member's tree hover is the editor's own hover at its name, word for word.
    await activateWithServer();
    const registered = await vscode.commands.getCommands(true);
    const manifest = JSON.parse(fs.readFileSync(path.join(extensionRoot, "package.json"), "utf8"));
    const titleBar = manifest.contributes.menus["view/title"].filter((entry: { when: string }) =>
      entry.when.includes("view == osprey.moduleExplorer"));
    for (const [command, when] of [
      ["osprey.modules.sortAlphabetical", "osprey.modules.sortOrder == alphabetical"],
      ["osprey.modules.sortSource", "osprey.modules.sortOrder == source"],
      ["osprey.modules.sortKind", "osprey.modules.sortOrder == kind"],
      ["osprey.modules.filter", "view == osprey.moduleExplorer"],
      ["osprey.modules.clearFilter", "osprey.modules.filtered"],
    ]) {
      assert.ok(command && when, "a title-bar command and its when clause");
      assert.ok(registered.includes(command), `${command} is registered`);
      assert.ok(titleBar.some((entry: { command: string; when: string }) => entry.command === command && entry.when.includes(when)),
        `${command} is a title-bar button shown when ${when}`);
    }
    for (const [scope, name, line] of [[outer, "keep", 5], [extra, "two", 4], [mainApp, "limit", 10]] as const) {
      const member = await child(scope, name);
      const document = await vscode.workspace.openTextDocument(member.location?.file ?? "");
      const at = new vscode.Position(line, document.lineAt(line).text.indexOf(name));
      const editor = await vscode.commands.executeCommand<vscode.Hover[]>("vscode.executeHoverProvider", document.uri, at);
      const expected = editor.flatMap((hover) => hover.contents)
        .map((part) => (typeof part === "string" ? part : part.value)).join("\n\n---\n\n");
      assert.ok(expected.includes(name), `the editor hovers ${name}: ${expected}`);
      const resolved = await tree.resolveTreeItem(tree.getTreeItem(member), member);
      assert.strictEqual((resolved.tooltip as vscode.MarkdownString | undefined)?.value, expected, name);
    }

    assert.deepStrictEqual(labels(await tree.getChildren(sources)), [path.join("src", "main.ospml"), path.join("src", "more.osp"), path.join("src", "sub", "deep.osp")]);
    assert.deepStrictEqual(labels(await tree.getChildren(dependencies)), ["No lock file", "@org/left", "@org/right"]);
    assert.strictEqual((await tree.sourceNode(path.join(root, "src", "more.osp")))?.parent, sources);

    // Selecting a module opens its file at its declaration.
    const command = tree.getTreeItem(extra).command;
    assert.strictEqual(command?.command, "osprey.modules.open");
    await openSource(command.arguments?.[0], [root]);
    const editor = vscode.window.activeTextEditor;
    assert.strictEqual(editor?.document.uri.fsPath, uriPath(path.join(root, "src", "more.osp")));
    assert.strictEqual(editor?.selection.active.line, 3);

    // A refresh after an edit answers the new membership, and the ids stay put.
    fs.writeFileSync(path.join(root, "src", "more.osp"), "namespace app;\n\nmodule Renamed {\n    export fn two() = 2\n}\n");
    tree.refresh();
    const again = await tree.getChildren(node);
    const againRoot = present(again[0], "the refreshed project node");
    assert.strictEqual(againRoot.id, modules.id);
    const againApp = await child(againRoot, "app");
    assert.strictEqual(againApp.id, mainApp.id);
    assert.deepStrictEqual(labels(await tree.getChildren(againApp)), ["Counter", "Deep", "limit", "Outer", "Renamed"]);
    await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
    // Windows holds the just-closed editor's file for a moment.
    fs.rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  });

  test("failures show as failures, never as an empty project", async () => {
    const broken = project({ "osprey.toml": MANIFEST, "src/main.ospml": "module\n" });
    const tree = provider([broken], "The package client is not installed.");
    const [node] = await tree.getChildren();
    const groups = await tree.getChildren(node);
    assert.deepStrictEqual(labels(groups), ["Analysis failed", "Dependencies"]);
    const failedGroup = present(groups[0], "the failed group");
    assert.ok(failedGroup.description?.includes("main.ospml"), failedGroup.description);
    const [state] = await tree.getChildren(present(groups[1], "the Dependencies group"));
    assert.ok(state, "the dependency state row");
    assert.strictEqual(state.label, "Dependencies unavailable");
    assert.strictEqual(state.description, "The package client is not installed.");

    const missing = await compilerSourceGraph(() => path.join(broken, "no-such-compiler"))(broken);
    assert.ok("error" in missing && missing.error.includes("ENOENT"), "a missing compiler is named by its spawn error");
    const silent = writeStubExecutable(path.join(broken, "silent-compiler"), "");
    assert.deepStrictEqual(await compilerSourceGraph(() => silent)(broken), { error: "The compiler answered no source graph." },
      "a compiler that succeeds without a graph is a failure, not an empty project");
    fs.rmSync(broken, { recursive: true, force: true });

    // An untrusted workspace runs nothing, says why, and can still navigate its files.
    let ran = false;
    const untrusted = provider(["/project"], [], {
      sourceGraph: async () => { ran = true; return { error: "ran" }; },
      packageGraph: async () => { ran = true; return { error: "ran" }; },
      plainSources: async () => ["/project/b.osp", "/project/a.osp"],
      trusted: () => false,
    });
    const [only] = await untrusted.getChildren();
    assert.ok(only, "the untrusted project node");
    const shown = await untrusted.getChildren(only);
    assert.deepStrictEqual(labels(shown), ["Analysis needs a trusted workspace", "Sources"]);
    assert.deepStrictEqual(labels(await untrusted.getChildren(shown[1])), ["a.osp", "b.osp"]);
    assert.strictEqual(ran, false);
    const bare: ModuleNode = { id: "/project/bare", label: "bare" };
    const leaf = untrusted.getTreeItem(bare);
    assert.deepStrictEqual([leaf.collapsibleState, leaf.iconPath, leaf.contextValue, leaf.command],
      [vscode.TreeItemCollapsibleState.None, undefined, undefined, undefined], "a bare node is an inert leaf");
    assert.deepStrictEqual(await untrusted.getChildren(bare), [], "and has no children");

    // Unsaved edits are stated, not hidden behind a clean answer.
    const stale = project({ "osprey.toml": MANIFEST, "src/main.ospml": 'print "x"\n' });
    const dirty = provider([stale], [], { hasUnsavedEdits: () => true });
    const [staleNode] = await dirty.getChildren();
    assert.strictEqual(labels(await dirty.getChildren(staleNode))[0], "Unsaved edits are not analysed");
    fs.rmSync(stale, { recursive: true, force: true });
  });

  test("navigation stays inside the authorized roots, through symlinks too", async () => {
    const inside = project({ "a.osp": "print(1)\n" });
    const outside = project({ "secret.osp": "print(2)\n" });
    fs.symlinkSync(path.join(outside, "secret.osp"), path.join(inside, "link.osp"));
    assert.ok(isContained(path.join(inside, "a.osp"), [inside]));
    assert.ok(!isContained(path.join(outside, "secret.osp"), [inside]));
    assert.ok(!isContained(path.join(inside, "link.osp"), [inside]));
    assert.ok(!isContained(path.join(inside, "..", path.basename(outside), "secret.osp"), [inside]));
    assert.ok(!isContained(path.join(inside, "missing.osp"), [inside]));
    assert.ok(!isContained(inside, [inside]));

    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    await openSource({ file: path.join(outside, "secret.osp"), line: 1, column: 0 }, [inside]);
    assert.deepStrictEqual(fileEditors(), [], "an escape opens nothing");
    await openSource({ file: path.join(inside, "a.osp"), line: 1, column: 0 }, [inside]);
    assert.deepStrictEqual(fileEditors(), [uriPath(path.join(inside, "a.osp"))]);
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    fs.rmSync(inside, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
    fs.rmSync(outside, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
  });

  test("the real package client answers per project, and a manifest change shows after a refresh", async () => {
    const client = path.resolve(extensionRoot, "..", "..", "..", "target", process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg");
    assert.ok(fs.existsSync(client), "build target/osprey-pkg before this suite");
    const frontend = { run: (command: string, cwd: string) => runFrontendCommand(client, command, cwd) };
    const manifest = (deps: string) => `[project]\nname = "demo"\nsource_roots = ["src"]\ndefault_namespace = "demo"\n\n[dependencies]\n${deps}`;
    const source = { "src/main.ospml": 'print "x"\n' };
    const first = project({ ...source, "osprey.toml": manifest("") });
    const second = project({ ...source, "osprey.toml": manifest('"@org/other" = { epoch = 1 }\n') });
    const tree = provider([first, second], [], { packageGraph: clientPackageGraph(frontend) });
    const dependencies = async (node: ModuleNode) => {
      const groups = await tree.getChildren(node);
      return tree.getChildren(groups.find((group) => group.label === "Dependencies"));
    };

    // Two projects in one workspace answer independently.
    const [one, two] = await tree.getChildren();
    assert.ok(one && two, "both projects");
    assert.deepStrictEqual(labels(await dependencies(one)), ["No lock file"]);
    assert.deepStrictEqual(labels(await dependencies(two)), ["No lock file", "@org/other"]);
    assert.strictEqual(present((await dependencies(two))[1], "the @org/other row").description, "not locked · direct");

    // An edit made outside the view shows after a refresh.
    fs.writeFileSync(path.join(first, "osprey.toml"), manifest('"@org/added" = { epoch = 2 }\n'));
    assert.deepStrictEqual(labels(await dependencies(one)), ["No lock file"], "the loaded answer holds until a refresh");
    tree.refresh();
    assert.deepStrictEqual(labels(await dependencies(one)), ["No lock file", "@org/added"]);
    assert.deepStrictEqual(labels(await dependencies(two)), ["No lock file", "@org/other"]);

    // A manifest the client rejects is that client's error, never an empty list.
    fs.writeFileSync(path.join(first, "osprey.toml"), "[project]\n");
    tree.refresh();
    const broken = present((await dependencies(one))[0], "the failed Dependencies group");
    assert.strictEqual(broken.label, "Dependencies unavailable");
    assert.ok((broken.description ?? "").length > 0);
    fs.rmSync(first, { recursive: true, force: true });
    fs.rmSync(second, { recursive: true, force: true });
  });

  test("the locked graph nests dependencies and shows what verification found", () => {
    const owner: ModuleNode = { id: "p", label: "p", projectRoot: "/p" };
    const source = { ok: { sources: ["/p/a.osp"], declarations: [] } };
    const [modules, sources, none] = projectChildren(owner, source, { ok: { direct: [], packages: [], findings: [] } });
    assert.ok(modules && sources && none, "the three groups");
    assert.deepStrictEqual(labels(modules.children ?? []), ["No declarations"], "files alone are not code elements");
    const empty = present(projectChildren(owner, { ok: { sources: [], declarations: [] } }, { ok: NO_LOCK([]) })[0], "the empty Namespaces group");
    assert.deepStrictEqual(labels(empty.children ?? []), ["No sources"]);
    assert.deepStrictEqual(labels(sources.children ?? []), ["a.osp"]);
    assert.deepStrictEqual(labels(none.children ?? []), ["No dependencies"]);

    const clean = { dependencies: [], findings: [], present: true };
    const graph: PackageGraph = {
      direct: ["@org/a", "@org/unlocked"],
      findings: ["verify.lock"],
      packages: [
        { ...clean, key: "@org/a", version: "e1.r2", dependencies: ["@org/b", "@org/gone"] },
        { ...clean, key: "@org/b", version: "e3.r0", dependencies: ["@org/a"], findings: ["verify.digest"] },
        { ...clean, key: "@org/gone", version: "e1.r0", findings: ["verify.missing"], present: false },
      ],
    };
    const dependencies = present(projectChildren(owner, source, { ok: graph })[2], "the Dependencies group");
    const [global, a, unlocked] = dependencies.children ?? [];
    assert.ok(global && a && unlocked, "the three top-level rows");
    assert.strictEqual(global.description, "verify.lock");
    assert.strictEqual(a.description, "e1.r2 · direct");
    assert.strictEqual(a.icon, "package");
    assert.strictEqual(unlocked.description, "not locked · direct");
    assert.strictEqual(unlocked.icon, "warning");
    const [b, gone] = a.children ?? [];
    assert.ok(b && gone, "both transitive rows");
    assert.strictEqual(b.description, "e3.r0 · transitive · verify.digest");
    assert.strictEqual(b.icon, "warning");
    assert.deepStrictEqual(b.children, [], "a cycle back to @org/a stops");
    assert.strictEqual(gone.description, "e1.r0 · transitive · verify.missing · not installed");
    assert.strictEqual(a.parent, dependencies);

    const local = present(projectChildren(owner, source, { ok: {
      direct: ["@org/a"], findings: [], packages: [{ ...clean, key: "@org/a", version: "e1.r2", override: "../a" }],
    } })[2], "the Dependencies group").children ?? [];
    const localA = present(local[0], "the local @org/a row");
    assert.strictEqual(localA.description, "e1.r2 · direct · local: ../a");
    assert.strictEqual(localA.icon, "package");

    const [lockState, bare] = present(projectChildren(owner, source, { ok: NO_LOCK(["@org/a"]) })[2], "the Dependencies group").children ?? [];
    assert.ok(lockState && bare, "the lock-state row and the bare dependency");
    assert.strictEqual(lockState.label, "No lock file");
    assert.strictEqual(bare.description, "not locked · direct");
    const failed = present(projectChildren(owner, source, { error: "lock.corrupt" })[2], "the Dependencies group").children ?? [];
    assert.deepStrictEqual(labels(failed), ["Dependencies unavailable"]);
    assert.strictEqual(present(failed[0], "the failed row").description, "lock.corrupt");
  });

  test("a graph or dependency report that is not complete is refused", async () => {
    for (const text of ["", "not json", "{}", '{"sources":[]}', '{"sources":[1],"declarations":[]}',
      '{"sources":[],"declarations":[{"kind":"module"}]}']) {
      assert.strictEqual(parseSourceGraph(text), undefined, text);
    }
    assert.deepStrictEqual(parseSourceGraph('{"sources":["a"],"declarations":[]}'), { sources: ["a"], declarations: [] });

    for (const text of ["", "{}", '{"direct":[],"findings":[]}', '{"direct":[1],"findings":[],"packages":[]}',
      '{"direct":[],"findings":[],"packages":[{"key":"k"}]}', '{"direct":[],"findings":[],"packages":[null]}',
      `{"direct":[],"findings":[],"packages":[${LOCKED.replace('"present":true', '"present":"yes"')}]}`,
      `{"direct":[],"findings":[],"packages":[${LOCKED.replace('"findings":[]', '"findings":[7]')}]}`,
      `{"direct":[],"findings":[],"packages":[${LOCKED.replace("}", ',"override":1}')}]}`]) {
      assert.strictEqual(parsePackageGraph(text), undefined, text);
    }
    const overridden = JSON.parse(LOCKED.replace("}", ',"override":"../left"}'));
    assert.deepStrictEqual(parsePackageGraph(`{"direct":["@org/left"],"findings":[],"packages":[${JSON.stringify(overridden)}]}`)?.packages, [overridden]);
    const report = '{"direct":[],"findings":["tree.no-lock"],"packages":[],"project":"p"}';
    const answering = (status: number, lines: string[]) =>
      clientPackageGraph({ run: async () => ({ status, lines }) as never })("/p");
    assert.deepStrictEqual(await answering(0, [report]), { ok: { direct: [], findings: ["tree.no-lock"], packages: [] } });
    assert.deepStrictEqual(await answering(1, ["manifest.bad at 1:1"]), { error: "manifest.bad at 1:1" });
    assert.deepStrictEqual(await answering(0, ["not json"]), { error: "not json" });
    assert.deepStrictEqual(await answering(2, []), { error: "The package client answered status 2." });
    const thrown = await clientPackageGraph({ run: async () => { throw new Error("spawn failed"); } })("/p");
    assert.deepStrictEqual(thrown, { error: "spawn failed" });
    const rejected = await clientPackageGraph({ run: () => Promise.reject("frontend vanished") })("/p");
    assert.deepStrictEqual(rejected, { error: "frontend vanished" }, "a non-Error rejection is still reported");
  });
});

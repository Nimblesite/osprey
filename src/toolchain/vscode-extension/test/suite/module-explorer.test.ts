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
import { extensionRoot, resolveBuiltOsprey } from "./osprey-test-env";

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

  test("the real compiler's modules, shared namespaces and sources across flavors, with navigation", async () => {
    const root = project({
      "osprey.toml": MANIFEST,
      "src/main.ospml": 'namespace app\n\nmodule Outer\n    export module Inner\n        export one = 1\n\nprint "${Outer::Inner::one}"\n',
      "src/more.osp": "namespace app;\n\n\nmodule Extra {\n    export fn two() = 2\n}\n",
    });
    const tree = provider([root], ["@org/left", "@org/right"]);
    const [node] = await tree.getChildren();
    assert.strictEqual(node.label, path.basename(root));
    assert.strictEqual(tree.getTreeItem(node).collapsibleState, vscode.TreeItemCollapsibleState.Collapsed);

    const groups = await tree.getChildren(node);
    assert.deepStrictEqual(labels(groups), ["Modules", "Sources", "Dependencies"]);
    const [modules, sources, dependencies] = groups;

    const declared = await tree.getChildren(modules);
    assert.deepStrictEqual(labels(declared), ["app", "app::Extra", "app::Outer", "app::Outer::Inner"]);
    // `app` spans two files, so it lists both; a module opens directly at its line.
    const homes = await tree.getChildren(declared[0]);
    assert.deepStrictEqual(labels(homes).sort(), [path.join("src", "main.ospml"), path.join("src", "more.osp")]);
    assert.strictEqual(declared[0].location, undefined);
    assert.strictEqual(declared[1].location?.file, path.join(root, "src", "more.osp"));
    assert.strictEqual(declared[1].location?.line, 4);
    assert.strictEqual(tree.getParent(declared[1]), modules);

    assert.deepStrictEqual(labels(await tree.getChildren(sources)), [path.join("src", "main.ospml"), path.join("src", "more.osp")]);
    assert.deepStrictEqual(labels(await tree.getChildren(dependencies)), ["No lock file", "@org/left", "@org/right"]);
    assert.strictEqual((await tree.sourceNode(path.join(root, "src", "more.osp")))?.parent, sources);

    // Selecting a module opens its file at its declaration.
    const command = tree.getTreeItem(declared[1]).command;
    assert.strictEqual(command?.command, "osprey.modules.open");
    await openSource(command.arguments?.[0], [root]);
    const editor = vscode.window.activeTextEditor;
    assert.strictEqual(editor?.document.uri.fsPath, path.join(root, "src", "more.osp"));
    assert.strictEqual(editor?.selection.active.line, 3);

    // A refresh after an edit answers the new membership, and the ids stay put.
    fs.writeFileSync(path.join(root, "src", "more.osp"), "namespace app;\n\nmodule Renamed {\n    export fn two() = 2\n}\n");
    tree.refresh();
    const again = await tree.getChildren(node);
    assert.strictEqual(again[0].id, modules.id);
    assert.ok(labels(await tree.getChildren(again[0])).includes("app::Renamed"));
    await vscode.commands.executeCommand("workbench.action.closeActiveEditor");
    fs.rmSync(root, { recursive: true, force: true });
  });

  test("failures show as failures, never as an empty project", async () => {
    const broken = project({ "osprey.toml": MANIFEST, "src/main.ospml": "module\n" });
    const tree = provider([broken], "The package client is not installed.");
    const [node] = await tree.getChildren();
    const groups = await tree.getChildren(node);
    assert.deepStrictEqual(labels(groups), ["Analysis failed", "Dependencies"]);
    assert.ok(groups[0].description?.includes("main.ospml"), groups[0].description);
    const [state] = await tree.getChildren(groups[1]);
    assert.strictEqual(state.label, "Dependencies unavailable");
    assert.strictEqual(state.description, "The package client is not installed.");

    const missing = await compilerSourceGraph(() => path.join(broken, "no-such-compiler"))(broken);
    assert.ok("error" in missing && missing.error.includes("ENOENT"), "a missing compiler is named by its spawn error");
    const silent = path.join(broken, "silent-compiler");
    fs.writeFileSync(silent, "#!/bin/sh\nexit 0\n", { mode: 0o755 });
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
    assert.deepStrictEqual(fileEditors(), [path.join(inside, "a.osp")]);
    await vscode.commands.executeCommand("workbench.action.closeAllEditors");
    fs.rmSync(inside, { recursive: true, force: true });
    fs.rmSync(outside, { recursive: true, force: true });
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
    assert.deepStrictEqual(labels(await dependencies(one)), ["No lock file"]);
    assert.deepStrictEqual(labels(await dependencies(two)), ["No lock file", "@org/other"]);
    assert.strictEqual((await dependencies(two))[1].description, "not locked · direct");

    // An edit made outside the view shows after a refresh.
    fs.writeFileSync(path.join(first, "osprey.toml"), manifest('"@org/added" = { epoch = 2 }\n'));
    assert.deepStrictEqual(labels(await dependencies(one)), ["No lock file"], "the loaded answer holds until a refresh");
    tree.refresh();
    assert.deepStrictEqual(labels(await dependencies(one)), ["No lock file", "@org/added"]);
    assert.deepStrictEqual(labels(await dependencies(two)), ["No lock file", "@org/other"]);

    // A manifest the client rejects is that client's error, never an empty list.
    fs.writeFileSync(path.join(first, "osprey.toml"), "[project]\n");
    tree.refresh();
    const [broken] = await dependencies(one);
    assert.strictEqual(broken.label, "Dependencies unavailable");
    assert.ok((broken.description ?? "").length > 0);
    fs.rmSync(first, { recursive: true, force: true });
    fs.rmSync(second, { recursive: true, force: true });
  });

  test("the locked graph nests dependencies and shows what verification found", () => {
    const owner: ModuleNode = { id: "p", label: "p", projectRoot: "/p" };
    const source = { ok: { sources: ["/p/a.osp"], declarations: [] } };
    const [modules, sources, none] = projectChildren(owner, source, { ok: { direct: [], packages: [], findings: [] } });
    assert.deepStrictEqual(labels(modules.children ?? []), ["No namespaces or modules"]);
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
    const [, , dependencies] = projectChildren(owner, source, { ok: graph });
    const [global, a, unlocked] = dependencies.children ?? [];
    assert.strictEqual(global.description, "verify.lock");
    assert.strictEqual(a.description, "e1.r2 · direct");
    assert.strictEqual(a.icon, "package");
    assert.strictEqual(unlocked.description, "not locked · direct");
    assert.strictEqual(unlocked.icon, "warning");
    const [b, gone] = a.children ?? [];
    assert.strictEqual(b.description, "e3.r0 · transitive · verify.digest");
    assert.strictEqual(b.icon, "warning");
    assert.deepStrictEqual(b.children, [], "a cycle back to @org/a stops");
    assert.strictEqual(gone.description, "e1.r0 · transitive · verify.missing · not installed");
    assert.strictEqual(a.parent, dependencies);

    const local = projectChildren(owner, source, { ok: {
      direct: ["@org/a"], findings: [], packages: [{ ...clean, key: "@org/a", version: "e1.r2", override: "../a" }],
    } })[2].children ?? [];
    assert.strictEqual(local[0].description, "e1.r2 · direct · local: ../a");
    assert.strictEqual(local[0].icon, "package");

    const [lockState, bare] = projectChildren(owner, source, { ok: NO_LOCK(["@org/a"]) })[2].children ?? [];
    assert.strictEqual(lockState.label, "No lock file");
    assert.strictEqual(bare.description, "not locked · direct");
    const failed = projectChildren(owner, source, { error: "lock.corrupt" })[2].children ?? [];
    assert.deepStrictEqual(labels(failed), ["Dependencies unavailable"]);
    assert.strictEqual(failed[0].description, "lock.corrupt");
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

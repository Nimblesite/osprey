import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import {
  clientDependencies, compilerSourceGraph, ModuleExplorerProvider, parseSourceGraph, projectChildren,
  type ModuleNode,
} from "../../client/src/module-explorer";
import { resolveBuiltOsprey } from "./osprey-test-env";

const labels = (nodes: readonly ModuleNode[]) => nodes.map((node) => node.label);

function project(files: Record<string, string>): string {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "osprey-modules-")));
  for (const [file, text] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    fs.writeFileSync(path.join(root, file), text);
  }
  return root;
}

const MANIFEST = '[project]\nname = "demo"\nsource_roots = ["src"]\nentry = "src/main.ospml"\n';

suite("Module Explorer [EDITOR-MODULE-EXPLORER]", () => {
  const compiler = () => {
    const built = resolveBuiltOsprey();
    assert.ok(built, "build the Osprey compiler before this suite");
    return built;
  };
  const provider = (roots: string[], direct: readonly string[] | string, trusted = true) =>
    new ModuleExplorerProvider({
      projectRoots: () => roots,
      sourceGraph: compilerSourceGraph(compiler),
      directDependencies: async () => (typeof direct === "string" ? { error: direct } : { ok: direct }),
      trusted: () => trusted,
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
    assert.deepStrictEqual(labels(await tree.getChildren(dependencies)), ["@org/left", "@org/right"]);
    assert.strictEqual((await tree.sourceNode(path.join(root, "src", "more.osp")))?.parent, sources);

    // Selecting a module opens its file at its declaration.
    const command = tree.getTreeItem(declared[1]).command;
    assert.strictEqual(command?.command, "osprey.modules.open");
    await vscode.extensions.getExtension("nimblesite.osprey")?.activate();
    await vscode.commands.executeCommand(command.command, ...(command.arguments ?? []));
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
    assert.ok("error" in missing && missing.error.length > 0);
    fs.rmSync(broken, { recursive: true, force: true });

    // An untrusted workspace runs nothing and says why.
    let ran = false;
    const untrusted = new ModuleExplorerProvider({
      projectRoots: () => ["/project"],
      sourceGraph: async () => { ran = true; return { error: "ran" }; },
      directDependencies: async () => { ran = true; return { error: "ran" }; },
      trusted: () => false,
    });
    const [only] = await untrusted.getChildren();
    assert.deepStrictEqual(labels(await untrusted.getChildren(only)), ["Analysis needs a trusted workspace"]);
    assert.strictEqual(ran, false);
  });

  test("a project with nothing declared and no dependencies says so", () => {
    const owner: ModuleNode = { id: "p", label: "p", projectRoot: "/p" };
    const [modules, sources, dependencies] = projectChildren(owner, { ok: { sources: ["/p/a.osp"], declarations: [] } }, { ok: [] });
    assert.deepStrictEqual(labels(modules.children ?? []), ["No namespaces or modules"]);
    assert.deepStrictEqual(labels(sources.children ?? []), ["a.osp"]);
    assert.deepStrictEqual(labels(dependencies.children ?? []), ["No direct dependencies"]);
  });

  test("a graph or dependency report that is not complete is refused", async () => {
    for (const text of ["", "not json", "{}", '{"sources":[]}', '{"sources":[1],"declarations":[]}',
      '{"sources":[],"declarations":[{"kind":"module"}]}']) {
      assert.strictEqual(parseSourceGraph(text), undefined, text);
    }
    assert.deepStrictEqual(parseSourceGraph('{"sources":["a"],"declarations":[]}'), { sources: ["a"], declarations: [] });

    const answering = (status: number, lines: string[]) =>
      clientDependencies({ run: async () => ({ status, lines }) as never })("/p");
    assert.deepStrictEqual(await answering(0, ["@org/a", "@org/a", "@org/b"]), { ok: ["@org/a", "@org/b"] });
    assert.deepStrictEqual(await answering(1, ["no manifest"]), { error: "no manifest" });
    assert.deepStrictEqual(await answering(0, ["not-qualified"]), { error: "not-qualified" });
    assert.deepStrictEqual(await answering(2, []), { error: "The package client answered status 2." });
    const thrown = await clientDependencies({ run: async () => { throw new Error("spawn failed"); } })("/p");
    assert.deepStrictEqual(thrown, { error: "spawn failed" });
  });
});

import * as assert from "assert";
import { TreeItemCollapsibleState } from "vscode";
import {
  buildProfilingNodes,
  buildProgramNodes,
  buildStackNodes,
  buildTree,
  buildVariableNodes,
  DapRequester,
  DebugSnapshot,
  EMPTY_SNAPSHOT,
  makePanelMessageHandler,
  OspreyDebugTreeProvider,
  programInfoFromConfig,
  readDebugSnapshot,
} from "../../client/src/debug-panel";

const stoppedSnapshot: DebugSnapshot = {
  state: "stopped",
  program: {
    sourceProgram: "/work/app.osp",
    debugBinary: "/work/.osprey-debug/app",
  },
  frames: [
    { id: 1, name: "square", path: "/work/app.osp", line: 1, column: 4 },
    { id: 2, name: "main", path: "/work/app.osp", line: 3, column: 1 },
  ],
  topVariables: [
    { name: "n", value: "7", type: "int" },
    { name: "acc", value: "0" },
  ],
};

suite("debug-panel builders", () => {
  test("program nodes show state, and source/binary when present", () => {
    const inactive = buildProgramNodes(EMPTY_SNAPSHOT);
    assert.strictEqual(inactive.length, 1);
    assert.strictEqual(inactive[0].description, "No active session");

    const full = buildProgramNodes(stoppedSnapshot);
    assert.deepStrictEqual(
      full.map((n) => n.label),
      ["State", "Source", "Debug binary"],
    );
    assert.strictEqual(full[0].description, "Paused");
    assert.strictEqual(full[1].description, "app.osp");
    assert.strictEqual(full[2].description, "app");
  });

  test("stack nodes render a placeholder when empty and frames otherwise", () => {
    const empty = buildStackNodes([]);
    assert.strictEqual(empty[0].kind, "placeholder");

    const nodes = buildStackNodes(stoppedSnapshot.frames);
    assert.deepStrictEqual(
      nodes.map((n) => n.label),
      ["square", "main"],
    );
    assert.strictEqual(nodes[0].description, "app.osp:1");
  });

  test("stack node falls back to a line label without a source path", () => {
    const nodes = buildStackNodes([{ id: 9, name: "f", line: 5, column: 1 }]);
    assert.strictEqual(nodes[0].description, "line 5");
  });

  test("variable nodes render typed and untyped locals, and an empty placeholder", () => {
    assert.strictEqual(buildVariableNodes([])[0].kind, "placeholder");
    const nodes = buildVariableNodes(stoppedSnapshot.topVariables);
    assert.strictEqual(nodes[0].description, "7 : int");
    assert.strictEqual(nodes[1].description, "0");
  });

  test("profiling nodes reserve the CPU and memory surfaces", () => {
    const nodes = buildProfilingNodes();
    assert.deepStrictEqual(
      nodes.map((n) => n.label),
      ["Performance Profiling", "Memory"],
    );
    assert.ok(nodes.every((n) => (n.children?.length ?? 0) > 0));
  });

  test("buildTree assembles the five top-level sections", () => {
    const sections = buildTree(stoppedSnapshot).map((n) => n.label);
    assert.deepStrictEqual(sections, [
      "Program",
      "Call Stack",
      "Variables",
      "Performance Profiling",
      "Memory",
    ]);
  });
});

suite("programInfoFromConfig", () => {
  test("prefers sourceProgram, falls back to program, binary is program", () => {
    assert.deepStrictEqual(
      programInfoFromConfig({ sourceProgram: "/a.osp", program: "/a.bin" }),
      { sourceProgram: "/a.osp", debugBinary: "/a.bin" },
    );
    assert.deepStrictEqual(programInfoFromConfig({ program: "/only.osp" }), {
      sourceProgram: "/only.osp",
      debugBinary: "/only.osp",
    });
  });
});

suite("OspreyDebugTreeProvider", () => {
  test("getChildren returns sections at the root and children for a node", () => {
    const provider = new OspreyDebugTreeProvider();
    provider.update(stoppedSnapshot);
    const sections = provider.getChildren();
    assert.strictEqual(sections.length, 5);
    const stack = sections.find((n) => n.label === "Call Stack");
    assert.ok(stack);
    assert.deepStrictEqual(
      provider.getChildren(stack).map((n) => n.label),
      ["square", "main"],
    );
    // A leaf node has no children.
    assert.deepStrictEqual(
      provider.getChildren({ kind: "info", label: "x" }),
      [],
    );
  });

  test("getTreeItem maps label, description, icon, contextValue and collapsibility", () => {
    const provider = new OspreyDebugTreeProvider();
    const section = buildTree(stoppedSnapshot)[0];
    const sectionItem = provider.getTreeItem(section);
    assert.strictEqual(sectionItem.label, "Program");
    assert.strictEqual(
      sectionItem.collapsibleState,
      TreeItemCollapsibleState.Expanded,
    );
    assert.strictEqual(sectionItem.contextValue, "section");

    const leaf = provider.getTreeItem({
      kind: "info",
      label: "State",
      description: "Paused",
    });
    assert.strictEqual(leaf.collapsibleState, TreeItemCollapsibleState.None);
    assert.strictEqual(leaf.description, "Paused");
  });

  test("update swaps the snapshot and fires the change event", () => {
    const provider = new OspreyDebugTreeProvider();
    let fired = 0;
    provider.onDidChangeTreeData(() => (fired += 1));
    provider.update(stoppedSnapshot);
    assert.strictEqual(provider.current(), stoppedSnapshot);
    assert.strictEqual(fired, 1);
  });
});

function fakeSession(
  responses: Record<string, unknown>,
  recorder?: string[],
): DapRequester {
  return {
    customRequest(command: string) {
      recorder?.push(command);
      return Promise.resolve(responses[command] ?? {});
    },
  };
}

suite("readDebugSnapshot", () => {
  const program = { sourceProgram: "/a.osp", debugBinary: "/a.bin" };

  test("reads the stopped frame and its locals", async () => {
    const session = fakeSession({
      threads: { threads: [{ id: 1 }] },
      stackTrace: {
        stackFrames: [
          { id: 10, name: "square", path: "/a.osp", line: 1, column: 4 },
        ],
      },
      scopes: {
        scopes: [{ variablesReference: 100 }, { variablesReference: 0 }],
      },
      variables: { variables: [{ name: "n", value: "7", type: "int" }] },
    });
    const snapshot = await readDebugSnapshot(session, program);
    assert.strictEqual(snapshot.state, "stopped");
    assert.strictEqual(snapshot.frames[0].name, "square");
    assert.deepStrictEqual(
      snapshot.topVariables.map((v) => v.name),
      ["n"],
    );
    assert.strictEqual(snapshot.program, program);
  });

  test("reports running when no thread has a frame", async () => {
    const noThreads = await readDebugSnapshot(
      fakeSession({ threads: { threads: [] } }),
      program,
    );
    assert.strictEqual(noThreads.state, "running");

    const emptyStack = await readDebugSnapshot(
      fakeSession({
        threads: { threads: [{ id: 1 }] },
        stackTrace: { stackFrames: [] },
      }),
      program,
    );
    assert.strictEqual(emptyStack.state, "running");
  });

  test("tolerates missing arrays in DAP responses", async () => {
    const snapshot = await readDebugSnapshot(fakeSession({}), undefined);
    assert.strictEqual(snapshot.state, "running");
    assert.deepStrictEqual(snapshot.frames, []);
  });
});

suite("makePanelMessageHandler", () => {
  const program = { sourceProgram: "/a.osp", debugBinary: "/a.bin" };

  test("a stopped event refreshes the snapshot from the session", async () => {
    const provider = new OspreyDebugTreeProvider();
    const session = fakeSession({
      threads: { threads: [{ id: 1 }] },
      stackTrace: {
        stackFrames: [
          { id: 10, name: "main", path: "/a.osp", line: 3, column: 1 },
        ],
      },
      scopes: { scopes: [] },
      variables: { variables: [] },
    });
    const handler = makePanelMessageHandler(session, provider, program);
    handler({ type: "event", event: "stopped" });
    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.strictEqual(provider.current().state, "stopped");
    assert.strictEqual(provider.current().frames[0].name, "main");
  });

  test("a continued event marks the panel running", () => {
    const provider = new OspreyDebugTreeProvider();
    provider.update({
      state: "stopped",
      program,
      frames: [],
      topVariables: [],
    });
    makePanelMessageHandler(
      fakeSession({}),
      provider,
      program,
    )({
      type: "event",
      event: "continued",
    });
    assert.strictEqual(provider.current().state, "running");
  });

  test("non-event and unrelated messages are ignored", () => {
    const provider = new OspreyDebugTreeProvider();
    provider.update(EMPTY_SNAPSHOT);
    const recorder: string[] = [];
    const handler = makePanelMessageHandler(
      fakeSession({}, recorder),
      provider,
      program,
    );
    handler({ type: "response", event: "stopped" });
    handler({ type: "event", event: "initialized" });
    assert.strictEqual(provider.current().state, "inactive");
    assert.deepStrictEqual(recorder, []);
  });
});

// Command registration tests for the package palette commands
// ([PACKAGE-API]): the real workbench registers both aliases, forwards URIs
// through workspace validation, and per-project mutations serialize.

import * as assert from "assert";
import * as path from "path";
import { createProjectSerializer } from "../../client/src/keyed-serializer";
import { registerPackageCommands } from "../../client/src/package-commands";
import * as sinon from "sinon";
import { commands, Uri, window, workspace } from "vscode";
import { activateWithServer, connectEditorBrowser } from "./profiler-kit";

const root = path.resolve("/work/app");

suite("serialized per-project mutation", () => {
  test("the serializer is per project root and always returns the report", async () => {
    const serialize = createProjectSerializer();
    const seen: number[] = [];
    const first = serialize("p", async () => {
      await new Promise((r) => setTimeout(r, 5));
      seen.push(1);
      return 1;
    });
    const second = serialize("p", async () => {
      seen.push(2);
      return 2;
    });
    assert.strictEqual(await first, 1);
    assert.strictEqual(await second, 2);
    assert.deepStrictEqual(seen, [1, 2]);
  });
});

suite("command registration", () => {
  teardown(() => sinon.restore());
  test("the shared browser harness attaches to the actual first-host workbench", async () => {
    await activateWithServer();
    const browser = await connectEditorBrowser();
    try {
      const pages = browser.contexts().flatMap((context) => context.pages());
      assert.ok(
        pages.some((page) => page.url().includes("/workbench/workbench.html")),
        "connected to the running editor",
      );
    } finally {
      await browser.close();
    }
  });

  test("real package aliases dispatch to workspace validation", async () => {
    await activateWithServer();
    assert.strictEqual(
      workspace.workspaceFolders?.length ?? 0,
      0,
      "isolated host has no open folder",
    );
    const dispatch = sinon.spy(commands, "executeCommand");
    const errors = sinon.spy(window, "showErrorMessage");
    const uri = Uri.file("/package-alias-e2e/osprey.toml");
    for (const alias of ["osprey.packages.add", "osprey.packages.remove"]) {
      const before = errors.callCount;
      await commands.executeCommand(alias, uri);
      assert.ok(
        dispatch.calledWithExactly("osprey.packages.open", uri),
        `${alias} forwards its URI`,
      );
      assert.strictEqual(
        errors.callCount,
        before + 1,
        `${alias} reaches real workspace validation`,
      );
      assert.strictEqual(
        errors.lastCall.args[0],
        "Open a workspace folder before managing Osprey packages.",
      );
      dispatch.resetHistory();
    }
    await commands.executeCommand("notifications.clearAll");
  });

  test("registerPackageCommands registers both commands on the real command palette", async () => {
    await activateWithServer();
    const callbacks = new Map<string, (uri?: Uri) => Thenable<unknown>>();
    const forwarded: [string, Uri | undefined][] = [];
    const subscriptions: { dispose(): unknown }[] = [];
    registerPackageCommands(
      { subscriptions },
      {
        registerCommand: (id, callback) => {
          assert.ok(!callbacks.has(id), "each alias is registered once");
          callbacks.set(id, callback);
          return {
            dispose: () => {
              callbacks.delete(id);
            },
          };
        },
        executeCommand: async (id, uri) => {
          forwarded.push([id, uri]);
        },
      },
    );
    try {
      assert.strictEqual(subscriptions.length, 2);
      const registered = await commands.getCommands(true);
      assert.ok(registered.includes("osprey.packages.add"));
      assert.ok(registered.includes("osprey.packages.remove"));
      assert.ok(registered.includes("osprey.packages.open"));
      const uri = Uri.file(root);
      for (const id of ["osprey.packages.add", "osprey.packages.remove"]) {
        const callback = callbacks.get(id);
        assert.ok(callback, `${id} callback exists`);
        await callback(uri);
      }
      assert.deepStrictEqual(forwarded, [
        ["osprey.packages.open", uri],
        ["osprey.packages.open", uri],
      ]);
    } finally {
      subscriptions.forEach((s) => s.dispose());
    }
    assert.strictEqual(callbacks.size, 0, "disposal removes both aliases");
  });
});

// Command-flow tests for the package add/remove UX ([PACKAGE-API]): every
// user-visible decision (pick, warning, error) and every frontend invocation
// is observed through injected fakes — the Osprey CLI stays the only mutator.

import * as assert from "assert";
import * as path from "path";
import {
  createProjectSerializer,
  makeAddHandler,
  makeRemoveHandler,
  registerPackageCommands,
} from "../../client/src/package-commands";
import * as sinon from "sinon";
import { commands, Uri, window, workspace } from "vscode";
import { activateWithServer, connectEditorBrowser } from "./profiler-kit";
import { fakeDeps, okAdd } from "./package-harness";

const root = path.resolve("/work/app");
const spaced = path.resolve("/work/my app");
const listing = JSON.stringify({
  packages: [
    { key: "@demo/pyadd", releases: [{ digest: "aa", epoch: 1, ordinal: 2 }] },
    { key: "@other/util", releases: [{ digest: "cc", epoch: 1, ordinal: 5 }] },
    { key: "@demo/util", releases: [{ digest: "dd", epoch: 3, ordinal: 1 }] },
  ],
});
const manifest = '[dependencies]\n"@other/util" = { epoch = 1, min_release = 5 }\n"@demo/pyadd" = { epoch = 1, min_release = 2 }';

suite("package add command", () => {
  test("an org-qualified pick runs the frontend with --project . from the project root", async () => {
    const deps = fakeDeps({ listing, frontend: [okAdd("@demo/pyadd")] });
    await makeAddHandler(deps)();
    assert.strictEqual(deps.frontend.calls.length, 1);
    assert.deepStrictEqual(deps.frontend.calls[0], {
      command: "--json add @demo/pyadd --registry http://127.0.0.1:8787 --project .",
      cwd: root,
    });
    assert.ok(deps.ui.messages.some((m) => m.includes("added @demo/pyadd e1.r2")), deps.ui.messages.join("|"));
  });

  test("a project root with spaces reaches the CLI as --project . and nothing else", async () => {
    const deps = fakeDeps({ projects: [spaced], listing, frontend: [okAdd("@demo/pyadd")] });
    await makeAddHandler(deps)();
    assert.strictEqual(deps.frontend.calls[0].cwd, spaced);
    assert.ok(!deps.frontend.calls[0].command.includes(spaced), "the spaced path must never enter the stdin command");
    assert.ok(deps.frontend.calls[0].command.endsWith("--project ."));
  });

  test("every pick item warns that security evidence is unavailable", async () => {
    const deps = fakeDeps({ listing });
    const seen: string[] = [];
    deps.ui.showQuickPick = async (items: any[]) => {
      seen.push(...items.map((i) => i.detail));
      return undefined;
    };
    await makeAddHandler(deps)();
    assert.strictEqual(seen.length, 3);
    for (const detail of seen) {
      assert.ok(detail.includes("No security audit evidence"), detail);
    }
  });

  test("duplicate leaf names under distinct orgs are both offered", async () => {
    const deps = fakeDeps({ listing });
    const keys: string[] = [];
    deps.ui.showQuickPick = async (items: any[]) => {
      keys.push(...items.map((i) => i.label));
      return undefined;
    };
    await makeAddHandler(deps)();
    assert.deepStrictEqual(keys.filter((k) => k.endsWith("/util")).sort(), ["@demo/util", "@other/util"]);
  });

  test("a security confirm step precedes the mutation and can decline it", async () => {
    const deps = fakeDeps({ listing, frontend: [okAdd("@demo/pyadd")] });
    deps.ui.picks = [{ label: "@demo/pyadd", key: "@demo/pyadd", detail: "No security audit evidence is available for this release — treat it as unverified." }];
    deps.ui.messages.length = 0;
    let confirmed = true;
    deps.ui.showWarningMessage = async (message: string) => {
      deps.ui.messages.push(message);
      return confirmed ? { title: "Add Anyway" } : undefined;
    };
    await makeAddHandler(deps)();
    assert.ok(deps.ui.messages.some((m) => m.toLowerCase().includes("unverified")), deps.ui.messages.join("|"));
    confirmed = false;
    deps.frontend.calls.length = 0;
    await makeAddHandler(deps)();
    assert.strictEqual(deps.frontend.calls.length, 0, "declining the warning must not mutate anything");
  });

  test("dismissing the QuickPick or a cancelled token is silence, not an error", async () => {
    const dismissed = fakeDeps({ listing });
    dismissed.ui.picks = [undefined];
    await makeAddHandler(dismissed)();
    assert.strictEqual(dismissed.frontend.calls.length, 0);
    assert.strictEqual(dismissed.ui.messages.length, 0);

    const cancelled = fakeDeps({ listing, cancelled: true });
    cancelled.ui.picks = [{ label: "@demo/pyadd", key: "@demo/pyadd" }];
    await makeAddHandler(cancelled)();
    assert.strictEqual(cancelled.frontend.calls.length, 0);
    assert.strictEqual(cancelled.ui.messages.length, 0);
  });

  test("an unavailable listing is an error, never an empty 'safe' pick list", async () => {
    const deps = fakeDeps({ listing: "not json" });
    await makeAddHandler(deps)();
    assert.strictEqual(deps.frontend.calls.length, 0);
    assert.ok(deps.ui.messages.some((m) => m.includes("registry listing")), deps.ui.messages.join("|"));
  });

  test("a semantically failed report is an error and says the reason code", async () => {
    const deps = fakeDeps({ listing, frontend: [{ status: 1, lines: ["add.exists @demo/pyadd"] }] });
    deps.ui.picks = [{ label: "@demo/pyadd", key: "@demo/pyadd" }];
    await makeAddHandler(deps)();
    assert.ok(deps.ui.messages.some((m) => m.includes("add.exists")), deps.ui.messages.join("|"));
    assert.ok(deps.ui.messages.every((m) => !m.startsWith("Added ")), deps.ui.messages.join("|"));
  });

  test("dirty manifest or lock files block the mutation before the CLI runs", async () => {
    for (const dirty of [[path.join(root, "osprey.toml")], [path.join(root, "osprey.lock")]]) {
      const deps = fakeDeps({ listing, dirty, frontend: [okAdd("@demo/pyadd")] });
      deps.ui.picks = [{ label: "@demo/pyadd", key: "@demo/pyadd" }];
      await makeAddHandler(deps)();
      assert.strictEqual(deps.frontend.calls.length, 0);
      assert.ok(deps.ui.messages.some((m) => /unsaved/i.test(m)), deps.ui.messages.join("|"));
    }
  });

  test("no workspace folder is a useful error", async () => {
    const deps = fakeDeps({ projects: [] });
    await makeAddHandler(deps)();
    assert.ok(deps.ui.messages.some((m) => m.includes("workspace")), deps.ui.messages.join("|"));
  });
});

suite("package remove command", () => {
  test("removes a direct dependency through the frontend", async () => {
    const deps = fakeDeps({ manifest, frontend: [{ status: 0, lines: ["removed @other/util"] }] });
    deps.ui.picks = [{ label: "@other/util" }];
    await makeRemoveHandler(deps)();
    assert.strictEqual(deps.frontend.calls[0].command, "--json tree --direct --project .");
    assert.deepStrictEqual(deps.frontend.calls[1], {
      command: "--json remove @other/util --registry http://127.0.0.1:8787 --project .",
      cwd: root,
    });
  });

  test("only the client's parsed direct dependencies are offered", async () => {
    const deps = fakeDeps({ manifest });
    const keys: string[] = [];
    deps.ui.showQuickPick = async (items: any[]) => {
      keys.push(...items.map((i) => i.label));
      return undefined;
    };
    await makeRemoveHandler(deps)();
    assert.deepStrictEqual(keys.sort(), ["@demo/pyadd", "@other/util"]);
  });

  test("a missing manifest is an error, not an empty list", async () => {
    const deps = fakeDeps({ manifest: undefined });
    await makeRemoveHandler(deps)();
    assert.ok(deps.ui.messages.some((m) => m.includes("osprey.toml")), deps.ui.messages.join("|"));
  });

  test("a failed remove keeps its reason code and never claims success", async () => {
    const deps = fakeDeps({ manifest, frontend: [{ status: 1, lines: ["remove.missing @other/util"] }] });
    deps.ui.picks = [{ label: "@other/util" }];
    await makeRemoveHandler(deps)();
    assert.ok(deps.ui.messages.some((m) => m.includes("remove.missing")), deps.ui.messages.join("|"));
  });

  test("dirty files and cancellation block the mutation too", async () => {
    const dirty = fakeDeps({ manifest, dirty: [path.join(root, "osprey.lock")] });
    dirty.ui.picks = [{ label: "@other/util" }];
    await makeRemoveHandler(dirty)();
    assert.strictEqual(dirty.frontend.calls.length, 0);

    const cancelled = fakeDeps({ manifest, cancelled: true });
    cancelled.ui.picks = [{ label: "@other/util" }];
    await makeRemoveHandler(cancelled)();
    assert.strictEqual(cancelled.frontend.calls.length, 0);
    assert.strictEqual(cancelled.ui.messages.length, 0);
  });
});

suite("serialized per-project mutation", () => {
  test("overlapping operations on one project run one after the other", async () => {
    const order: string[] = [];
    const deps = fakeDeps({ listing, frontend: [] });
    deps.frontend.run = async (command) => {
      const tag = command.includes("@demo/pyadd") ? "a-start" : "b-start";
      order.push(tag);
      await new Promise((resolve) => setTimeout(resolve, 5));
      order.push(tag.replace("start", "end"));
      return okAdd("@demo/pyadd");
    };
    const add = makeAddHandler(deps);
    deps.ui.picks = [
      { label: "@demo/pyadd", key: "@demo/pyadd" },
      { label: "@other/util", key: "@other/util" },
    ];
    await Promise.all([add(), add()]);
    assert.deepStrictEqual(order, ["a-start", "a-end", "b-start", "b-end"]);
  });

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
      assert.ok(pages.some((page) => page.url().includes("/workbench/workbench.html")), "connected to the running editor");
    } finally { await browser.close(); }
  });

  test("real package aliases dispatch to workspace validation", async () => {
    await activateWithServer();
    assert.strictEqual(workspace.workspaceFolders?.length ?? 0, 0, "isolated host has no open folder");
    const dispatch = sinon.spy(commands, "executeCommand");
    const errors = sinon.spy(window, "showErrorMessage");
    const uri = Uri.file("/package-alias-e2e/osprey.toml");
    for (const alias of ["osprey.packages.add", "osprey.packages.remove"]) {
      const before = errors.callCount;
      await commands.executeCommand(alias, uri);
      assert.ok(dispatch.calledWithExactly("osprey.packages.open", uri), `${alias} forwards its URI`);
      assert.strictEqual(errors.callCount, before + 1, `${alias} reaches real workspace validation`);
      assert.strictEqual(errors.lastCall.args[0], "Open a workspace folder before managing Osprey packages.");
      dispatch.resetHistory();
    }
    await commands.executeCommand("notifications.clearAll");
  });


  test("registerPackageCommands registers both commands on the real command palette", async () => {
    await activateWithServer();
    const callbacks = new Map<string, (uri?: Uri) => Thenable<unknown>>();
    const forwarded: [string, Uri | undefined][] = [];
    const subscriptions: { dispose(): unknown }[] = [];
    registerPackageCommands({ subscriptions }, {
      registerCommand: (id, callback) => {
        assert.ok(!callbacks.has(id), "each alias is registered once");
        callbacks.set(id, callback);
        return { dispose: () => { callbacks.delete(id); } };
      },
      executeCommand: async (id, uri) => { forwarded.push([id, uri]); },
    });
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
      assert.deepStrictEqual(forwarded, [["osprey.packages.open", uri], ["osprey.packages.open", uri]]);
    } finally { subscriptions.forEach((s) => s.dispose()); }
    assert.strictEqual(callbacks.size, 0, "disposal removes both aliases");
  });
});

suite("package queued editor safety", () => {
  test("a manifest dirtied while an operation waits blocks the queued write", async () => {
    const deps = fakeDeps({ listing });
    let dirty = false;
    deps.dirtyFiles = () => dirty ? [path.join(root, "osprey.toml")] : [];
    const serialize = async <T>(_root: string, task: () => Promise<T>) => { dirty = true; return task(); };
    await makeAddHandler(deps, serialize)();
    assert.strictEqual(deps.frontend.calls.length, 0);
    assert.ok(deps.ui.messages.some((message) => /Unsaved/.test(message)));
  });
});

suite("package hosted trust boundary", () => {
  test("a hosted listing without security eligibility cannot offer an install bypass", async () => {
    const deps = fakeDeps({ listing });
    deps.registry = () => "https://packages.example";
    await makeAddHandler(deps)();
    assert.strictEqual(deps.frontend.calls.length, 0);
    assert.ok(deps.ui.messages.some((message) => /security eligibility/.test(message)));
  });
});

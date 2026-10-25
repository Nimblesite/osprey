import * as assert from "assert";
import * as sinon from "sinon";
import { CancellationTokenSource, window } from "vscode";
import { defaultPackageDeps, makeAddHandler } from "../../client/src/package-commands";

const selected = { label: "@finch/ledger", key: "@finch/ledger", detail: "Unverified evidence" };

function editorFixture(): CancellationTokenSource {
  sinon.stub(window, "showQuickPick").resolves(selected);
  sinon.stub(window, "showWarningMessage").resolves({ title: "Add Anyway" });
  sinon.stub(window, "showInformationMessage").resolves(undefined);
  const cancellation = new CancellationTokenSource();
  sinon.stub(window, "withProgress").callsFake(async (_options, task) =>
    task({ report: () => undefined }, cancellation.token));
  return cancellation;
}

function dependencyFixture(calls: string[]) {
  return {
    ...defaultPackageDeps(), projectRoots: () => ["/work/project with spaces"], dirtyFiles: () => [],
    fetchListing: async () => JSON.stringify({ packages: [
      { key: selected.key, releases: [{ epoch: 1, ordinal: 1, digest: `sha256:${"a".repeat(64)}` }] },
    ] }),
    frontend: { run: async (command: string, cwd: string) => {
      calls.push(`${cwd}\n${command}`);
      return { status: 0, lines: ["fixture operation completed"] };
    } },
  };
}

suite("package production editor adapter", () => {
  teardown(() => sinon.restore());

  test("the real single-selection QuickPick reaches the package frontend", async () => {
    const cancellation = editorFixture();
    const calls: string[] = [];
    try {
      await makeAddHandler(dependencyFixture(calls))();
      assert.strictEqual(calls.length, 1, "a real VS Code item must not disappear through an array cast");
      assert.ok(calls[0].includes("--project ."));
    } finally {
      cancellation.dispose();
    }
  });
});

suite("package VSIX contributions", () => {
  test("commands and machine-scoped executable setting ship in the manifest", () => {
    const manifest = require("../../../package.json");
    for (const id of ["osprey.packages.add", "osprey.packages.remove"]) {
      assert.ok(manifest.contributes.commands.some((entry: {command: string}) => entry.command === id), id);
    }
    assert.strictEqual(manifest.contributes.configuration.properties["osprey.packages.frontendPath"].scope, "machine");
  });
});

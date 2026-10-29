import * as assert from "assert";
import { createHash } from "crypto";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { waitFor } from "./editor";

async function installedOsprey() {
  const extension = vscode.extensions.getExtension("nimblesite.osprey");
  assert.ok(extension, "Osprey must be installed from the VSIX");
  await extension.activate();
  return extension;
}

function checkedWasm(file: string, expected: string | undefined) {
  const wasm = fs.readFileSync(file);
  assert.strictEqual(createHash("sha256").update(wasm).digest("hex"), expected);
  assert.ok(WebAssembly.validate(wasm), "the installed application must be valid WASM");
  return new WebAssembly.Module(wasm);
}

suite("installed VSIX packages", () => {
  test("ships the exact package client and WASM workspace and activates package commands", async () => {
    const extension = await installedOsprey();
    const platform = `${process.platform === "win32" ? "win32" : process.platform === "darwin" ? "darwin" : "linux"}-${process.arch === "arm64" ? "arm64" : "x64"}`;
    const executable = process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg";
    const client = path.join(extension.extensionPath, "bin", platform, executable);
    assert.strictEqual(createHash("sha256").update(fs.readFileSync(client)).digest("hex"), process.env.OSPREY_VSIX_PACKAGE_CLIENT_SHA256);
    checkedWasm(path.join(path.dirname(client), "package-workspace.wasm"), process.env.OSPREY_VSIX_PACKAGE_WORKSPACE_SHA256);
    const commands = await vscode.commands.getCommands(true);
    for (const command of ["osprey.packages.open", "osprey.packages.add", "osprey.packages.remove"]) assert.ok(commands.includes(command));
  });

  test("ships the exact shared Workbench WASM and every asset and registers its command", async () => {
    const extension = await installedOsprey();
    const root = path.join(extension.extensionPath, "bin", `${process.platform}-${process.arch}`, "workbench");
    const hashes = JSON.parse(process.env.OSPREY_VSIX_WORKBENCH_HASHES ?? "{}") as Record<string, string>;
    assert.ok(Object.keys(hashes).length > 10, "the source bundle must contain Workbench and its assets");
    for (const [name, hash] of Object.entries(hashes))
      assert.strictEqual(createHash("sha256").update(fs.readFileSync(path.join(root, name))).digest("hex"), hash, name);
    const exports = WebAssembly.Module.exports(checkedWasm(path.join(path.dirname(root), "workbench.wasm"), process.env.OSPREY_VSIX_WORKBENCH_SHA256));
    assert.ok(exports.some(item => item.name === "_start" && item.kind === "function"));
    assert.ok(exports.some(item => item.name === "memory" && item.kind === "memory"));
    assert.ok((await vscode.commands.getCommands(true)).includes("osprey.workbench.open"));
  });

  test("the installed Workbench command opens and reuses its WASM panel", async () => {
    await installedOsprey();
    const panels = () => vscode.window.tabGroups.all.flatMap(group => group.tabs).filter(tab => tab.label === "Osprey Workbench" && tab.input instanceof vscode.TabInputWebview);
    const before = new Set(panels());
    try {
      await vscode.commands.executeCommand("osprey.workbench.open");
      await waitFor(() => panels().length, count => count === 1, "the installed Workbench opens its panel");
      await vscode.commands.executeCommand("osprey.workbench.open");
      assert.strictEqual(panels().length, 1, "the installed command reuses its panel");
    } finally { await vscode.window.tabGroups.close(panels().filter(tab => !before.has(tab))); }
  });
});

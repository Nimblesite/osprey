import * as assert from "assert";
import { createHash } from "crypto";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";

suite("installed VSIX packages", () => {
  test("ships the exact package client and WASM workspace and activates package commands", async () => {
    const extension = vscode.extensions.getExtension("nimblesite.osprey");
    assert.ok(extension, "Osprey must be installed from the VSIX");
    await extension.activate();
    const platform = `${process.platform === "win32" ? "win32" : process.platform === "darwin" ? "darwin" : "linux"}-${process.arch === "arm64" ? "arm64" : "x64"}`;
    const executable = process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg";
    const client = path.join(extension.extensionPath, "bin", platform, executable);
    assert.strictEqual(createHash("sha256").update(fs.readFileSync(client)).digest("hex"), process.env.OSPREY_VSIX_PACKAGE_CLIENT_SHA256);
    const wasm = fs.readFileSync(path.join(path.dirname(client), "package-workspace.wasm"));
    assert.strictEqual(createHash("sha256").update(wasm).digest("hex"), process.env.OSPREY_VSIX_PACKAGE_WORKSPACE_SHA256);
    assert.ok(WebAssembly.validate(wasm), "the installed package workspace must be valid WASM");
    const commands = await vscode.commands.getCommands(true);
    for (const command of ["osprey.packages.open", "osprey.packages.add", "osprey.packages.remove"]) assert.ok(commands.includes(command));
  });
});

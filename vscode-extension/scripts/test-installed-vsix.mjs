import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { downloadAndUnzipVSCode, runTests, runVSCodeCommand } from "@vscode/test-electron";

const extensionRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = JSON.parse(fs.readFileSync(path.join(extensionRoot, "package.json"), "utf8"));
const vsix = path.resolve(process.argv[2] ?? path.join(extensionRoot, `osprey-${manifest.version}.vsix`));
const executable = process.platform === "win32" ? "osprey.exe" : "osprey";
const compiler = path.resolve(extensionRoot, "..", "target", "release", executable);
assert.ok(fs.existsSync(vsix), `Package the extension before testing: ${vsix}`);
assert.ok(fs.existsSync(compiler), `Build the compiler before testing: ${compiler}`);
const compilerHash = createHash("sha256").update(fs.readFileSync(compiler)).digest("hex");
const clientHashes = Object.fromEntries(["extension", "warning-fixes"].map((name) => [name,
  createHash("sha256").update(fs.readFileSync(path.join(extensionRoot, "out", "client", "src", `${name}.js`))).digest("hex")]));
const root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-installed-vsix-"));
const extensions = path.join(root, "extensions");
const profile = path.join(root, "profile");
const workspace = path.join(root, "workspace");
const driver = path.join(root, "test-driver");
const trapDir = path.join(root, "path-trap");
const trapMarker = path.join(root, "path-compiler-was-used");
for (const directory of [extensions, profile, workspace, trapDir, driver]) fs.mkdirSync(directory);
fs.writeFileSync(path.join(driver, "package.json"), JSON.stringify({
  name: "installed-vsix-test-driver", publisher: "osprey-tests", version: "0.0.0",
  engines: { vscode: "^1.96.0" }, main: "./index.js", activationEvents: ["*"],
}));
fs.writeFileSync(path.join(driver, "index.js"), "exports.activate = () => {};\n");
const profileArgs = ["--extensions-dir", extensions, "--user-data-dir", profile];
const trap = process.platform === "win32"
  ? '@echo off\r\necho %*>>"%OSPREY_VSIX_PATH_TRAP%"\r\nexit /b 87\r\n'
  : '#!/bin/sh\nprintf "%s\\n" "$*" >> "$OSPREY_VSIX_PATH_TRAP"\nexit 87\n';
fs.writeFileSync(path.join(trapDir, process.platform === "win32" ? "osprey.cmd" : "osprey"), trap, { mode: 0o755 });
fs.mkdirSync(path.join(profile, "User"));
fs.writeFileSync(path.join(profile, "User", "settings.json"), JSON.stringify({
  "files.autoSave": "off", "editor.formatOnSave": false,
  "editor.codeActionsOnSave": {}, "workbench.startupEditor": "none",
  "extensions.autoCheckUpdates": false, "extensions.autoUpdate": false,
  // Built-in AI Fix/Explain providers would add unrelated actions to the
  // language server's exact result set, even in an empty extension profile.
  "chat.disableAIFeatures": true,
  "osprey.server.compilerPath": "", "osprey.server.path": "",
}));

try {
  const vscodeExecutablePath = await downloadAndUnzipVSCode("stable");
  const installed = await runVSCodeCommand([...profileArgs, "--install-extension", vsix, "--force"]);
  process.stdout.write(installed.stdout);
  await runTests({
    vscodeExecutablePath,
    // VS Code requires a development path to run tests. This inert driver has
    // no product code: Osprey is loaded only from the installed VSIX.
    extensionDevelopmentPath: driver,
    extensionTestsPath: path.join(extensionRoot, "out", "test", "installed", "index.js"),
    launchArgs: [workspace, ...profileArgs, "--disable-workspace-trust", "--skip-welcome", "--skip-release-notes"],
    extensionTestsEnv: {
      ELECTRON_RUN_AS_NODE: undefined,
      OSPREY_VSIX_TEST_ROOT: root,
      OSPREY_VSIX_EXTENSIONS: extensions,
      OSPREY_VSIX_COMPILER_SHA256: compilerHash,
      OSPREY_VSIX_CLIENT_HASHES: JSON.stringify(clientHashes),
      OSPREY_VSIX_SOURCE_EXTENSION: extensionRoot,
      OSPREY_VSIX_PATH_TRAP: trapMarker,
      PATH: `${trapDir}${path.delimiter}${process.env.PATH ?? ""}`,
    },
  });
  // Shipwright probes all candidates with --version before selecting the
  // bundle. A probe is allowed; executing the LSP or any compiler work is not.
  const pathCalls = fs.existsSync(trapMarker) ? fs.readFileSync(trapMarker, "utf8").trim().split(/\r?\n/) : [];
  assert.ok(pathCalls.every((args) => args === "--version"), `The installed extension executed compiler work on PATH: ${JSON.stringify(pathCalls)}`);
} finally {
  fs.rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
}

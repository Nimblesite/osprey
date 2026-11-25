// [PACKAGE-EDITOR] Add and Remove open the Osprey WASM package workspace.
import * as fs from "fs";
import * as path from "path";
import { commands, Uri, workspace } from "vscode";
import { shipwrightPlatform } from "./client-config";
import { runFrontendCommand } from "./package-frontend";
import type { PkgReport } from "./package-frontend";

export interface FrontendRunner {
  run(command: string, cwd: string): Promise<PkgReport>;
}

export const MANIFEST_FILE = "osprey.toml";
const FRONTEND_CONFIG = "packages.frontendPath";
const FRONTEND_DEFAULT = "osprey-pkg";

/** The frontend executable: the configured path, else `osprey-pkg` on PATH. */
export function defaultFrontend(extensionPath?: string): FrontendRunner {
  return { run: (command, cwd) => {
    const configured = workspace.getConfiguration("osprey").get<string>(FRONTEND_CONFIG);
    const bundled = extensionPath && path.join(extensionPath, "bin", shipwrightPlatform(), process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg");
    return runFrontendCommand(configured || (bundled && fs.existsSync(bundled) ? bundled : FRONTEND_DEFAULT), command, cwd);
  } };
}

/** VS Code capabilities used by the two package-workspace aliases. */
export interface PackageCommandHost {
  registerCommand(id: string, callback: (uri?: Uri) => Thenable<unknown>): { dispose(): unknown };
  executeCommand(id: string, uri?: Uri): Thenable<unknown>;
}

/** Register Add/Remove as aliases of the Osprey WASM package workspace. */
export function registerPackageCommands(
  context: { subscriptions: { push(item: { dispose(): unknown }): unknown } },
  host: PackageCommandHost = commands,
): void {
  for (const id of ["osprey.packages.add", "osprey.packages.remove"]) {
    context.subscriptions.push(host.registerCommand(id, (uri) =>
      host.executeCommand("osprey.packages.open", uri)));
  }
}

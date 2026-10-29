// Implements [WORKBENCH-VSIX]: the shared Osprey app hosted without a project.
import * as path from "node:path";
import * as os from "node:os";
import { Uri, window, type ExtensionContext } from "vscode";
import { shipwrightPlatform } from "./client-config";
import { createLocalExecutor } from "./osprey-ui/local-capabilities";
import type { DocumentDialogs } from "./osprey-ui/document-capabilities";
import { nativeContext, registerNativePanel, type NativeApplication } from "./osprey-ui/panel";

export const WORKBENCH_COMMAND = "osprey.workbench.open";

function selectedPath(uri: Uri | undefined): string | undefined {
  if (uri && uri.scheme !== "file") throw new Error(`Unsupported selected document scheme: ${uri.scheme}`);
  return uri?.fsPath;
}

function documentDialogs(): DocumentDialogs {
  return {
    open: async types => selectedPath((await window.showOpenDialog({ canSelectFiles: true, canSelectFolders: false,
      canSelectMany: false, ...(types.length ? { filters: { Documents: [...types] } } : {}) }))?.[0]),
    save: async name => selectedPath(await window.showSaveDialog({ defaultUri: Uri.file(path.join(os.homedir(), name)) })),
  };
}

function application(context: ExtensionContext): NativeApplication {
  const platform = path.join(context.extensionPath, "bin", shipwrightPlatform());
  const assets = path.join(platform, "workbench");
  return { key: "workbench", artifact: path.join(platform, "workbench.wasm"), assets, stylesheet: "workbench.css", symbols: "symbols.json",
    context: nativeContext([]), execute: createLocalExecutor({ assetRoot: assets, dialogs: documentDialogs(),
      storeRoot: path.join(context.globalStorageUri.fsPath, "workbench") }) };
}

/** Open the same Osprey workbench used by browser and native hosts. */
export function registerWorkbench(context: ExtensionContext): { dispose(): void } {
  return registerNativePanel(context, { command: WORKBENCH_COMMAND, view: "osprey.workbench", title: "Osprey Workbench",
    loading: "Loading your workbench…", prepare: async () => application(context) });
}

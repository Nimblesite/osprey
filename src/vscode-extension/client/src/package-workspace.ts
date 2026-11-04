/// <reference lib="dom" />
// "Osprey: Manage Packages" ([PACKAGE-EDITOR]): the VSIX shell around the
// Osprey workspace WASM application. The host owns the panel, the WASI process,
// theme-safe rendering and bounded capabilities; package state and decisions
// stay in Osprey.

import * as fs from "fs";
import * as path from "path";
import { commands, ExtensionContext, Range, Uri, ViewColumn, window, workspace } from "vscode";
import { shipwrightPlatform } from "./client-config";
import { createRequestExecutor, type CapabilityHost } from "./osprey-ui/capabilities";
import { nativeContext, registerNativePanel, type NativeApplication } from "./osprey-ui/panel";

export const PACKAGE_WORKSPACE_COMMAND = "osprey.packages.open";
const PANEL_TITLE = "Osprey Packages";
const VIEW_ID = "osprey.packages";
const DEFAULT_REGISTRY = "http://127.0.0.1:8787";
const WASM_NAME = "package-workspace.wasm";

function executableName(name: string): string {
  return process.platform === "win32" ? `${name}.exe` : name;
}

function registryOrigin(resource: Uri): string {
  const configured = workspace.getConfiguration("osprey", resource).get<string>("packages.registry") ?? DEFAULT_REGISTRY;
  const url = new URL(configured);
  const loopback = ["127.0.0.1", "localhost", "[::1]", "::1"].includes(url.hostname);
  if (url.username || url.password || url.search || url.hash || (url.pathname !== "" && url.pathname !== "/")) {
    throw new Error("The package registry setting must be a plain origin without credentials, path, query or fragment");
  }
  if (url.protocol !== "https:" && !(url.protocol === "http:" && loopback)) {
    throw new Error("The package registry must use HTTPS (HTTP is allowed only on loopback)");
  }
  return url.origin;
}

/** Resolve the invoking project, asking when a command has no root context. */
export async function folderFor(resource?: Uri): Promise<Uri | undefined> {
  const invoking = resource ? workspace.getWorkspaceFolder(resource) : undefined;
  if (invoking) return invoking.uri;
  const folders = workspace.workspaceFolders ?? [];
  if (folders.length === 1) return folders[0].uri;
  if (folders.length === 0) return undefined;
  const selected = await window.showQuickPick(
    folders.map((folder) => ({ label: folder.name, description: folder.uri.fsPath, uri: folder.uri })),
    { title: "Choose the project to manage", placeHolder: "Package changes apply only to this project" },
  );
  return selected?.uri;
}

function applicationCandidates(context: ExtensionContext): string[] {
  const configured = workspace.getConfiguration("osprey").get<string>("packages.appPath") ?? "";
  const platform = shipwrightPlatform();
  const candidates = [
    configured,
    path.join(context.extensionPath, "bin", platform, WASM_NAME),
    path.join(context.extensionPath, "bin", WASM_NAME),
    path.join(context.extensionPath, "bin", platform, "registry-editor.wasm"),
  ];
  return candidates.filter((candidate) => candidate !== "");
}

function applicationPath(context: ExtensionContext): string {
  const found = applicationCandidates(context).find((candidate) => fs.existsSync(candidate));
  if (!found) {
    throw new Error(`The Osprey package workspace application is not built. Expected ${applicationCandidates(context).join(" or ")}`);
  }
  return found;
}

function packageExecutable(context: Pick<ExtensionContext, "extensionPath">): string {
  return path.join(context.extensionPath, "bin", shipwrightPlatform(), executableName("osprey-pkg"));
}

function workspaceSnapshot(): unknown {
  return {
    trusted: workspace.isTrusted,
    folders: (workspace.workspaceFolders ?? []).map((folder) => ({ uri: folder.uri.toString(), name: folder.name, path: folder.uri.fsPath })),
    activeDocument: window.activeTextEditor?.document.uri.toString() ?? null,
  };
}

function readConfiguration(section: string, workspaceUri: string): unknown {
  const configuration = workspace.getConfiguration(section, Uri.parse(workspaceUri));
  const values: Record<string, unknown> = {};
  for (const key of ["registry", "frontendPath", "appPath", "server.compilerPath"]) {
    const value = configuration.get(key);
    if (value !== undefined) values[key] = value;
  }
  return { section, values };
}

export function hostBridge(context: Pick<ExtensionContext, "extensionPath">): CapabilityHost {
  return {
    get trusted() { return workspace.isTrusted; },
    get workspaceFolders() { return (workspace.workspaceFolders ?? []).map((folder) => folder.uri.fsPath); },
    configuredExecutable: workspace.getConfiguration("osprey").get<string>("packages.frontendPath") ?? "",
    bundledExecutable: packageExecutable(context),
    dirtyFiles: () => workspace.textDocuments.filter((document) => document.isDirty).map((document) => document.fileName),
    workspaceSnapshot,
    openDocument: async (uri, line, column) => {
      const document = await workspace.openTextDocument(Uri.parse(uri));
      const selection = new Range(line - 1, column - 1, line - 1, column - 1);
      await window.showTextDocument(document, { preview: true, viewColumn: ViewColumn.Beside, selection });
      return { opened: true, uri };
    },
    readConfiguration,
    revealView: async (view) => {
      if (view !== VIEW_ID) throw new Error(`Unknown Osprey view: ${view}`);
      await commands.executeCommand(PACKAGE_WORKSPACE_COMMAND);
      return { revealed: true };
    },
  };
}

async function application(context: ExtensionContext, ready: (folder: Uri) => Promise<boolean>, resource?: Uri): Promise<NativeApplication | undefined> {
  const folder = await folderFor(resource);
  if (!folder) {
    if (!workspace.workspaceFolders?.length) void window.showErrorMessage("Open a workspace folder before managing Osprey packages.");
    return undefined;
  }
  if (!await ready(folder)) return undefined;
  const registry = registryOrigin(folder);
  const artifact = applicationPath(context);
  const bridge = hostBridge(context);
  return { key: JSON.stringify([folder.toString(), registry, artifact, bridge.configuredExecutable]), artifact,
    context: nativeContext([folder.toString(), registry]), execute: createRequestExecutor(bridge) };
}

/** Register the package app through the shared native panel host. */
export function registerPackageWorkspace(context: ExtensionContext, ready: (folder: Uri) => Promise<boolean> = async () => true): { dispose(): void } {
  return registerNativePanel(context, { command: PACKAGE_WORKSPACE_COMMAND, view: VIEW_ID, title: PANEL_TITLE,
    loading: "Loading your packages…", prepare: resource => application(context, ready, resource) });
}

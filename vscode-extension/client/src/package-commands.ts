// "Osprey: Add Package" / "Osprey: Remove Package" ([PACKAGE-API]): thin
// editor glue only — QuickPicks over the registry listing and the manifest's
// direct dependencies, a security confirm step, then ONE stdin command to
// the Osprey package frontend, which alone mutates `osprey.toml` and
// `osprey.lock`. Success is the report's semantic status
// (package-frontend.ts), mutations are serialized per project root, unsaved
// manifest/lock editors block any mutation, and a listing without security
// evidence is presented as unverified, never as safe.

import * as fs from "fs";
import { createProjectSerializer } from "./keyed-serializer";
export { createProjectSerializer };
import * as path from "path";
import { fetchListingText, registryUrl, isDevelopmentRegistry } from "./package-api";
import { shipwrightPlatform } from "./client-config";
import { commands, ProgressLocation, Uri, window, workspace } from "vscode";
import {
  PackageEntry,
  dependencyKeys,
  listingOf,
  newestRelease,
  parseListing,
  securityDetail,
} from "./package-listing";
import { buildStdinCommand, runFrontendCommand, semanticSuccess } from "./package-frontend";
import type { PkgReport } from "./package-frontend";

export type { PkgReport } from "./package-frontend";

/** Every UI surface the flows use — injectable so tests observe, not open. */
export interface Ui {
  showQuickPick<T>(items: T[]): Thenable<T | undefined>;
  showWarningMessage(message: string, action?: string): Thenable<{ title: string } | undefined>;
  showErrorMessage(message: string): Thenable<unknown>;
  showInformationMessage(message: string): Thenable<unknown>;
  withProgress<T>(options: { title: string }, task: () => Thenable<T>): Thenable<T>;
}

/** The one mutation channel: one stdin command against the frontend. */
export interface FrontendRunner {
  run(command: string, cwd: string): Promise<PkgReport>;
}

/** Everything the flows need; every piece defaults to the real editor. */
export interface PackageCommandDeps {
  ui: Ui;
  frontend: FrontendRunner;
  /** The configured registry base URL ([PACKAGE-API] `--registry`). */
  registry(root?: string): string;
  projectRoots(): string[];
  /** `osprey.toml` text of a project root, or undefined when absent. */
  manifestText(root: string): string | undefined;
  /** Unsaved editor paths that a mutation would clobber. */
  dirtyFiles(): string[];
  /** The raw `GET /v1/packages` body. */
  fetchListing(root?: string): Promise<string>;
  /** Whether the user cancelled the invoking command. */
  cancelled(): boolean;
}

export const MANIFEST_FILE = "osprey.toml";
export const LOCK_FILE = "osprey.lock";
const DEFAULT_REGISTRY = "http://127.0.0.1:8787"; // Fetch::defaultRegistry
const REGISTRY_CONFIG = "packages.registry";
const FRONTEND_CONFIG = "packages.frontendPath";
const FRONTEND_DEFAULT = "osprey-pkg";

/** The frontend executable: the configured path, else `osprey-pkg` on PATH. */
function defaultFrontend(extensionPath?: string): FrontendRunner {
  return { run: (command, cwd) => {
    const configured = workspace.getConfiguration("osprey").get<string>(FRONTEND_CONFIG);
    const bundled = extensionPath && path.join(extensionPath, "bin", shipwrightPlatform(), process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg");
    return runFrontendCommand(configured || (bundled && fs.existsSync(bundled) ? bundled : FRONTEND_DEFAULT), command, cwd);
  } };
}

const vscodeUi: Ui = {
  showQuickPick: (items) =>
    window.showQuickPick(items as any, {
      matchOnDescription: true,
      placeHolder: "Pick an org-qualified package (@org/name)",
    }) as Thenable<any | undefined>,
  showWarningMessage: (message, action = "Add Anyway") => window.showWarningMessage(message, { modal: true }, action) as any,
  showErrorMessage: (message) => window.showErrorMessage(message) as any,
  showInformationMessage: (message) => window.showInformationMessage(message) as any,
  withProgress: (options, task) =>
    window.withProgress({ location: ProgressLocation.Notification, title: options.title }, () => task()),
};

function registryFor(root?: string): string {
  return workspace.getConfiguration("osprey", root ? Uri.file(root) : undefined).get<string>(REGISTRY_CONFIG) || DEFAULT_REGISTRY;
}

/** The production deps: real editor state, real frontend, configured registry. */
export function defaultPackageDeps(extensionPath?: string): PackageCommandDeps {
  return {
    ui: vscodeUi,
    frontend: defaultFrontend(extensionPath),
    registry: registryFor,
    projectRoots: () => (workspace.workspaceFolders ?? []).map((folder) => folder.uri.fsPath),
    manifestText: (root) => {
      try {
        return fs.readFileSync(path.join(root, MANIFEST_FILE), "utf8");
      } catch {
        return undefined;
      }
    },
    dirtyFiles: () =>
      workspace.textDocuments.filter((doc) => doc.isDirty).map((doc) => doc.fileName),
    fetchListing: (root) => fetchListingText(registryFor(root)),
    cancelled: () => false,
  };
}


function settingsOf(deps: PackageCommandDeps, root: string): string[] {
  return ["--registry", registryUrl(deps.registry(root)).origin, "--project", "."];
}

/** The single frontend command an add boils down to, run from the project root. */
function addCommand(deps: PackageCommandDeps, key: string, root: string): string {
  return buildStdinCommand(["--json", "add", key, ...settingsOf(deps, root)]);
}

function removeCommand(deps: PackageCommandDeps, key: string, root: string): string {
  return buildStdinCommand(["--json", "remove", key, ...settingsOf(deps, root)]);
}

function dirtyBlockers(deps: PackageCommandDeps, root: string): string[] {
  const manifest = path.join(root, MANIFEST_FILE);
  const lock = path.join(root, LOCK_FILE);
  return deps.dirtyFiles().filter((file) => file === manifest || file === lock);
}

function warnDirty(deps: PackageCommandDeps, root: string): boolean {
  const dirty = dirtyBlockers(deps, root);
  if (dirty.length > 0) {
    deps.ui.showErrorMessage(
      `Unsaved changes to ${dirty.map((f) => path.basename(f)).join(" and ")} — save or revert it before changing packages.`,
    );
    return true;
  }
  return false;
}

async function report(deps: PackageCommandDeps, key: string, verb: string, run: () => Promise<PkgReport>): Promise<void> {
  const outcome = await deps.ui.withProgress({ title: `Osprey Packages: ${verb} ${key}` }, run);
  if (semanticSuccess(outcome)) {
    deps.ui.showInformationMessage(`${verb === "Add" ? "Added" : "Removed"} ${key}: ${outcome.lines.join("; ")}`);
  } else {
    deps.ui.showErrorMessage(`${verb} ${key} failed: ${outcome.lines.join("; ") || "no reason given"}`);
  }
}

/** Pick the project to mutate when the workspace has more than one root. */
async function pickProject(deps: PackageCommandDeps): Promise<string | undefined> {
  const roots = deps.projectRoots();
  if (roots.length === 0) {
    deps.ui.showErrorMessage("Open a workspace folder before managing Osprey packages.");
    return undefined;
  }
  if (roots.length === 1) {
    return roots[0];
  }
  const chosen = await deps.ui.showQuickPick(roots.map((root) => ({ label: path.basename(root), root })));
  return chosen?.root;
}

async function chooseAddition(deps: PackageCommandDeps, root: string) {
  try {
    const text = await deps.ui.withProgress({ title: "Osprey Packages: Loading registry" }, () => deps.fetchListing(root));
    if (!isDevelopmentRegistry(deps.registry(root))) {
      deps.ui.showErrorMessage("This registry listing has no security eligibility or advisory evidence. Hosted package installation is unavailable until that API contract is implemented.");
      return undefined;
    }
    const entries = listingOf(parseListing(text));
    if (entries.length) return deps.ui.showQuickPick(entries.map(pickOf));
    deps.ui.showErrorMessage("The registry listing is unavailable or lists no org-qualified packages.");
  } catch (error) {
    deps.ui.showErrorMessage(`Could not read the registry listing: ${String((error as Error).message)}`);
  }
  return undefined;
}

/** Pick a package, review its warning, then serialize the Osprey mutation. */
export function makeAddHandler(deps: PackageCommandDeps, serialize = createProjectSerializer()): () => Promise<void> {
  return async () => {
    const root = await pickProject(deps);
    if (!root || deps.cancelled() || warnDirty(deps, root)) return;
    const entry = await chooseAddition(deps, root);
    if (!entry || deps.cancelled() || warnDirty(deps, root)) return;
    if (!await deps.ui.showWarningMessage(warningFor(entry))) return;
    await queueMutation(deps, serialize, root, entry.key, "Add");
  };
}

function queueMutation(deps: PackageCommandDeps, serialize: ReturnType<typeof createProjectSerializer>, root: string, key: string, verb: "Add" | "Remove") {
  return serialize(root, async () => {
    if (deps.cancelled() || warnDirty(deps, root)) return;
    await report(deps, key, verb, () => deps.frontend.run(
      verb === "Add" ? addCommand(deps, key, root) : removeCommand(deps, key, root), root));
  });
}

function pickOf(entry: PackageEntry) {
  const release = newestRelease(entry);
  return { label: entry.key, description: `e${release.epoch}.r${release.ordinal}`, detail: securityDetail(entry), key: entry.key };
}

function warningFor(entry: { key: string; detail: string }): string {
  return `Add ${entry.key}? ${entry.detail}`;
}

async function chooseRemoval(deps: PackageCommandDeps, root: string) {
  const manifest = deps.manifestText(root);
  if (manifest === undefined) {
    deps.ui.showErrorMessage(`No ${MANIFEST_FILE} found in ${path.basename(root)} — nothing to remove from.`);
    return undefined;
  }
  const result = await deps.ui.withProgress({ title: "Osprey Packages: Reading dependencies" }, () =>
    deps.frontend.run("--json tree --direct --project .", root));
  const keys = dependencyKeys(result);
  if (!keys) { deps.ui.showErrorMessage(`Cannot read dependencies: ${result.lines.join("; ")}`); return undefined; }
  if (keys.length) return deps.ui.showQuickPick(keys.map((label) => ({ label })));
  deps.ui.showErrorMessage(`${MANIFEST_FILE} declares no direct dependencies.`);
  return undefined;
}

/** Select a direct dependency; the Osprey client owns removal and relocking. */
export function makeRemoveHandler(deps: PackageCommandDeps, serialize = createProjectSerializer()): () => Promise<void> {
  return async () => {
    const root = await pickProject(deps);
    if (!root || deps.cancelled() || warnDirty(deps, root)) return;
    const picked = await chooseRemoval(deps, root);
    if (!picked || deps.cancelled() || warnDirty(deps, root)) return;
    if (!await deps.ui.showWarningMessage(`Remove ${picked.label} from ${MANIFEST_FILE} and relock?`, "Remove Package")) return;
    await queueMutation(deps, serialize, root, picked.label, "Remove");
  };
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

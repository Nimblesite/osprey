// The Module Explorer [EDITOR-MODULE-EXPLORER]: each workspace project's namespaces,
// modules and sources from the compiler's checked source graph, and its dependencies
// and verification findings from the Osprey package client. Nothing here resolves names or reads a manifest, and
// a failed source shows its failure, never an empty tree.
import { execFile } from "child_process";
import * as fs from "fs";
import * as path from "path";
import {
  commands, EventEmitter, RelativePattern, Selection, ThemeIcon, TreeItem, TreeItemCollapsibleState,
  Uri, window, workspace,
} from "vscode";
import type { Event, ExtensionContext, TreeDataProvider } from "vscode";
import type { FrontendRunner } from "./package-commands";
import { MANIFEST_FILE } from "./package-commands";

export const MODULE_EXPLORER_VIEW = "osprey.moduleExplorer";
export const OPEN_SOURCE_COMMAND = "osprey.modules.open";
export const REFRESH_COMMAND = "osprey.modules.refresh";
export const MANAGE_PACKAGES_COMMAND = "osprey.modules.managePackages";
const GRAPH_TIMEOUT_MS = 120_000;
const NO_LOCK_CODE = "tree.no-lock";

/** One namespace or module a source declares. Lines are one-based, columns zero-based. */
export interface Declaration {
  readonly kind: string;
  readonly name: string;
  readonly source: string;
  readonly line: number;
  readonly column: number;
}

export interface SourceGraph {
  readonly sources: readonly string[];
  readonly declarations: readonly Declaration[];
}

export type Loaded<T> = { readonly ok: T } | { readonly error: string };

export interface ModuleNode {
  readonly id: string;
  readonly label: string;
  readonly parent?: ModuleNode;
  readonly icon?: string;
  readonly description?: string;
  readonly tooltip?: string;
  /** Opens here when selected. */
  readonly location?: { readonly file: string; readonly line: number; readonly column: number };
  readonly children?: readonly ModuleNode[];
  /** The project this node loads on expansion. */
  readonly projectRoot?: string;
}

function isDeclaration(item: any): item is Declaration {
  return typeof item?.kind === "string" && typeof item?.name === "string" &&
    typeof item?.source === "string" && Number.isSafeInteger(item?.line) && Number.isSafeInteger(item?.column);
}

/** Reject anything but a complete graph, so a partial one cannot read as a small project. */
export function parseSourceGraph(text: string): SourceGraph | undefined {
  try {
    const graph = JSON.parse(text);
    return Array.isArray(graph?.sources) && graph.sources.every((source: unknown) => typeof source === "string") &&
      Array.isArray(graph?.declarations) && graph.declarations.every(isDeclaration)
      ? { sources: graph.sources, declarations: graph.declarations }
      : undefined;
  } catch {
    return undefined;
  }
}

function attach(parent: ModuleNode, build: (self: ModuleNode) => readonly ModuleNode[]): ModuleNode {
  const self = parent as { children?: readonly ModuleNode[] };
  self.children = build(parent);
  return parent;
}

function stateNode(parent: ModuleNode, key: string, label: string, detail: string, icon = "error"): ModuleNode {
  return { id: `${parent.id}/${key}`, parent, label, description: detail, tooltip: detail, icon };
}

function fileNode(parent: ModuleNode, root: string, file: string, line = 1, column = 0): ModuleNode {
  return {
    id: `${parent.id}/${file}`, parent, label: path.relative(root, file) || path.basename(file),
    icon: "file-code", location: { file, line, column },
  };
}

function declarationNodes(parent: ModuleNode, root: string, graph: SourceGraph): ModuleNode[] {
  const names = [...new Set(graph.declarations.map((item) => item.name))].sort();
  return names.map((name) => {
    const homes = graph.declarations.filter((item) => item.name === name);
    const first = homes[0];
    const node: ModuleNode = {
      id: `${parent.id}/${first.kind}:${name}`, parent, label: name,
      icon: first.kind === "namespace" ? "symbol-namespace" : "symbol-module",
      location: homes.length === 1 ? { file: first.source, line: first.line, column: first.column } : undefined,
    };
    return homes.length === 1
      ? node
      : attach(node, (self) => homes.map((home) => fileNode(self, root, home.source, home.line, home.column)));
  });
}

/** One package of `osprey-pkg tree --graph`, the editor's projection of the lock. */
export interface LockedPackage {
  readonly key: string;
  readonly version: string;
  readonly dependencies: readonly string[];
  readonly findings: readonly string[];
  readonly present: boolean;
  /** The local checkout standing in for this release, when one does. */
  readonly override?: string;
}

export interface PackageGraph {
  readonly direct: readonly string[];
  readonly packages: readonly LockedPackage[];
  readonly findings: readonly string[];
}

const isTextList = (value: any): value is string[] =>
  Array.isArray(value) && value.every((item) => typeof item === "string");

function isLockedPackage(item: any): item is LockedPackage {
  return typeof item?.key === "string" && typeof item?.version === "string" && typeof item?.present === "boolean" &&
    isTextList(item?.dependencies) && isTextList(item?.findings) &&
    (item?.override === undefined || typeof item.override === "string");
}

/** Reject anything but a complete graph, so a partial one cannot read as a safe dependency. */
export function parsePackageGraph(text: string): PackageGraph | undefined {
  try {
    const graph = JSON.parse(text);
    return isTextList(graph?.direct) && isTextList(graph?.findings) &&
      Array.isArray(graph?.packages) && graph.packages.every(isLockedPackage)
      ? { direct: graph.direct, packages: graph.packages, findings: graph.findings }
      : undefined;
  } catch {
    return undefined;
  }
}

function packageNode(owner: ModuleNode, key: string, graph: PackageGraph, ancestors: readonly string[]): ModuleNode {
  const locked = graph.packages.find((item) => item.key === key);
  const role = ancestors.length ? "transitive" : "direct";
  const problems = [...(locked?.findings ?? []), ...(locked && !locked.present ? ["not installed"] : [])];
  const node: ModuleNode = {
    id: `${owner.id}/${key}`, parent: owner, label: key,
    icon: !locked || problems.length ? "warning" : "package",
    description: [
      locked?.version ?? "not locked", role, ...(locked?.override ? [`local: ${locked.override}`] : []), ...problems,
    ].join(" · "),
    tooltip: problems.join("\n") || undefined,
  };
  const trail = [...ancestors, key];
  return attach(node, (self) => (locked?.dependencies ?? [])
    .filter((next) => !trail.includes(next))
    .map((next) => packageNode(self, next, graph, trail)));
}

function dependencyNodes(self: ModuleNode, packages: Loaded<PackageGraph>): readonly ModuleNode[] {
  if ("error" in packages) {
    return [stateNode(self, "unavailable", "Dependencies unavailable", packages.error)];
  }
  const graph = packages.ok;
  const states = graph.findings.map((code) => code === NO_LOCK_CODE
    ? stateNode(self, code, "No lock file", "Versions are unknown until the project is locked.", "warning")
    : stateNode(self, code, "Verification finding", code, "warning"));
  const direct = graph.direct.map((key) => packageNode(self, key, graph, []));
  return states.length || direct.length
    ? [...states, ...direct]
    : [stateNode(self, "none", "No dependencies", "The package client reports none.", "info")];
}

/** The groups under one project, from what the compiler and package client answered. */
export function projectChildren(
  project: ModuleNode, graph: Loaded<SourceGraph>, packages: Loaded<PackageGraph>,
): ModuleNode[] {
  const root = project.projectRoot ?? "";
  const group = (key: string, label: string, icon: string) =>
    ({ id: `${project.id}/${key}`, parent: project, label, icon }) as ModuleNode;
  const dependencies = attach(group("dependencies", "Dependencies", "package"), (self) =>
    dependencyNodes(self, packages));
  if ("error" in graph) {
    return [stateNode(project, "analysis", "Analysis failed", graph.error), dependencies];
  }
  return [
    attach(group("modules", "Modules", "symbol-module"), (self) =>
      graph.ok.declarations.length
        ? declarationNodes(self, root, graph.ok)
        : [stateNode(self, "none", "No namespaces or modules", "The compiler found none in this project.", "info")]),
    attach(group("sources", "Sources", "files"), (self) =>
      [...graph.ok.sources].sort().map((file) => fileNode(self, root, file))),
    dependencies,
  ];
}

export interface ModuleExplorerDeps {
  projectRoots(): Thenable<readonly string[]> | readonly string[];
  sourceGraph(root: string): Promise<Loaded<SourceGraph>>;
  packageGraph(root: string): Promise<Loaded<PackageGraph>>;
  /** Files found without running anything, for an untrusted workspace. */
  plainSources(root: string): Thenable<readonly string[]>;
  /** Whether this project has unsaved edits the compiler cannot see. */
  hasUnsavedEdits(root: string): boolean;
  trusted(): boolean;
}

/** The tree; a project's children load on expansion and reload on refresh. */
export class ModuleExplorerProvider implements TreeDataProvider<ModuleNode> {
  private readonly changed = new EventEmitter<ModuleNode | undefined>();
  private readonly loaded = new Map<string, Promise<ModuleNode[]>>();
  readonly onDidChangeTreeData: Event<ModuleNode | undefined> = this.changed.event;

  constructor(private readonly deps: ModuleExplorerDeps) {}

  refresh(): void {
    this.loaded.clear();
    this.changed.fire(undefined);
  }

  getParent(node: ModuleNode): ModuleNode | undefined {
    return node.parent;
  }

  getTreeItem(node: ModuleNode): TreeItem {
    const expandable = node.projectRoot !== undefined || (node.children?.length ?? 0) > 0;
    const item = new TreeItem(node.label, expandable ? TreeItemCollapsibleState.Collapsed : TreeItemCollapsibleState.None);
    item.id = node.id;
    item.description = node.description;
    item.tooltip = node.tooltip;
    item.iconPath = node.icon ? new ThemeIcon(node.icon) : undefined;
    item.contextValue = node.projectRoot ? "ospreyProject" : undefined;
    item.command = node.location
      ? { command: OPEN_SOURCE_COMMAND, title: "Open", arguments: [node.location] }
      : undefined;
    return item;
  }

  async getChildren(node?: ModuleNode): Promise<ModuleNode[]> {
    if (!node) {
      return (await this.deps.projectRoots()).map((root) =>
        ({ id: root, label: path.basename(root), projectRoot: root, icon: "project", description: root }));
    }
    if (node.projectRoot === undefined) {
      return [...(node.children ?? [])];
    }
    const pending = this.loaded.get(node.projectRoot) ?? this.withLoading(this.load(node));
    this.loaded.set(node.projectRoot, pending);
    return pending;
  }

  private withLoading(work: Promise<ModuleNode[]>): Promise<ModuleNode[]> {
    return Promise.resolve(window.withProgress({ location: { viewId: MODULE_EXPLORER_VIEW } }, () => work));
  }

  private async load(project: ModuleNode): Promise<ModuleNode[]> {
    const root = project.projectRoot ?? "";
    if (!this.deps.trusted()) {
      const files = await this.deps.plainSources(root);
      return [
        stateNode(project, "untrusted", "Analysis needs a trusted workspace", "Trust this workspace to let the compiler analyse it.", "shield"),
        attach({ id: `${project.id}/sources`, parent: project, label: "Sources", icon: "files" }, (self) =>
          [...files].sort().map((file) => fileNode(self, root, file))),
      ];
    }
    const [graph, packages] = await Promise.all([this.deps.sourceGraph(root), this.deps.packageGraph(root)]);
    const children = projectChildren(project, graph, packages);
    return this.deps.hasUnsavedEdits(root)
      ? [stateNode(project, "stale", "Unsaved edits are not analysed", "Save to refresh this project.", "warning"), ...children]
      : children;
  }

  /** The Sources entry for a file, when its project is already loaded. */
  async sourceNode(file: string): Promise<ModuleNode | undefined> {
    for (const pending of this.loaded.values()) {
      const groups = await pending;
      const sources = groups.find((group) => group.label === "Sources");
      const found = sources?.children?.find((node) => node.location?.file === file);
      if (found) return found;
    }
    return undefined;
  }
}

/** Run `osprey <project> --source-graph`, answering its diagnostics on failure. */
export function compilerSourceGraph(compiler: () => string): (root: string) => Promise<Loaded<SourceGraph>> {
  return (root) => new Promise((resolve) => {
    execFile(compiler(), [root, "--source-graph", "--no-warnings"],
      { cwd: root, timeout: GRAPH_TIMEOUT_MS, maxBuffer: 64 * 1024 * 1024, windowsHide: true },
      (error, stdout, stderr) => {
        const graph = error ? undefined : parseSourceGraph(String(stdout));
        resolve(graph ? { ok: graph } : { error: String(stderr).trim() || error?.message || "The compiler answered no source graph." });
      });
  });
}

/** The locked graph with its verification findings, through the package client. */
export function clientPackageGraph(frontend: FrontendRunner): (root: string) => Promise<Loaded<PackageGraph>> {
  return async (root) => {
    try {
      const report = await frontend.run("--json tree --graph --project .", root);
      const graph = report.status === 0 && report.lines.length === 1 ? parsePackageGraph(report.lines[0]) : undefined;
      return graph ? { ok: graph } : { error: report.lines.join("; ") || `The package client answered status ${report.status}.` };
    } catch (error) {
      return { error: error instanceof Error ? error.message : String(error) };
    }
  };
}

const SOURCE_GLOB = "**/*.{osp,ospml,py}";
const GENERATED_GLOB = "{**/node_modules/**,**/.osprey/**,**/target/**}";

async function plainSources(root: string): Promise<string[]> {
  const files = await workspace.findFiles(new RelativePattern(root, SOURCE_GLOB), GENERATED_GLOB);
  return files.map((file) => file.fsPath);
}

function isWithin(root: string, file: string): boolean {
  const relative = path.relative(root, file);
  return relative !== "" && !relative.startsWith("..") && !path.isAbsolute(relative);
}

/** Whether `file`, with symlinks resolved, lies under one of `roots`. */
export function isContained(file: string, roots: readonly string[]): boolean {
  try {
    const real = fs.realpathSync(file);
    return roots.some((root) => isWithin(fs.realpathSync(root), real));
  } catch {
    return false;
  }
}

async function workspaceProjects(): Promise<string[]> {
  const manifests = await workspace.findFiles(`**/${MANIFEST_FILE}`, GENERATED_GLOB);
  return manifests.map((manifest) => path.dirname(manifest.fsPath)).sort();
}

/** Open a source at its position, refusing anything outside `roots`. */
export async function openSource(location: NonNullable<ModuleNode["location"]>, roots: readonly string[]): Promise<void> {
  if (!isContained(location.file, roots)) {
    window.showErrorMessage(`${location.file} is outside this workspace.`);
    return;
  }
  const document = await workspace.openTextDocument(Uri.file(location.file));
  const line = Math.min(Math.max(location.line - 1, 0), Math.max(document.lineCount - 1, 0));
  const start = document.lineAt(line).range.start.translate(0, location.column);
  await window.showTextDocument(document, { selection: new Selection(start, start), preview: true });
}

/** Register the view, its commands, and refresh on source and manifest changes. */
export function registerModuleExplorer(
  context: ExtensionContext, compiler: () => string, frontend: FrontendRunner,
): ModuleExplorerProvider {
  const provider = new ModuleExplorerProvider({
    projectRoots: workspaceProjects,
    sourceGraph: compilerSourceGraph(compiler),
    packageGraph: clientPackageGraph(frontend),
    plainSources,
    hasUnsavedEdits: (root) => workspace.textDocuments.some((doc) => doc.isDirty && isWithin(root, doc.fileName)),
    trusted: () => workspace.isTrusted,
  });
  const view = window.createTreeView(MODULE_EXPLORER_VIEW, { treeDataProvider: provider, showCollapseAll: true });
  const watcher = workspace.createFileSystemWatcher("**/{*.osp,*.ospml,*.py,osprey.toml,osprey.lock}");
  context.subscriptions.push(
    view,
    watcher,
    commands.registerCommand(REFRESH_COMMAND, () => provider.refresh()),
    commands.registerCommand(OPEN_SOURCE_COMMAND, (location: NonNullable<ModuleNode["location"]>) =>
      openSource(location, (workspace.workspaceFolders ?? []).map((folder) => folder.uri.fsPath))),
    commands.registerCommand(MANAGE_PACKAGES_COMMAND, (node?: ModuleNode) =>
      commands.executeCommand("osprey.packages.open", node?.projectRoot ? Uri.file(node.projectRoot) : undefined)),
    watcher.onDidChange(() => provider.refresh()),
    watcher.onDidCreate(() => provider.refresh()),
    watcher.onDidDelete(() => provider.refresh()),
    window.onDidChangeActiveTextEditor(async (editor) => {
      const node = editor && view.visible ? await provider.sourceNode(editor.document.uri.fsPath) : undefined;
      if (node) await view.reveal(node, { select: true, focus: false });
    }),
  );
  return provider;
}

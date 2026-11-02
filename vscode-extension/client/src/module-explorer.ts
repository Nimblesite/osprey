// The Module Explorer [EDITOR-MODULE-EXPLORER]: each workspace project's namespaces,
// modules and sources from the compiler's checked source graph, and its dependencies
// from the Osprey package client. Nothing here resolves names or reads a manifest, and
// a failed source shows its failure, never an empty tree.
import { execFile } from "child_process";
import * as path from "path";
import {
  commands, EventEmitter, Selection, ThemeIcon, TreeItem, TreeItemCollapsibleState,
  Uri, window, workspace,
} from "vscode";
import type { Event, ExtensionContext, TreeDataProvider } from "vscode";
import { dependencyKeys } from "./package-listing";
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

/** `osprey-pkg tree` lines under the project line, nested by their indentation. */
function lockedNodes(parent: ModuleNode, lines: readonly string[]): ModuleNode[] {
  const rows = lines.slice(1).map((line) => ({ depth: line.length - line.trimStart().length, text: line.trim() }));
  const build = (owner: ModuleNode, start: number, depth: number): ModuleNode[] => {
    const nodes: ModuleNode[] = [];
    for (let at = start; at < rows.length && rows[at].depth >= depth; at += 1) {
      if (rows[at].depth === depth) {
        const unlocked = rows[at].text.endsWith("(not locked)");
        nodes.push(attach({
          id: `${owner.id}/${rows[at].text}#${at}`, parent: owner, label: rows[at].text,
          icon: unlocked ? "warning" : "package", description: owner === parent ? "direct" : "transitive",
        }, (self) => (rows[at + 1]?.depth > depth ? build(self, at + 1, rows[at + 1].depth) : [])));
      }
    }
    return nodes;
  };
  return rows.length ? build(parent, 0, rows[0].depth) : [];
}

function dependencyNodes(
  self: ModuleNode, direct: Loaded<readonly string[]>, locked: Loaded<readonly string[]>,
): readonly ModuleNode[] {
  if ("ok" in locked) {
    const nodes = lockedNodes(self, locked.ok);
    return nodes.length ? nodes : [stateNode(self, "none", "No dependencies", "The lock file records none.", "info")];
  }
  const lockState = stateNode(self, "lock", locked.error.startsWith(NO_LOCK_CODE) ? "No lock file" : "Lock unavailable", locked.error,
    locked.error.startsWith(NO_LOCK_CODE) ? "warning" : "error");
  if ("error" in direct) {
    return [lockState, stateNode(self, "unavailable", "Dependencies unavailable", direct.error)];
  }
  return [lockState, ...direct.ok.map((key) =>
    ({ id: `${self.id}/${key}`, parent: self, label: key, icon: "package", description: "direct, not locked" }))];
}

/** The groups under one project, from what the compiler and package client answered. */
export function projectChildren(
  project: ModuleNode, graph: Loaded<SourceGraph>, direct: Loaded<readonly string[]>, locked: Loaded<readonly string[]>,
): ModuleNode[] {
  const root = project.projectRoot ?? "";
  const group = (key: string, label: string, icon: string) =>
    ({ id: `${project.id}/${key}`, parent: project, label, icon }) as ModuleNode;
  const dependencies = attach(group("dependencies", "Dependencies", "package"), (self) =>
    dependencyNodes(self, direct, locked));
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
  directDependencies(root: string): Promise<Loaded<readonly string[]>>;
  lockedTree(root: string): Promise<Loaded<readonly string[]>>;
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
    const pending = this.loaded.get(node.projectRoot) ?? this.load(node);
    this.loaded.set(node.projectRoot, pending);
    return pending;
  }

  private async load(project: ModuleNode): Promise<ModuleNode[]> {
    if (!this.deps.trusted()) {
      const detail = "Trust this workspace to let the compiler analyse it.";
      return [stateNode(project, "untrusted", "Analysis needs a trusted workspace", detail, "shield")];
    }
    const root = project.projectRoot ?? "";
    const [graph, direct, locked] = await Promise.all(
      [this.deps.sourceGraph(root), this.deps.directDependencies(root), this.deps.lockedTree(root)]);
    return projectChildren(project, graph, direct, locked);
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

async function clientReport(
  frontend: FrontendRunner, command: string, root: string,
  answer: (report: { status: number; lines: readonly string[] }) => readonly string[] | undefined,
): Promise<Loaded<readonly string[]>> {
  try {
    const report = await frontend.run(command, root);
    const lines = answer(report);
    return lines ? { ok: lines } : { error: report.lines.join("; ") || `The package client answered status ${report.status}.` };
  } catch (error) {
    return { error: error instanceof Error ? error.message : String(error) };
  }
}

/** Direct dependencies through the same report the Remove command reads. */
export function clientDependencies(frontend: FrontendRunner): (root: string) => Promise<Loaded<readonly string[]>> {
  return (root) => clientReport(frontend, "--json tree --direct --project .", root, dependencyKeys);
}

/** The locked graph, direct and transitive, as the package client prints it. */
export function clientLockedTree(frontend: FrontendRunner): (root: string) => Promise<Loaded<readonly string[]>> {
  return (root) => clientReport(frontend, "--json tree --project .", root,
    (report) => (report.status === 0 ? report.lines : undefined));
}

async function workspaceProjects(): Promise<string[]> {
  const manifests = await workspace.findFiles(`**/${MANIFEST_FILE}`, "{**/node_modules/**,**/.osprey/**,**/target/**}");
  return manifests.map((manifest) => path.dirname(manifest.fsPath)).sort();
}

async function openSource(location: NonNullable<ModuleNode["location"]>): Promise<void> {
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
    directDependencies: clientDependencies(frontend),
    lockedTree: clientLockedTree(frontend),
    trusted: () => workspace.isTrusted,
  });
  const view = window.createTreeView(MODULE_EXPLORER_VIEW, { treeDataProvider: provider, showCollapseAll: true });
  const watcher = workspace.createFileSystemWatcher("**/{*.osp,*.ospml,*.py,osprey.toml,osprey.lock}");
  context.subscriptions.push(
    view,
    watcher,
    commands.registerCommand(REFRESH_COMMAND, () => provider.refresh()),
    commands.registerCommand(OPEN_SOURCE_COMMAND, openSource),
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

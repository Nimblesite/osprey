// The Test Explorer item tree ([TESTING-VSCODE]): one item per test file and
// directory, leaves from `--list-tests`, their docs, and workspace discovery
// through a bounded queue and file watchers.

import { existsSync } from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import { testDescription, testDocOf, type TestDoc } from "./test-explorer-docs";
import {
  discoveryOutcome,
  fileTestId,
  leafTestId,
  testRangeStart,
  type DiscoveredTest,
  type TestListParse,
} from "./test-explorer-parse";
import { runCompiler } from "./test-explorer-process";
import { dropSkipWarnings } from "./test-explorer-skips";

/** The discovery glob — both flavors ([TESTING-FILE-CONVENTION]). */
export const TEST_FILE_GLOB = "**/*.test.{osp,ospml}";
export const DIRECTORY_ID_PREFIX = "osprey-test-directory:";

/** A test file's Explorer label: workspace-relative path, or basename outside one. */
export function testFileLabel(uri: vscode.Uri): string {
  return vscode.workspace.getWorkspaceFolder(uri) !== undefined
    ? vscode.workspace.asRelativePath(uri, false)
    : path.basename(uri.fsPath);
}

function getOrCreateFileItem(
  controller: vscode.TestController,
  uri: vscode.Uri,
): vscode.TestItem {
  const id = fileTestId(uri.toString());
  const workspaceFolder = vscode.workspace.getWorkspaceFolder(uri);
  if (workspaceFolder === undefined) {
    const existing = controller.items.get(id);
    if (existing !== undefined) {
      return existing;
    }
    const item = controller.createTestItem(id, testFileLabel(uri), uri);
    controller.items.add(item);
    return item;
  }

  const parts = vscode.workspace
    .asRelativePath(uri, false)
    .split(/[\\/]+/)
    .filter((part) => part.length > 0);
  const fileLabel = parts.pop() ?? path.basename(uri.fsPath);
  let children = controller.items;
  let directoryPath = "";
  for (const part of parts) {
    directoryPath =
      directoryPath.length === 0 ? part : `${directoryPath}/${part}`;
    const directoryId = `${DIRECTORY_ID_PREFIX}${workspaceFolder.uri.toString()}:${directoryPath}`;
    let directory = children.get(directoryId);
    if (directory === undefined) {
      directory = controller.createTestItem(directoryId, part);
      children.add(directory);
    }
    children = directory.children;
  }

  const existing = children.get(id);
  if (existing !== undefined) {
    return existing;
  }
  const item = controller.createTestItem(id, fileLabel, uri);
  children.add(item);
  return item;
}

/**
 * Every discovered case's documentation, keyed by leaf TestItem id
 * ([TESTING-DOC]). Discovery refreshes a file's entries wholesale, so a doc
 * comment deleted from the source disappears here on the next resolve. The
 * Test Explorer reads it for the inline description, the "Show Documentation"
 * detail panel, and the documentation header on a failure message.
 */
export const testDocs = new Map<string, TestDoc>();

/** The documentation discovery recorded for one leaf id, if any. */
export function testDocFor(leafId: string): TestDoc | undefined {
  return testDocs.get(leafId);
}

/** Every leaf id currently carrying documentation (exported for tests). */
export function documentedTestIds(): string[] {
  return [...testDocs.keys()];
}

function makeLeafItem(
  controller: vscode.TestController,
  uri: vscode.Uri,
  test: DiscoveredTest,
): vscode.TestItem {
  const id = leafTestId(uri.toString(), test.name);
  const leaf = controller.createTestItem(id, test.name, uri);
  const start = testRangeStart(test);
  leaf.range = new vscode.Range(
    start.line,
    start.character,
    start.line,
    start.character,
  );
  // The whole doc renders greyed beside the case name — truncated in the row,
  // entire in the row's hover, which is the only tooltip a TestItem has. The
  // Markdown form is stashed for the detail panel and failure messages.
  // Implements [TESTING-DOC-VSCODE].
  const description = testDescription(test);
  if (description !== undefined) {
    leaf.description = description;
  }
  testDocs.set(id, testDocOf(test));
  return leaf;
}

/** Drop every doc recorded for `uri`'s cases — called before a re-resolve. */
function forgetDocs(uri: vscode.Uri): void {
  const prefix = leafTestId(uri.toString(), "");
  for (const id of [...testDocs.keys()]) {
    if (id.startsWith(prefix)) {
      testDocs.delete(id);
    }
  }
}

function applyDiscovery(
  controller: vscode.TestController,
  item: vscode.TestItem,
  uri: vscode.Uri,
  outcome: TestListParse,
): void {
  forgetDocs(uri);
  if (!outcome.ok) {
    item.error = outcome.error;
    item.children.replace([]);
    return;
  }
  item.error = undefined;
  item.children.replace(
    outcome.tests.map((test) => makeLeafItem(controller, uri, test)),
  );
}

/**
 * (Re-)discover one test file: create/refresh its file-level item and replace
 * its children from `--list-tests` ([TESTING-LIST]). A parse failure surfaces
 * as the file item's error message (discovery skips the type gate, so this
 * only happens on syntax errors).
 */
export async function refreshTestFile(
  controller: vscode.TestController,
  uri: vscode.Uri,
  compiler: string,
  token?: vscode.CancellationToken,
): Promise<vscode.TestItem> {
  const item = getOrCreateFileItem(controller, uri);
  const result = await runCompiler(
    compiler,
    [uri.fsPath, "--list-tests"],
    path.dirname(uri.fsPath),
    process.env,
    token,
  );
  applyDiscovery(controller, item, uri, discoveryOutcome(result));
  return item;
}

/** Drop a deleted test file's item from the tree, and its skip warnings. */
export function removeTestFile(
  controller: vscode.TestController,
  uri: vscode.Uri,
): void {
  dropSkipWarnings(uri);
  const id = fileTestId(uri.toString());
  const item = fileItems(controller.items).find(
    (candidate) => candidate.id === id,
  );
  if (item === undefined) {
    return;
  }
  const parent = item.parent;
  if (parent === undefined) {
    controller.items.delete(id);
    return;
  }
  parent.children.delete(id);
  let emptyDirectory: vscode.TestItem | undefined = parent;
  while (
    emptyDirectory !== undefined &&
    emptyDirectory.id.startsWith(DIRECTORY_ID_PREFIX) &&
    emptyDirectory.children.size === 0
  ) {
    const ancestor: vscode.TestItem | undefined = emptyDirectory.parent;
    if (ancestor === undefined) {
      controller.items.delete(emptyDirectory.id);
    } else {
      ancestor.children.delete(emptyDirectory.id);
    }
    emptyDirectory = ancestor;
  }
}

/** Every file-level item, however deep its directory nesting. */
export function fileItems(items: vscode.TestItemCollection): vscode.TestItem[] {
  const found: vscode.TestItem[] = [];
  items.forEach((item) => {
    if (item.id.startsWith(DIRECTORY_ID_PREFIX)) {
      found.push(...fileItems(item.children));
    } else {
      found.push(item);
    }
  });
  return found;
}

/** Build output holds copies of the suites; they never belong in the tree. */
const BUILD_OUTPUT_GLOB = "**/{target,node_modules}/**";

/** Whether `uri` lies under a build-output directory of its workspace. */
export function isBuildOutput(uri: vscode.Uri): boolean {
  return vscode.workspace
    .asRelativePath(uri, false)
    .split(/[\\/]+/)
    .some((part) => part === "target" || part === "node_modules");
}

/** Whether `uri` names a test file ([TESTING-FILE-CONVENTION]). */
export function isTestFile(uri: vscode.Uri): boolean {
  return /\.test\.osp(ml)?$/.test(uri.fsPath);
}

/**
 * The project a source file belongs to: the nearest directory holding an
 * `osprey.toml`, or the file's own directory when no manifest encloses it.
 */
export function projectRootOf(uri: vscode.Uri): string {
  const start = path.dirname(uri.fsPath);
  for (let dir = start; ; dir = path.dirname(dir)) {
    if (existsSync(path.join(dir, "osprey.toml"))) {
      return dir;
    }
    if (path.dirname(dir) === dir) {
      return start;
    }
  }
}

/** Parallel `--list-tests` processes; discovery is a parse, so cores bound it. */
const DISCOVERY_WORKERS = Math.max(2, os.availableParallelism());

/**
 * Coalesced discovery: a file resolves at most once at a time, a change landing
 * mid-resolve queues exactly one more pass, and at most `workers` resolves run
 * together. `schedule` settles once the queue drains.
 */
export function makeDiscoveryQueue(
  refresh: (uri: vscode.Uri) => Promise<unknown>,
  workers: number = DISCOVERY_WORKERS,
): (uris: readonly vscode.Uri[]) => Promise<void> {
  const pending = new Map<string, vscode.Uri>();
  const running = new Set<string>();
  const drained: (() => void)[] = [];
  const pump = (): void => {
    for (const [key, uri] of pending) {
      if (running.size >= workers) {
        break;
      }
      if (!running.has(key)) {
        pending.delete(key);
        running.add(key);
        void refresh(uri)
          .catch(() => undefined)
          .finally(() => {
            running.delete(key);
            pump();
          });
      }
    }
    if (running.size === 0 && pending.size === 0) {
      for (const settle of drained.splice(0)) {
        settle();
      }
    }
  };
  return (uris) => {
    for (const uri of uris) {
      pending.set(uri.toString(), uri);
    }
    const done = new Promise<void>((settle) => drained.push(settle));
    pump();
    return done;
  };
}

/**
 * Full workspace scan, also the Testing view's Refresh: resolves every test
 * file outside build output and drops items whose file is gone from disk.
 */
export async function scanWorkspaceTestFiles(
  controller: vscode.TestController,
  schedule: (uris: readonly vscode.Uri[]) => Promise<void>,
  findFiles: (glob: string) => Thenable<vscode.Uri[]> = (glob) =>
    vscode.workspace.findFiles(glob, BUILD_OUTPUT_GLOB),
): Promise<void> {
  const uris = (await findFiles(TEST_FILE_GLOB)).filter(
    (uri) => !isBuildOutput(uri),
  );
  const found = new Set(uris.map((uri) => fileTestId(uri.toString())));
  for (const item of fileItems(controller.items)) {
    // A file the watcher added mid-scan is absent from `found` but on disk.
    if (
      !found.has(item.id) &&
      item.uri !== undefined &&
      !existsSync(item.uri.fsPath)
    ) {
      removeTestFile(controller, item.uri);
    }
  }
  await schedule(uris);
}

/**
 * The runnable items a request targets. Directory selections (including every
 * root for Run All) expand to the files beneath them; file and case selections
 * remain intact for run planning.
 */
export function requestedItems(
  controller: vscode.TestController,
  request: Pick<vscode.TestRunRequest, "include">,
): vscode.TestItem[] {
  const requested =
    request.include ??
    (() => {
      const roots: vscode.TestItem[] = [];
      controller.items.forEach((item) => roots.push(item));
      return roots;
    })();
  const runnable: vscode.TestItem[] = [];
  const expand = (item: vscode.TestItem): void => {
    if (!item.id.startsWith(DIRECTORY_ID_PREFIX)) {
      runnable.push(item);
      return;
    }
    item.children.forEach(expand);
  };
  for (const item of requested) {
    expand(item);
  }
  return runnable;
}

/**
 * The tree's reactions (exported so they are directly testable): a test file
 * created or changed re-resolves; deleted, it leaves the tree; any other source
 * re-resolves every test file of its project, since a suite's cases depend on
 * the modules it imports; `rescan` re-resolves the whole workspace.
 */
export function makeDiscovery(
  controller: vscode.TestController,
  resolveCompiler: () => string,
  findFiles?: (glob: string) => Thenable<vscode.Uri[]>,
): {
  changed: (uri: vscode.Uri) => Promise<void>;
  deleted: (uri: vscode.Uri) => Promise<void>;
  rescan: () => Promise<void>;
} {
  // A file deleted while its discovery waited in the queue stays out of the tree.
  const schedule = makeDiscoveryQueue((uri) =>
    existsSync(uri.fsPath)
      ? refreshTestFile(controller, uri, resolveCompiler())
      : Promise.resolve(removeTestFile(controller, uri)),
  );
  const project = (uri: vscode.Uri): Promise<void> => {
    const root = projectRootOf(uri);
    return schedule(
      fileItems(controller.items)
        .map((item) => item.uri)
        .filter(
          (file): file is vscode.Uri =>
            file !== undefined && projectRootOf(file) === root,
        ),
    );
  };
  const changed = (uri: vscode.Uri): Promise<void> =>
    isBuildOutput(uri)
      ? Promise.resolve()
      : isTestFile(uri)
        ? schedule([uri])
        : project(uri);
  return {
    changed,
    deleted: (uri) => {
      if (isTestFile(uri)) {
        removeTestFile(controller, uri);
        return Promise.resolve();
      }
      return changed(uri);
    },
    rescan: () => scanWorkspaceTestFiles(controller, schedule, findFiles),
  };
}

/** A rebuild writes the binary in bursts; one rescan follows the last write. */
const REBUILD_SETTLE_MS = 750;

/**
 * Rescans after the compiler binary is rebuilt, since a new compiler may list
 * different cases. A bare command name resolved through PATH has no file to
 * watch.
 */
export function watchCompiler(
  compiler: string,
  rescan: () => Promise<void>,
): vscode.Disposable {
  if (!path.isAbsolute(compiler)) {
    return new vscode.Disposable(() => undefined);
  }
  const watcher = vscode.workspace.createFileSystemWatcher(
    new vscode.RelativePattern(
      vscode.Uri.file(path.dirname(compiler)),
      path.basename(compiler),
    ),
  );
  let timer: NodeJS.Timeout | undefined;
  const rebuilt = (): void => {
    clearTimeout(timer);
    timer = setTimeout(() => void rescan(), REBUILD_SETTLE_MS);
  };
  watcher.onDidCreate(rebuilt);
  watcher.onDidChange(rebuilt);
  return vscode.Disposable.from(
    watcher,
    new vscode.Disposable(() => clearTimeout(timer)),
  );
}

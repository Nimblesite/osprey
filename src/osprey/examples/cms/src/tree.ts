// osprey: namespace=cms
// osprey: module=Tree

/**
 * The files the editor may open: every file under every root that passes
 * [CMS-PATH-SAFE], so the tree never shows a file that would then be refused.
 * Hidden names are never listed. [CMS-API-TREE]
 */
import { type Config } from "./settings.ts";
import { kindFor, safePath } from "./paths.ts";

/** One file of the tree. */
export interface Entry {
  readonly path: string;
  readonly category: string;
}

/** `entry`, a full child path from `listDir`, relative to the repository and without a folder's trailing `/`. */
function relative(config: Config, entry) {
  const local = drop(entry, satAdd(byteLength(config.repo), 1));
  return endsWith(local, "/") ? take(local, satSub(byteLength(local), 1)) : local;
}

function lastName(path) {
  const names = unwrapOr(split(path, "/"), []);
  return unwrapOr(listGet(names, satSub(names.length, 1)), "");
}

/** The files at and below one listed entry. */
function expand(config, entry) {
  const path = relative(config, entry);
  if (startsWith(lastName(path), ".")) {
    return [];
  }
  return endsWith(entry, "/") ? walk(config, path) : [path];
}

/** Every file below `folder`; a folder that cannot be listed has none to offer. */
function walk(config, folder) {
  const entries = unwrapOr(listDir(`${config.repo}/${folder}`), []);
  return foldList(entries, [], (found, entry) => listConcat(found, expand(config, entry)));
}

function editable(config: Config, path) {
  const checked = safePath(config.roots, path);
  switch (checked.kind) {
    case "success":
      return true;
    case "error":
      return false;
  }
}

/** Every editable file under every root, sorted by path. */
export function tree(config: Config): List<Entry> {
  const found = foldList(config.roots, [], (all, root) => listConcat(all, walk(config, root)));
  return listSort(found.filter((path) => editable(config, path))).map((path) => ({ path: path, category: kindFor(path) }) satisfies Entry);
}

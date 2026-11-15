// Which TypeScript- and Python-flavor files the Osprey language server claims.
// `.ts` and `.py` are common extensions, so a file is Osprey only when a
// project manifest lists it or its first line carries the flavor marker; any
// other `.ts` or `.py` file stays the editor's own. The editor's TypeScript
// and Python services keep serving a claimed file too, so its Osprey and
// `tsc` diagnostics appear together. Implements [FLAVOR-TS-SELECT],
// [FLAVOR-TS-VERIFICATION] and [FLAVOR-PYTHON-SELECT].

import * as fs from "fs";
import * as path from "path";

/** A syntax flavor whose files share an extension with another language. */
export type SharedFlavor = "typescript" | "python";

/** A file a project opts into Osprey, by the editor's language id. */
export interface FlavorSource {
  readonly language: SharedFlavor;
  readonly path: string;
}

/** The first-line marker of each flavor. */
const MARKERS: ReadonlyArray<[SharedFlavor, string]> = [
  ["typescript", "// osprey: flavor=typescript"],
  ["python", "# osprey: flavor=python"],
];

/** The manifest key that lists each flavor's sources. */
const KEYS: ReadonlyArray<[SharedFlavor, string]> = [
  ["typescript", "typescript_sources"],
  ["python", "python_sources"],
];

/**
 * The sources `manifestText` lists under `typescript_sources` and
 * `python_sources`, resolved against the manifest's directory.
 */
export function listedSources(
  manifestPath: string,
  manifestText: string,
): FlavorSource[] {
  const directory = path.dirname(manifestPath);
  return KEYS.flatMap(([language, key]) => {
    const list =
      new RegExp(`^\\s*${key}\\s*=\\s*\\[([^\\]]*)\\]`, "m").exec(
        manifestText,
      )?.[1] ?? "";
    return [...list.matchAll(/"([^"]+)"/g)].map((match) => ({
      language,
      path: path.resolve(directory, match[1]),
    }));
  });
}

/** The flavor a file's first non-blank line marks, if any. */
export function markedFlavor(text: string): SharedFlavor | undefined {
  const first = text
    .split("\n")
    .find((line) => line.trim() !== "")
    ?.trim();
  return MARKERS.find(([, marker]) => marker === first)?.[0];
}

/** One document filter per claimed file, by its exact path. */
export function flavorSelectors(
  sources: ReadonlyArray<FlavorSource>,
): { scheme: string; language: SharedFlavor; pattern: string }[] {
  const seen = new Set<string>();
  return sources
    .filter((source) => !seen.has(source.path) && seen.add(source.path))
    .map((source) => ({
      scheme: "file",
      language: source.language,
      pattern: source.path,
    }));
}

/** Folders a workspace walk never enters: dependencies and build output. */
const SKIPPED = new Set([
  "node_modules",
  ".git",
  "target",
  "dist",
  "out",
  ".osprey",
  "coverage",
]);
/** The walk's bounds, so activation stays fast in a large workspace. */
const MAX_DEPTH = 8;
const MAX_FILES = 20000;

/**
 * Every file the workspace `roots` opt into Osprey: the sources each
 * `osprey.toml` lists, and each `.ts` or `.py` file whose first line is its
 * flavor marker.
 */
export function claimedSources(roots: ReadonlyArray<string>): FlavorSource[] {
  const found: FlavorSource[] = [];
  let budget = MAX_FILES;
  const visit = (directory: string, depth: number): void => {
    if (depth > MAX_DEPTH || budget <= 0) {
      return;
    }
    let entries: fs.Dirent[];
    try {
      entries = fs.readdirSync(directory, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      budget -= 1;
      const full = path.join(directory, entry.name);
      if (entry.isDirectory() && !SKIPPED.has(entry.name)) {
        visit(full, depth + 1);
      } else if (entry.isFile()) {
        found.push(...claimedFile(full, entry.name));
      }
    }
  };
  roots.forEach((root) => visit(root, 0));
  return found;
}

function claimedFile(full: string, name: string): FlavorSource[] {
  if (name === "osprey.toml") {
    return listedSources(full, readText(full));
  }
  if (!name.endsWith(".ts") && !name.endsWith(".py")) {
    return [];
  }
  const language = markedFlavor(readText(full).slice(0, 512));
  const extension: SharedFlavor = name.endsWith(".ts")
    ? "typescript"
    : "python";
  return language === extension ? [{ language, path: full }] : [];
}

function readText(file: string): string {
  try {
    return fs.readFileSync(file, "utf8");
  } catch {
    return "";
  }
}

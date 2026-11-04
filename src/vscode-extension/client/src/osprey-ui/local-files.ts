// Implements [LESSONS-FILES]: hosts map application-relative assets only
// after resolving their real location within an explicitly approved root.
import * as fs from "node:fs";
import * as path from "node:path";

export const PRIVATE_DIRECTORY_MODE = 0o700;
const PRIVATE_FILE_MODE = 0o600;

/** Read at most `limit` bytes; a longer file is refused, never truncated. */
export function boundedText(file: string, limit: number): string {
  const handle = fs.openSync(file, "r");
  try {
    const buffer = Buffer.alloc(limit + 1);
    let total = 0;
    while (total <= limit) {
      // A short read is legal, so keep reading until EOF or until the bound is
      // provably exceeded. Decoding a partial read would silently truncate.
      const seen = fs.readSync(handle, buffer, total, limit + 1 - total, total);
      if (seen === 0) break;
      total += seen;
    }
    if (total > limit) throw new Error(`File ${file} exceeds its ${limit}-byte bound`);
    return new TextDecoder("utf-8", { fatal: true }).decode(buffer.subarray(0, total));
  } finally { fs.closeSync(handle); }
}

/** Atomically replace approved text using a private temporary file beside its destination. */
export function persist(target: string, content: string): void {
  fs.mkdirSync(path.dirname(target), { recursive: true, mode: PRIVATE_DIRECTORY_MODE });
  const temporary = `${target}.tmp-${process.pid}-${Date.now()}`;
  const handle = fs.openSync(temporary, "wx", PRIVATE_FILE_MODE);
  try {
    try { fs.fchmodSync(handle, PRIVATE_FILE_MODE); fs.writeFileSync(handle, content); }
    finally { fs.closeSync(handle); }
    fs.renameSync(temporary, target);
  } finally { if (fs.existsSync(temporary)) fs.rmSync(temporary, { force: true }); }
}

/** Whether a relative path climbs out of its root: `..`, or `..` plus a separator. */
const climbs = (relative: string): boolean => relative === ".." || relative.startsWith(`..${path.sep}`);

/** Whether a canonical path is contained by an approved canonical directory. */
export function inside(root: string, candidate: string): boolean {
  const relative = path.relative(root, candidate);
  return relative === "" || (!climbs(relative) && !path.isAbsolute(relative));
}

/** Resolve an application asset to an existing file without accepting URL or traversal syntax. */
export function approvedFile(root: string, source: string): string {
  if (source === "" || /[\\:#?\x00-\x1f\x7f]/.test(source) || path.isAbsolute(source)
    || source.split("/").some((part) => part === "" || part === "." || part === "..")) {
    throw new Error("Invalid application asset path");
  }
  const candidate = fs.realpathSync(path.join(root, source));
  if (!inside(root, candidate)) throw new Error("Asset is outside the approved directory");
  if (!fs.statSync(candidate).isFile()) throw new Error("Asset must be a file");
  return candidate;
}

/** Give a host URI mapper only approved canonical files, including when an asset is a symlink. */
export function createAssetResolver(root: string, uri: (file: string) => string): (source: string) => string {
  const canonicalRoot = fs.realpathSync(root);
  if (!fs.statSync(canonicalRoot).isDirectory()) throw new Error("Asset root must be a directory");
  return (source) => uri(approvedFile(canonicalRoot, source));
}

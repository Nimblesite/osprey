// osprey: namespace=cms
// osprey: module=Upload

/**
 * Media arrives as base64 text in numbered parts, because a request body is
 * at most 1 MiB and a string cannot hold a NUL byte. Parts wait under
 * `.git/cms-upload/`; the last one joins and decodes them all into the target,
 * so a missing or corrupt part leaves the target as it was. [CMS-API-UPLOAD]
 */
import { type Config } from "./settings.ts";
import { commitUpload, type Saved } from "./git.ts";
import { folderOf } from "./paths.ts";
import { receivedJson, savedJson } from "./replies.ts";
import { run } from "./shell.ts";

/** Where parts wait until the last one arrives. */
const PARTS = ".git/cms-upload";
const MAX_PARTS = 4096;
const MAX_PART_BYTES = 786432;

/** What one part produced: more parts are expected, or the file was committed. */
export type Progress = { readonly kind: "received"; readonly part: int } | { readonly kind: "stored"; readonly saved: Saved };

function stem(path) {
  return `${PARTS}/${join(unwrapOr(split(path, "/"), []), "__")}`;
}

/** The part numbers, as `0 ≤ part < parts ≤ 4096`. */
export function partsOf(partText: string, partsText: string): Result<List<int>, string> {
  return andThen(parseInt(partText), (part) =>
    andThen(parseInt(partsText), (parts) => (part >= 0 && part < parts && parts <= MAX_PARTS ? Success([part, parts]) : Error("part must satisfy 0 <= part < parts <= 4096"))));
}

/** Part 0 starts over; any later part needs the one before it. */
function ordered(config: Config, path, part) {
  if (part === 0) {
    return run(config.repo, `rm -f ${stem(path)}.* && mkdir -p ${PARTS}`);
  }
  const before = kindOf(`${config.repo}/${stem(path)}.${satSub(part, 1)}`);
  return unwrapOr(before, "") === "file" ? Success("") : Error(`part ${satSub(part, 1)} of ${path} has not arrived`);
}

/** Join parts 0..parts-1, decode them into `path`, and remove them. */
function joined(config: Config, path, parts) {
  const base = stem(path);
  const gather = `i=0; while [ $i -lt ${parts} ]; do cat ${base}.$i || exit 1; i=$((i+1)); done > ${base}.all`;
  const decode = `base64 -d < ${base}.all > ${base}.bin && mkdir -p '${folderOf(path)}' && mv ${base}.bin '${path}'`;
  const decoded = run(config.repo, `(${gather}) && ${decode}; status=$?; rm -f ${base}.*; exit $status`);
  return andThen(decoded, (_) => commitUpload(config, path));
}

function stored(config: Config, path, part, parts, body) {
  const written = writeFile(`${config.repo}/${stem(path)}.${part}`, body);
  if (satAdd(part, 1) < parts) {
    return andThen(written, (_) => Success({ kind: "received", part: part } satisfies Progress));
  }
  return andThen(written, (_) => andThen(joined(config, path, parts), (saved) => Success({ kind: "stored", saved: saved } satisfies Progress)));
}

/** Store part `part` of `parts` of `path`. */
export function receive(config: Config, path: string, part: int, parts: int, body: string): Result<Progress, string> {
  if (byteLength(body) > MAX_PART_BYTES || byteLength(body) === 0) {
    return Error("a part must hold 1 to 786432 bytes of base64");
  }
  return andThen(ordered(config, path, part), (_) => stored(config, path, part, parts, body));
}

/** The bytes of the media file `path` as one line of base64. */
export function encoded(config: Config, path: string): Result<string, string> {
  return run(config.repo, `base64 < '${path}' | tr -d '\\n'`);
}

/** The reply for one part. The switch stays beside `Progress`: a TypeScript-flavor switch cannot yet name the variants of a union another module declares. */
export function progressJson(path: string, progress: Progress) {
  switch (progress.kind) {
    case "received":
      return receivedJson(progress.part);
    case "stored":
      return savedJson(path, progress.saved);
  }
}

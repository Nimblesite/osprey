// osprey: namespace=cms
// osprey: module=Git

/**
 * Every change the editor makes is one git commit in the site's working tree.
 * Saving never publishes: publishing pushes the saved commits to the site's
 * remote, which the site's pipeline deploys. [CMS-API-SAVE] [CMS-PUBLISH]
 */
import { type Config } from "./settings.ts";
import { folderOf } from "./paths.ts";
import { run, succeeds } from "./shell.ts";

/** The commit a change made. */
export interface Saved {
  readonly commit: string;
}

/** How many commits a publish pushed, and the commit the site now holds. */
export interface Published {
  readonly count: int;
  readonly commit: string;
}

/** One commit of the history. */
export interface Commit {
  readonly sha: string;
  readonly date: string;
  readonly subject: string;
}

const HISTORY = "git log -20 --format=%H%x09%cI%x09%s";

function head(config: Config) {
  return run(config.repo, "git rev-parse HEAD");
}

function saved(sha) {
  return { commit: sha } satisfies Saved;
}

/** Commit what is staged for `path` under `subject`; an unchanged path keeps the current HEAD. */
function committed(config, path, subject) {
  if (succeeds(config.repo, `git diff --cached --quiet -- '${path}'`)) {
    return andThen(head(config), (sha) => Success(saved(sha)));
  }
  const identity = `-c user.name='${config.author}' -c user.email='${config.email}'`;
  return andThen(run(config.repo, `git ${identity} commit -q -m '${subject}' -- '${path}'`), (_) => andThen(head(config), (sha) => Success(saved(sha))));
}

function staged(config, path, subject) {
  return andThen(run(config.repo, `git add -- '${path}'`), (_) => committed(config, path, subject));
}

/** Write the Markdown `text` to `path`, creating its folders, and commit it. */
export function saveText(config: Config, path: string, text: string): Result<Saved, string> {
  const folders = run(config.repo, `mkdir -p '${folderOf(path)}'`);
  return andThen(folders, (_) => andThen(writeFile(`${config.repo}/${path}`, text), (_) => staged(config, path, `Edit ${path}`)));
}

/** Commit the file already written at `path` by an upload. */
export function commitUpload(config: Config, path: string): Result<Saved, string> {
  return staged(config, path, `Upload ${path}`);
}

/** Remove `path` from the tree and commit. */
export function removeFile(config: Config, path: string): Result<Saved, string> {
  return andThen(run(config.repo, `git rm -q -- '${path}'`), (_) => committed(config, path, `Delete ${path}`));
}

function commitOf(line) {
  const fields = unwrapOr(split(line, "\t"), []);
  const field = (index) => unwrapOr(listGet(fields, index), "");
  return { sha: field(0), date: field(1), subject: field(2) } satisfies Commit;
}

/** The newest 20 commits, newest first. */
export function history(config: Config): Result<List<Commit>, string> {
  return andThen(run(config.repo, HISTORY), (text) => Success(lines(text).filter((line) => line !== "").map(commitOf)));
}

/** How many saved commits the remote does not have; an error when the site has no remote to publish to. */
export function unpublished(config: Config): Result<int, string> {
  return andThen(run(config.repo, "git rev-list --count @{upstream}..HEAD"), (count) => parseInt(count));
}

/** Push every saved commit to the site's remote. */
export function publish(config: Config): Result<Published, string> {
  return andThen(unpublished(config), (count) =>
    andThen(run(config.repo, "git push -q"), (_) => andThen(head(config), (sha) => Success({ count: count, commit: sha } satisfies Published))));
}

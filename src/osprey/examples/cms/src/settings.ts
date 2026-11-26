// osprey: namespace=cms
// osprey: module=Settings

/** The server's settings, read from the environment once at start. [CMS-CONFIG] */
import { plainText } from "./paths.ts";
import { quotable } from "./shell.ts";

/** Where the site lives, what the editor may change, and who commits. */
export interface Config {
  readonly repo: string;
  readonly roots: List<string>;
  readonly port: int;
  readonly push: boolean;
  readonly editor: string;
  readonly author: string;
  readonly email: string;
}

const DEFAULT_ROOTS = "content,media";
const DEFAULT_PORT = "4321";
const DEFAULT_EDITOR = "src/osprey/examples/cms/editor";
const DEFAULT_AUTHOR = "Osprey CMS";
const DEFAULT_EMAIL = "cms@localhost";

function setting(name, fallback) {
  return unwrapOr(env(name), fallback);
}

/** The roots, each one plain folder name. */
function rootsOf(text) {
  const roots = unwrapOr(split(text, ","), []).map(trim);
  const plain = roots.filter((root) => root !== "" && !contains(root, "/") && !startsWith(root, ".") && plainText(root));
  return plain.length === roots.length && roots.length > 0 ? Success(roots) : Error(`CMS_ROOTS must be plain folder names: ${text}`);
}

function repoOf() {
  const repo = trimEnd(setting("CMS_REPO", ""));
  if (repo === "") {
    return Error("CMS_REPO must name the site's git working tree");
  }
  return plainText(repo) ? Success(repo) : Error(`CMS_REPO may only use A-Z a-z 0-9 . _ - /: ${repo}`);
}

function identity(author, email) {
  return quotable(author) && quotable(email) ? Success(email) : Error("CMS_AUTHOR_NAME and CMS_AUTHOR_EMAIL may not hold quotes or line breaks");
}

function assembled(repo, roots, port, author, email): Config {
  return { repo: repo, roots: roots, port: port, push: setting("CMS_PUSH", "") === "1", editor: setting("CMS_EDITOR", DEFAULT_EDITOR), author: author, email: email };
}

/** The configuration, or the first setting that is missing or unsafe. */
export function loadConfig(): Result<Config, string> {
  const author = setting("CMS_AUTHOR_NAME", DEFAULT_AUTHOR);
  return andThen(repoOf(), (repo) =>
    andThen(rootsOf(setting("CMS_ROOTS", DEFAULT_ROOTS)), (roots) =>
      andThen(parseInt(setting("CMS_PORT", DEFAULT_PORT)), (port) =>
        andThen(identity(author, setting("CMS_AUTHOR_EMAIL", DEFAULT_EMAIL)), (email) => Success(assembled(repo, roots, port, author, email))))));
}

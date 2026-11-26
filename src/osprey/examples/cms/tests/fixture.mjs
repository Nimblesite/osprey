// The Git CMS test fixture [CMS-E2E]: the throwaway Eleventy-shaped site with its bare "published"
// remote, the server compiled under a memory model, the Osprey Native editor's web build, and the
// server's lifecycle. Shared by the API session (e2e.mjs) and the browser session (editor.spec.mjs).
import { execFileSync, spawn } from "node:child_process";
import { mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
export const root = path.resolve(here, "../../../../..");
export const osprey = process.env.OSPREY ?? path.join(root, "target/release/osprey");
export const project = path.join(root, "src/osprey/examples/cms");
export const out = path.join(root, "target/cms");
/** The Osprey Native editor's web build, made by the browser host's build script. [CMS-EDITOR] */
export const EDITOR = path.join(out, "editor-web");
export const ROOTS = ["content", "media"];
export const INIT = "Init site";
/** The site the browser session edits, and the port its server listens on. */
export const BROWSER_SITE = path.join(out, "site-browser");
export const BROWSER_PORT = 4397;
/** The real Eleventy sample site under the CMS, for the author's journey from editor to built site. [CMS-SITE] */
export const ELEVENTY_SITE = path.join(root, "src/osprey/examples/cms/site");
export const AUTHOR_SITE = path.join(out, "site-author");
export const AUTHOR_PORT = 4398;
export const ELEVENTY = path.join(root, "src/toolchain/website/node_modules/.bin/eleventy");
export const BUILD_MS = 300_000;
const LISTEN_MS = 10_000;

/** The site the developer hands over: `src/` and dot-files are what the editor must never be shown. */
export const SITE = {
  "content/index.md": "---\ntitle: Home\n---\n\n# Welcome 👋\n\nWe build *sites* with “curly quotes”, café crème and 🚀.\n\n- [First post](/blog/first/)\n",
  "content/blog/first.md": "# First post\n\n```osp\nfn main() = print(\"hi\")\n```\n\nTyped in an editor that leaves no final newline",
  "content/about.markdown": "# About\n",
  "content/.draft.md": "# A hidden draft\n",
  "media/logo.svg": '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"><rect width="1" height="1" fill="#f80"/></svg>\n',
  "media/photo.JPG": Buffer.from([0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00]),
  "media/notes.txt": "shot list\n",
  "media/tone.mp3": Buffer.from([0x49, 0x44, 0x33, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xfb]),
  "media/.Thumbs.db": Buffer.from([0xd0, 0xcf, 0x11, 0xe0]),
  "src/layout.njk": "<h1>{{ title }}</h1>\n",
};

/** Run git in the site and return its trimmed stdout. */
export function git(site, ...args) {
  return execFileSync("git", args, { cwd: site, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

export const head = (site) => git(site, "rev-parse", "HEAD");

/** Build the Osprey Native editor for the browser into EDITOR. */
export function buildEditor() {
  mkdirSync(out, { recursive: true });
  execFileSync("node", [path.join(root, "src/toolchain/hosts/web/scripts/build.mjs"), "--app", path.join(project, "editor"), "--title", "Osprey Studio", "--out", EDITOR],
    { env: { ...process.env, OSPREY_BIN: osprey }, stdio: ["ignore", "inherit", "inherit"], timeout: BUILD_MS });
}

/** Every tracked file of the site at `folder`, as `SITE` spells them; build output stays out. */
export function siteFiles(folder) {
  const names = git(folder, "ls-files", "--", ".").split("\n").filter(Boolean);
  return Object.fromEntries(names.map((name) => [name, readFileSync(path.join(folder, name))]));
}

/** Create the throwaway site repository at `site`: the developer's `files`, one commit, a local identity. */
export function createSite(site, files = SITE) {
  rmSync(site, { recursive: true, force: true });
  for (const [file, bytes] of Object.entries(files)) {
    mkdirSync(path.dirname(path.join(site, file)), { recursive: true });
    writeFileSync(path.join(site, file), bytes);
  }
  git(site, "init", "-q");
  for (const [name, value] of [["user.name", "CMS e2e"], ["user.email", "e2e@cms.invalid"], ["commit.gpgsign", "false"]]) {
    git(site, "config", name, value);
  }
  git(site, "add", "-A");
  git(site, "commit", "-q", "-m", INIT);
  // The published site: a bare repository the working tree pushes to, as a bucket-backed remote would be. [CMS-PUBLISH]
  const remote = `${site}.git`;
  rmSync(remote, { recursive: true, force: true });
  execFileSync("git", ["init", "-q", "--bare", remote]);
  git(site, "remote", "add", "origin", remote);
  git(site, "push", "-q", "-u", "origin", "HEAD");
}

/** The tree's row for `key`. */
export const row = (page, key) => page.locator(`.osp-row[data-row-key="${key}"]`);

/** The status pill whose words are exactly `words` (case-sensitive, so "Saved" never matches "Unsaved"). */
export const status = (page, words) => page.locator(".osp-frame-status").filter({ hasText: new RegExp(`(^|[^A-Za-z])${words}$`) });

/** Select `word` (the `nth` occurrence) in `area` as a person dragging over it would. */
export async function selectWord(area, word, nth = 0) {
  await area.focus();
  await area.evaluate((field, [target, which]) => {
    let at = -1;
    for (let seen = 0; seen <= which; seen += 1) at = field.value.indexOf(target, at + 1);
    if (at < 0) throw new Error(`"${target}" is not in the field`);
    field.setSelectionRange(at, at + target.length);
  }, [word, nth]);
}

/** Build the Eleventy site in `folder` into `built`, as the site's own pipeline would. [CMS-SITE] */
export function buildSite(folder, built) {
  rmSync(built, { recursive: true, force: true });
  execFileSync(ELEVENTY, ["--output", built], { cwd: folder, stdio: "pipe", timeout: 120_000 });
}

/** Serve the built site in `built` on `port` and resolve once it answers; kill the process to stop it. */
export async function serveSite(built, port) {
  const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1"], { cwd: built, stdio: "ignore" });
  const deadline = Date.now() + 10_000;
  while (await fetch(`http://127.0.0.1:${port}/`).then((reply) => reply.status, () => 0) !== 200) {
    if (Date.now() > deadline) throw new Error(`the built site never answered on ${port}`);
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return server;
}

/** The commit the published site (the bare remote) holds. */
export const published = (site) => execFileSync("git", ["--git-dir", `${site}.git`, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();

/** Compile the server under `memory` and return its binary. */
export function build(memory) {
  const binary = path.join(out, `cms-${memory}`);
  const args = [project, "--compile", "--quiet", "--no-warnings", `--memory=${memory}`, "-o", binary];
  try {
    execFileSync(osprey, args, { stdio: ["ignore", "pipe", "pipe"], timeout: BUILD_MS });
  } catch (error) {
    throw new Error(`[${memory}] the server did not compile:\n${error.stdout}${error.stderr}`, { cause: error });
  }
  return binary;
}

/** Fail unless the port is free: a server left behind would answer instead of the one under test. */
export async function unused(port) {
  for (let attempt = 0; attempt < 60; attempt += 1) {
    const open = await new Promise((resolve) => {
      const probe = http.request({ host: "127.0.0.1", port, method: "GET", path: "/", agent: false }, (reply) => {
        reply.resume();
        resolve(true);
      });
      probe.on("error", () => resolve(false));
      probe.end();
    });
    if (!open) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`something already listens on port ${port}; stop it before the test`);
}

/** Resolve once the server answers on the port; reject as soon as it exits, or after LISTEN_MS. */
function listening(server, port) {
  const deadline = Date.now() + LISTEN_MS;
  return new Promise((resolve, reject) => {
    const exited = () => reject(new Error(`the server exited (${server.exitCode ?? server.signalCode}) before it listened`));
    const giveUp = () => {
      server.off("exit", exited);
      reject(new Error(`nothing listened on port ${port} within ${LISTEN_MS} ms`));
    };
    server.once("exit", exited);
    const probe = () => {
      if (server.exitCode !== null || server.signalCode !== null) return;
      const request = http.request({ host: "127.0.0.1", port, method: "GET", path: "/api/tree", agent: false }, (reply) => {
        reply.resume();
        server.off("exit", exited);
        resolve();
      });
      request.on("error", () => (Date.now() < deadline ? setTimeout(probe, 50) : giveUp()));
      request.end();
    };
    probe();
  });
}

/** Spawn the server on the throwaway site and resolve once it listens; its log is kept beside the binary. */
export async function start(ctx, binary) {
  const logPath = path.join(out, `cms-${ctx.memory}.log`);
  const log = openSync(logPath, "w");
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("CMS_")));
  const server = spawn(binary, [], { cwd: root, stdio: ["ignore", log, log],
    env: { ...env, CMS_REPO: ctx.site, CMS_PORT: String(ctx.port), CMS_ROOTS: ROOTS.join(","), CMS_EDITOR: EDITOR } });
  try {
    await listening(server, ctx.port);
  } catch (error) {
    server.kill();
    throw new Error(`[${ctx.memory}] ${error.message}\n--- ${path.relative(root, logPath)} ---\n${readFileSync(logPath, "utf8").slice(-2000)}`, { cause: error });
  }
  return server;
}

/** Kill the server and wait for it to go, so the port is free for the next memory. */
export function stop(server) {
  return new Promise((resolve) => {
    if (server.exitCode !== null || server.signalCode !== null) return resolve();
    server.once("exit", resolve);
    server.kill();
  });
}


// End-to-end acceptance for the Git CMS [CMS-E2E]: the server is compiled by
// Osprey under GC and ARC, pointed at a throwaway site repository and driven
// over real loopback HTTP as an editor works — open, edit, save, create,
// delete, upload in parts, preview, history — and sent every unsafe path of
// [CMS-PATH-SAFE]. Each step asserts the reply, the file bytes and the git log.
// Run from the repository root:
//   node src/osprey/examples/cms/tests/e2e.mjs
// [CMS-API], [CMS-API-TREE], [CMS-API-SAVE], [CMS-API-UPLOAD], [CMS-KINDS]
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, openSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, "../../../../..");
const osprey = process.env.OSPREY ?? path.join(root, "target/release/osprey");
const project = path.join(root, "src/osprey/examples/cms");
const out = path.join(root, "target/cms");
/** The Osprey Native editor's web build, made once per run by the browser host's build script. [CMS-EDITOR] */
const EDITOR = path.join(out, "editor-web");
const PORT = 4399;
const ROOTS = ["content", "media"];
const MEMORIES = ["gc", "arc"];
const INIT = "Init site";
const INDEX = "content/index.md";
const PART = 786432;
const BURST = 22;
const SHA = /^[0-9a-f]{40}$/;
const BASE64 = /^[A-Za-z0-9+/]+={0,2}$/;
const BUILD_MS = 300_000;
const REQUEST_MS = 30_000;
const LISTEN_MS = 10_000;

// The site, the kinds and the unsafe paths -------------------------------------------------

/** The site the developer hands over: `src/` and dot-files are what the editor must never be shown. */
const SITE = {
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

/** [CMS-KINDS] */
const KINDS = {
  md: "markdown", markdown: "markdown",
  png: "image", jpg: "image", jpeg: "image", gif: "image", webp: "image", svg: "image", avif: "image",
  mp4: "video", webm: "video", mov: "video",
  mp3: "audio", wav: "audio", ogg: "audio", m4a: "audio", flac: "audio",
};
const MEDIA = ["image", "video", "audio"];
const kind = (file) => KINDS[path.posix.extname(file).slice(1).toLowerCase()] ?? "other";

/** What the tree must list for `files`: those under the roots, hidden ones left out, sorted by path. [CMS-API-TREE] */
function listed(files) {
  const shown = (file) => ROOTS.includes(file.split("/")[0]) && !file.split("/").some((segment) => segment.startsWith("."));
  return files.filter(shown).sort().map((file) => ({ path: file, kind: kind(file) }));
}

/** Every rule of [CMS-PATH-SAFE] broken one way each. */
const UNSAFE = [
  "../etc/passwd", "content/../x.md", ".git/config", "content/.hidden.md", "/abs.md", "src/layout.njk", "content",
  "content//a.md", "content/a b.md", "content/a;rm.md", "content/$(x).md", "content/a'.md", "content/a\\.md", "",
  `content/${"a".repeat(300)}.md`,
  `content/${"a".repeat(190)}.md`, "content/..", "content/./x.md", "content/sub/.env", "content/x.md/", "media",
  "Content/index.md", 'content/a"b.md', "content/a`b.md", "content/a$b.md", "content/a&b.md", "content/a%b.md",
  "content/a\tb.md", "content/é.md",
];

/** The longest path the rule lets through: 200 bytes exactly, so a missing file (404), never an unsafe one (400). */
const LONGEST = `content/${"a".repeat(189)}.md`;

/** The routes that take a path, each with the request an editor would send beside it. */
const ROUTES = [
  { method: "GET", route: "/api/file" },
  { method: "PUT", route: "/api/file", body: "# overwrite\n" },
  { method: "DELETE", route: "/api/file" },
  { method: "POST", route: "/api/upload", params: { part: 0, parts: 1 }, body: Buffer.from("x").toString("base64") },
];

// Counted assertions, git and HTTP ------------------------------------------------------------

let assertions = 0;

/** `assert.equal`, counted so the run ends with how many checks it made. */
function equal(actual, expected, label) {
  assertions += 1;
  assert.equal(actual, expected, label);
}

function deepEqual(actual, expected, label) {
  assertions += 1;
  assert.deepEqual(actual, expected, label);
}

function ok(value, label) {
  assertions += 1;
  assert.ok(value, label);
}

/** Run git in the site and return its trimmed stdout. */
function git(site, ...args) {
  return execFileSync("git", args, { cwd: site, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

const head = (site) => git(site, "rev-parse", "HEAD");
const lastSubject = (site) => git(site, "log", "-1", "--format=%s");
const log = (site) => git(site, "log", "--format=%H").split("\n");
const disk = (site, file) => readFileSync(path.join(site, file));
const exists = (site, file) => existsSync(path.join(site, file));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");

/** One request over loopback HTTP, its body sent as a browser sends a string; resolves with status, Content-Type and bytes. */
function call(port, method, target, body = "") {
  const payload = Buffer.from(body);
  const request = { host: "127.0.0.1", port, method, path: target, agent: false,
    headers: { "Content-Type": "text/plain; charset=utf-8", "Content-Length": payload.length } };
  return new Promise((resolve, reject) => {
    const sent = http.request(request, (reply) => {
      const chunks = [];
      reply.on("data", (chunk) => chunks.push(chunk));
      reply.on("end", () => resolve({ status: reply.statusCode, type: reply.headers["content-type"], body: Buffer.concat(chunks) }));
    });
    sent.setTimeout(REQUEST_MS, () => sent.destroy(new Error(`${method} ${target}: no reply within ${REQUEST_MS} ms`)));
    sent.on("error", reject);
    sent.end(payload);
  });
}

/** The request target: every query value encoded as the editor's encodeURIComponent encodes it, `/` included. */
function target(route, params) {
  const query = Object.entries(params).map(([name, value]) => `${name}=${encodeURIComponent(value)}`).join("&");
  return `${route}?${query}`;
}

/** A Content-Type without the formatting HTTP lets vary: lower case, no blanks around `;`. */
const mediaType = (type) => (type ?? "").toLowerCase().split(";").map((part) => part.trim()).join(";");

/** Assert a reply's status and Content-Type; return its bytes. `type` is the full header, or its essence alone. */
function typed(reply, status, type, label) {
  equal(reply.status, status, `${label}: status`);
  const got = mediaType(reply.type);
  equal(type.includes(";") ? got : got.split(";")[0], type, `${label}: Content-Type`);
  return reply.body;
}

/** Assert a JSON reply: `status` and the Content-Type every JSON reply carries; return what it decoded to. */
function json(reply, status, label) {
  typed(reply, status, "application/json;charset=utf-8", label);
  let parsed = null;
  try {
    parsed = JSON.parse(reply.body.toString("utf8"));
  } catch {
    parsed = null;
  }
  ok(parsed !== null && typeof parsed === "object", `${label}: body is a JSON object`);
  return parsed;
}

/** Assert a failure: `status` (or any 4xx) and the `{"error": "<reason>"}` no failure may omit. */
function failure(reply, status, label) {
  if (status === "4xx") ok(reply.status >= 400 && reply.status < 500, `${label}: status is 4xx, got ${reply.status}`);
  const parsed = json(reply, status === "4xx" ? reply.status : status, label);
  ok(typeof parsed.error === "string" && parsed.error.length > 0, `${label}: {"error": "<reason>"}`);
  return parsed;
}

// The site and the server ----------------------------------------------------------------------

/** Create the throwaway site repository at `site`: the developer's files, one commit, a local identity. */
function createSite(site) {
  rmSync(site, { recursive: true, force: true });
  for (const [file, bytes] of Object.entries(SITE)) {
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

/** The commit the published site (the bare remote) holds. */
const published = (site) => execFileSync("git", ["--git-dir", `${site}.git`, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();

/** Compile the server under `memory` and return its binary. */
function build(memory) {
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
async function unused(port) {
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
function listening(server) {
  const deadline = Date.now() + LISTEN_MS;
  return new Promise((resolve, reject) => {
    const exited = () => reject(new Error(`the server exited (${server.exitCode ?? server.signalCode}) before it listened`));
    const giveUp = () => {
      server.off("exit", exited);
      reject(new Error(`nothing listened on port ${PORT} within ${LISTEN_MS} ms`));
    };
    server.once("exit", exited);
    const probe = () => {
      if (server.exitCode !== null || server.signalCode !== null) return;
      const request = http.request({ host: "127.0.0.1", port: PORT, method: "GET", path: "/api/tree", agent: false }, (reply) => {
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
async function start(ctx, binary) {
  const logPath = path.join(out, `cms-${ctx.memory}.log`);
  const log = openSync(logPath, "w");
  const env = Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith("CMS_")));
  const server = spawn(binary, [], { cwd: root, stdio: ["ignore", log, log],
    env: { ...env, CMS_REPO: ctx.site, CMS_PORT: String(PORT), CMS_ROOTS: ROOTS.join(","), CMS_EDITOR: EDITOR } });
  try {
    await listening(server);
  } catch (error) {
    server.kill();
    throw new Error(`[${ctx.memory}] ${error.message}\n--- ${path.relative(root, logPath)} ---\n${readFileSync(logPath, "utf8").slice(-2000)}`, { cause: error });
  }
  return server;
}

/** Kill the server and wait for it to go, so the port is free for the next memory. */
function stop(server) {
  return new Promise((resolve) => {
    if (server.exitCode !== null || server.signalCode !== null) return resolve();
    server.once("exit", resolve);
    server.kill();
  });
}

// The editor's requests ------------------------------------------------------------------------

/** The server survived the step: no request may take it down. */
function alive(ctx, label) {
  ok(ctx.server.exitCode === null && ctx.server.signalCode === null, `${label}: the server is still running`);
}

/** The files the tree lists that are of one of `kinds`, sorted by path. */
const shown = (ctx, ...kinds) => listed([...ctx.files]).filter((file) => kinds.includes(file.kind)).map((file) => file.path);
const say = (ctx, text) => `${text} [${ctx.memory}]`;

/** GET /api/file: a 200 text reply; its bytes. */
async function fetched(ctx, file) {
  return typed(await call(ctx.port, "GET", target("/api/file", { path: file })), 200, "text/plain;charset=utf-8", say(ctx, `GET /api/file ${file}`));
}

/** GET /api/media: a 200 text reply; its bytes. */
async function previewed(ctx, file) {
  return typed(await call(ctx.port, "GET", target("/api/media", { path: file })), 200, "text/plain", say(ctx, `GET /api/media ${file}`));
}

/** PUT /api/file, the editor's save; the JSON reply. */
async function save(ctx, file, text) {
  return json(await call(ctx.port, "PUT", target("/api/file", { path: file }), text), 200, say(ctx, `PUT /api/file ${file}`));
}

/** DELETE /api/file; the JSON reply. */
async function remove(ctx, file) {
  return json(await call(ctx.port, "DELETE", target("/api/file", { path: file })), 200, say(ctx, `DELETE /api/file ${file}`));
}

/** POST one part of an upload; the reply. */
function send(ctx, file, params, body) {
  return call(ctx.port, "POST", target("/api/upload", { path: file, ...params }), body);
}

/** A write the server committed: it replied the commit HEAD now is, the only new commit, with the subject and a clean tree. */
function committed(ctx, reply, file, verb) {
  const done = `${verb} ${file}`;
  const label = say(ctx, done);
  equal(reply.path, file, `${label}: path`);
  ok(SHA.test(reply.commit ?? ""), `${label}: commit is a 40-hex sha`);
  equal(head(ctx.site), reply.commit, `${label}: HEAD is the replied commit`);
  equal(log(ctx.site).length, ctx.made.length + 2, `${label}: one new commit`);
  equal(lastSubject(ctx.site), done, `${label}: git log -1 --format=%s`);
  equal(git(ctx.site, "status", "--porcelain"), "", `${label}: working tree clean`);
  equal(published(ctx.site), ctx.live, `${label}: saving never publishes`);
  alive(ctx, label);
  ctx.made.push({ sha: reply.commit, subject: done });
}

/** GET /api/tree is exactly the files the session knows of, sorted, with their kinds. */
async function tree(ctx) {
  const reply = json(await call(ctx.port, "GET", "/api/tree"), 200, say(ctx, "GET /api/tree"));
  deepEqual(reply.roots, ROOTS, say(ctx, "tree: roots"));
  deepEqual(reply.files, listed([...ctx.files]), say(ctx, "tree: files, sorted, with kinds"));
}

// The session ----------------------------------------------------------------------------------

/** The editor opens the site: the page, the tree, and every file as it is on disk. */
async function browse(ctx) {
  const page = await call(ctx.port, "GET", "/");
  const label = say(ctx, "GET /");
  ok(typed(page, 200, "text/html", label).equals(readFileSync(path.join(EDITOR, "index.html"))), `${label}: the editor page as built`);
  ok(page.body.includes('id="osprey-root"'), `${label}: the page holds the Osprey Native root`);
  for (const [file, type] of [["host.js", "text/javascript"], ["host.css", "text/css"]]) {
    ok(typed(await call(ctx.port, "GET", `/${file}`), 200, type, `${label} ${file}`).equals(readFileSync(path.join(EDITOR, file))), `${label}: ${file} as built`);
  }
  await tree(ctx);
  for (const file of shown(ctx, "markdown")) {
    ok((await fetched(ctx, file)).equals(disk(ctx.site, file)), say(ctx, `GET /api/file ${file}: the bytes on disk`));
  }
  await notText(ctx);
  await preview(ctx);
  await missing(ctx);
}

/** Only Markdown is edited as text: every other file under the roots is a 400. */
async function notText(ctx) {
  for (const file of shown(ctx, ...MEDIA, "other")) {
    failure(await call(ctx.port, "GET", target("/api/file", { path: file })), 400, say(ctx, `GET /api/file ${file}`));
  }
}

/** Every image, video and audio file previews as the base64 of the bytes on disk. */
async function preview(ctx) {
  for (const file of shown(ctx, ...MEDIA)) {
    const label = say(ctx, `GET /api/media ${file}`);
    const encoded = (await previewed(ctx, file)).toString("utf8").replace(/\s/g, "");
    ok(BASE64.test(encoded), `${label}: base64 text`);
    ok(Buffer.from(encoded, "base64").equals(disk(ctx.site, file)), `${label}: decodes to the bytes on disk`);
  }
}

/** A missing file is a 404 with a reason, never an empty page or an empty preview. */
async function missing(ctx) {
  failure(await call(ctx.port, "GET", target("/api/file", { path: "content/missing.md" })), 404, say(ctx, "GET /api/file content/missing.md"));
  failure(await call(ctx.port, "GET", target("/api/media", { path: "media/missing.png" })), 404, say(ctx, "GET /api/media media/missing.png"));
}

/** Edit the home page and save it; save the same text again, which changes nothing. */
async function editHome(ctx) {
  const text = `${disk(ctx.site, INDEX).toString("utf8")}\nEdited in the [${ctx.memory}] session.\n`;
  const first = await save(ctx, INDEX, text);
  committed(ctx, first, INDEX, "Edit");
  ok(disk(ctx.site, INDEX).equals(Buffer.from(text)), say(ctx, `Edit ${INDEX}: the bytes on disk are what was sent`));
  ok((await fetched(ctx, INDEX)).equals(Buffer.from(text)), say(ctx, `GET /api/file ${INDEX}: what was saved`));
  const again = await save(ctx, INDEX, text);
  equal(again.commit, first.commit, say(ctx, "second save of the same text: replies HEAD"));
  equal(log(ctx.site).length, ctx.made.length + 1, say(ctx, "second save of the same text: no new commit"));
}

/** Create a page two folders deep, then delete it; a second delete and a read find nothing. */
async function nestedPage(ctx) {
  const file = "content/blog/2026/new.md";
  const text = "# New page\n\nSaved from the editor.\n";
  ok(!exists(ctx.site, "content/blog/2026"), say(ctx, `new ${file}: its folder does not exist yet`));
  committed(ctx, await save(ctx, file, text), file, "Edit");
  ctx.files.add(file);
  ok(disk(ctx.site, file).equals(Buffer.from(text)), say(ctx, `Edit ${file}: the bytes on disk, folders created`));
  await tree(ctx);
  committed(ctx, await remove(ctx, file), file, "Delete");
  ctx.files.delete(file);
  ok(!exists(ctx.site, file), say(ctx, `Delete ${file}: gone from disk`));
  await tree(ctx);
  const before = head(ctx.site);
  failure(await call(ctx.port, "DELETE", target("/api/file", { path: file })), 404, say(ctx, `DELETE ${file} again`));
  failure(await call(ctx.port, "GET", target("/api/file", { path: file })), 404, say(ctx, `GET /api/file ${file} after the delete`));
  equal(head(ctx.site), before, say(ctx, `delete of the missing ${file}: no new commit`));
}

/** A page under a folder that does not exist yet, named with every character the rule allows. */
async function namedPage(ctx) {
  const file = "content/drafts/my-page_v2.0.md";
  committed(ctx, await save(ctx, file, "# Draft\n"), file, "Edit");
  ctx.files.add(file);
  await tree(ctx);
  ok((await fetched(ctx, file)).toString("utf8") === "# Draft\n", say(ctx, `GET /api/file ${file}: what was saved`));
}

/** An editor may clear a page: an empty body is a save like any other, and the empty file opens. */
async function emptiedPage(ctx) {
  const file = "content/empty.md";
  committed(ctx, await save(ctx, file, ""), file, "Edit");
  ctx.files.add(file);
  equal(disk(ctx.site, file).length, 0, say(ctx, `Edit ${file}: an empty file on disk`));
  equal((await fetched(ctx, file)).length, 0, say(ctx, `GET /api/file ${file}: empty text`));
  await tree(ctx);
}

/** Media is deleted like a page, and committed as such. */
async function deletedMedia(ctx) {
  const file = "media/notes.txt";
  committed(ctx, await remove(ctx, file), file, "Delete");
  ctx.files.delete(file);
  ok(!exists(ctx.site, file), say(ctx, `Delete ${file}: gone from disk`));
  await tree(ctx);
}

/** A deterministic clip: `size` bytes holding every byte value, NUL included. */
function clip(size) {
  const bytes = Buffer.allocUnsafe(size);
  let state = 0x9e3779b9;
  for (let index = 0; index < size; index += 1) {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    bytes[index] = (state >>> 24) ^ (index & 0xff);
  }
  return bytes;
}

/** The base64 of `bytes` in parts of PART characters, as the editor splits it. */
function parts(bytes) {
  const encoded = bytes.toString("base64");
  return Array.from({ length: Math.ceil(encoded.length / PART) }, (_, index) => encoded.slice(index * PART, (index + 1) * PART));
}

/** Send every part of `bytes` to `file` in order: the early parts are only acknowledged and commit nothing; the last reply is returned. */
async function deliver(ctx, file, bytes) {
  const split = parts(bytes);
  const before = head(ctx.site);
  for (const [index, body] of split.slice(0, -1).entries()) {
    const label = say(ctx, `POST /api/upload ${file} part ${index} of ${split.length}`);
    equal(json(await send(ctx, file, { part: index, parts: split.length }, body), 200, label).received, index, `${label}: {"received": ${index}}`);
    ok(!exists(ctx.site, file), `${label}: the target is left as it was`);
    equal(head(ctx.site), before, `${label}: no commit`);
  }
  const last = split.length - 1;
  return json(await send(ctx, file, { part: last, parts: split.length }, split.at(-1)), 200, say(ctx, `POST /api/upload ${file} part ${last} of ${split.length}`));
}

/** Upload a video in four parts, then preview it; it is media, so it is not text. */
async function uploaded(ctx) {
  const file = "media/clip.mp4";
  const bytes = clip(2_000_000);
  equal(new Set(bytes).size, 256, "the clip holds every byte value, NUL included");
  equal(parts(bytes).length, 4, "2,000,000 bytes are four parts of at most 786432 characters");
  const reply = await deliver(ctx, file, bytes);
  committed(ctx, reply, file, "Upload");
  ctx.files.add(file);
  equal(sha256(disk(ctx.site, file)), sha256(bytes), say(ctx, `Upload ${file}: the sha256 of the bytes on disk is the clip's`));
  await preview(ctx);
  await tree(ctx);
  await notText(ctx);
}

/** An image may land under any root: the kind comes from the extension, not the folder. */
async function uploadedImage(ctx) {
  const file = "content/picture.png";
  const bytes = clip(70_000);
  committed(ctx, await deliver(ctx, file, bytes), file, "Upload");
  ctx.files.add(file);
  ok(disk(ctx.site, file).equals(bytes), say(ctx, `Upload ${file}: the bytes on disk are the clip's`));
  await tree(ctx);
}

/** The uploads the server must refuse: out of order, out of range, undecodable, or not a media kind. */
function refused(split) {
  const small = split[0];
  return [
    ["media/oops.mp4", { part: 1, parts: 2 }, split[1]],
    ["media/oops.mp4", { part: 2, parts: 2 }, small],
    ["media/oops.mp4", { part: -1, parts: 2 }, small],
    ["media/oops.mp4", { part: 0, parts: 0 }, small],
    ["media/oops.mp4", { part: 0, parts: 4097 }, small],
    ["media/oops.mp4", { part: 0 }, small],
    ["media/oops.mp4", { parts: 1 }, small],
    ["media/oops.mp4", { part: "x", parts: 1 }, small],
    ["media/oops.mp4", { part: 0, parts: 1 }, "!!!!not base64!!!!"],
    ["media/big.bin", { part: 0, parts: 1 }, small],
    ["content/note.md", { part: 0, parts: 1 }, small],
  ];
}

/** Each refused upload is a 4xx with a reason that leaves its target and the log as they were. */
async function rejectedUploads(ctx) {
  const attempts = refused(parts(clip(590_000)));
  const before = head(ctx.site);
  for (const [file, params, body] of attempts) {
    const label = say(ctx, `POST /api/upload ${file} ${JSON.stringify(params)}`);
    failure(await send(ctx, file, params, body), "4xx", label);
    ok(!exists(ctx.site, file), `${label}: the target was not created`);
    equal(head(ctx.site), before, `${label}: no new commit`);
  }
  alive(ctx, say(ctx, `after ${attempts.length} refused uploads`));
}

/** The editor retries the refused upload in order and it works: a failed upload must not jam its path. */
async function retriedUpload(ctx) {
  const file = "media/oops.mp4";
  const bytes = clip(590_000);
  committed(ctx, await deliver(ctx, file, bytes), file, "Upload");
  ctx.files.add(file);
  ok(disk(ctx.site, file).equals(bytes), say(ctx, `Upload ${file}: the bytes on disk are the clip's`));
  await tree(ctx);
}

/** Every entry of the site outside `.git`: its path and, for a file, the hash of its bytes. */
function snapshot(site) {
  return readdirSync(site, { recursive: true, withFileTypes: true })
    .filter((entry) => path.relative(site, entry.parentPath).split(path.sep)[0] !== ".git")
    .map((entry) => {
      const full = path.join(entry.parentPath, entry.name);
      return `${path.relative(site, full)}${entry.isFile() ? ` ${sha256(readFileSync(full))}` : ""}`;
    })
    .sort();
}

/** One unsafe path against every route that takes one: 400 with a reason, and no commit. */
async function refusedPath(ctx, unsafe, before) {
  for (const { method, route, params, body } of ROUTES) {
    const label = say(ctx, `${method} ${route} ${JSON.stringify(unsafe)}`);
    failure(await call(ctx.port, method, target(route, { path: unsafe, ...params }), body), 400, label);
    equal(head(ctx.site), before, `${label}: no new commit`);
  }
}

/** Every unsafe path: nothing is answered but 400, nothing on disk is written, removed or changed. */
async function unsafePaths(ctx) {
  const before = head(ctx.site);
  const outside = path.resolve(ctx.site, "..", "etc", "passwd");
  ok(!existsSync(outside), say(ctx, `control: ${path.relative(root, outside)} does not exist beforehand`));
  const [untouched, config] = [snapshot(ctx.site), readFileSync(path.join(ctx.site, ".git/config"))];
  for (const unsafe of UNSAFE) await refusedPath(ctx, unsafe, before);
  deepEqual(snapshot(ctx.site), untouched, say(ctx, "unsafe paths: nothing on disk was written, removed or changed"));
  ok(readFileSync(path.join(ctx.site, ".git/config")).equals(config), say(ctx, "unsafe paths: .git/config is untouched"));
  ok(!existsSync(outside), say(ctx, "unsafe paths: nothing was written beside the site"));
  equal(git(ctx.site, "status", "--porcelain"), "", say(ctx, "unsafe paths: working tree clean"));
  alive(ctx, say(ctx, "after the unsafe paths"));
}

/** The 200-byte path is the longest the rule lets through: a missing file (404), never an unsafe one (400). */
async function longestPath(ctx) {
  equal(Buffer.byteLength(LONGEST), 200, "control: the longest accepted path is 200 bytes");
  const before = head(ctx.site);
  failure(await call(ctx.port, "GET", target("/api/file", { path: LONGEST })), 404, say(ctx, "GET /api/file <200 bytes>"));
  equal(head(ctx.site), before, say(ctx, "GET /api/file <200 bytes>: no new commit"));
}

/** GET /api/history against what the session committed: newest first, each subject, at most 20, and `git log` agrees. */
async function history(ctx, moment) {
  const label = say(ctx, `GET /api/history ${moment}`);
  const reply = json(await call(ctx.port, "GET", "/api/history"), 200, label);
  const made = [{ sha: ctx.init, subject: INIT }, ...ctx.made].reverse();
  ok(Array.isArray(reply.commits), `${label}: commits is a list`);
  equal(reply.commits.length, Math.min(made.length, 20), `${label}: every commit, twenty at most`);
  deepEqual(reply.commits.map(({ sha, subject }) => ({ sha, subject })), made.slice(0, 20), `${label}: newest first, with the session's subjects`);
  reply.commits.forEach((commit, index) => ok(typeof commit.date === "string" && commit.date.length > 0, `${label}: commit ${index} has a date`));
  deepEqual(log(ctx.site).slice(0, reply.commits.length), reply.commits.map((commit) => commit.sha), `${label}: git log agrees`);
}

/** The history before and after more saves than it lists. */
async function historySteps(ctx) {
  await history(ctx, "after the edits");
  const base = disk(ctx.site, INDEX).toString("utf8");
  for (let revision = 1; revision <= BURST; revision += 1) {
    const text = revision === 3 ? `${base}\n${"A long paragraph. ".repeat(10_000)}\n` : `${base}\n\nRevision ${revision}.\n`;
    committed(ctx, await save(ctx, INDEX, text), INDEX, "Edit");
  }
  await history(ctx, `after ${BURST} more saves`);
}

/** GET /api/status: how many saved commits the published site does not have yet. [CMS-PUBLISH] */
async function unpublished(ctx, label) {
  const status = json(await call(ctx.port, "GET", "/api/status"), 200, `${label}: GET /api/status`);
  const expected = Number(git(ctx.site, "rev-list", "--count", `${ctx.live}..HEAD`));
  equal(status.unpublished, expected, `${label}: unpublished counts the commits since the last publish`);
  equal(status.remote, true, `${label}: the site has a remote to publish to`);
  return status.unpublished;
}

/** POST /api/publish pushes every saved commit, and only then does the published site move. [CMS-PUBLISH] */
async function publishSteps(ctx) {
  const label = say(ctx, "publish");
  const waiting = await unpublished(ctx, `${label} before`);
  ok(waiting > 0, `${label}: there are saved changes to publish`);
  equal(published(ctx.site), ctx.live, `${label}: nothing published until asked`);
  const reply = json(await call(ctx.port, "POST", "/api/publish"), 200, `${label}: POST /api/publish`);
  equal(reply.published, waiting, `${label}: every waiting commit published`);
  equal(reply.commit, head(ctx.site), `${label}: the published commit is HEAD`);
  equal(published(ctx.site), head(ctx.site), `${label}: the remote now holds HEAD`);
  ctx.live = head(ctx.site);
  equal(await unpublished(ctx, `${label} after`), 0, `${label}: nothing left to publish`);
  const again = json(await call(ctx.port, "POST", "/api/publish"), 200, `${label}: POST /api/publish again`);
  equal(again.published, 0, `${label}: publishing twice publishes nothing`);
  equal(published(ctx.site), head(ctx.site), `${label}: the remote is unchanged`);
  alive(ctx, label);
}

/** One editor session, in the order an editor lives it. */
const STEPS = [
  browse, editHome, publishSteps, nestedPage, namedPage, emptiedPage, deletedMedia,
  uploaded, uploadedImage, rejectedUploads, retriedUpload, unsafePaths, longestPath, historySteps, publishSteps,
];

/** One editor session against the server built for `memory`. */
async function session(memory) {
  const binary = build(memory);
  const site = path.join(out, `site-${memory}`);
  createSite(site);
  const ctx = { memory, site, port: PORT, files: new Set(Object.keys(SITE)), made: [], init: head(site), live: head(site), server: null };
  equal(log(site).length, 1, say(ctx, "the site starts with one commit"));
  deepEqual(git(site, "ls-files").split("\n"), Object.keys(SITE).sort(), say(ctx, "the site holds the files the developer committed"));
  await unused(PORT);
  try {
    ctx.server = await start(ctx, binary);
    for (const step of STEPS) await step(ctx);
  } finally {
    if (ctx.server) await stop(ctx.server);
  }
}

mkdirSync(out, { recursive: true });
execFileSync("node", [path.join(root, "src/toolchain/hosts/web/scripts/build.mjs"), "--app", path.join(project, "editor"), "--title", "Osprey Studio", "--out", EDITOR],
  { env: { ...process.env, OSPREY_BIN: osprey }, stdio: ["ignore", "inherit", "inherit"], timeout: BUILD_MS });
for (const memory of MEMORIES) {
  const before = assertions;
  await session(memory);
  console.log(`ok cms [${memory}]: ${assertions - before} assertions over an editor session`);
}
console.log(`${assertions} assertions passed, 0 failed`);

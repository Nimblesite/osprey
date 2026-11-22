// Serves a folder build.mjs wrote (index.html, host.js, host.css) on this machine only. The web
// client is static files; the application's own server is a separate process it talks to over HTTP.
//   node scripts/serve.mjs --root <folder> --port <port>
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import path from "node:path";

const TYPES = new Map([[".html", "text/html; charset=utf-8"], [".js", "application/javascript; charset=utf-8"], [".css", "text/css; charset=utf-8"]]);

function option(flag) {
  const at = process.argv.indexOf(flag);
  if (at < 0 || at + 1 >= process.argv.length) throw new Error(`usage: serve.mjs --root <folder> --port <port> (missing ${flag})`);
  return process.argv[at + 1];
}

const root = path.resolve(option("--root"));
const port = Number(option("--port"));

/** The file a request path names inside `root`, or "" when it names nothing servable there. */
function located(folder, url) {
  const name = decodeURIComponent(new URL(url, "http://localhost").pathname);
  const file = path.resolve(folder, `.${name === "/" ? "/index.html" : name}`);
  return file.startsWith(folder + path.sep) && TYPES.has(path.extname(file)) ? file : "";
}

async function answer(request, response) {
  const file = request.method === "GET" ? located(root, request.url ?? "/") : "";
  const body = file === "" ? null : await readFile(file).catch(() => null);
  const headers = { "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" };
  if (body === null) response.writeHead(404, { ...headers, "Content-Type": "text/plain; charset=utf-8" }).end("Not found");
  else response.writeHead(200, { ...headers, "Content-Type": TYPES.get(path.extname(file)) }).end(body);
}

createServer((request, response) => void answer(request, response).catch(() => response.writeHead(500).end()))
  .listen(port, "127.0.0.1", () => console.log(`[web] serving ${root} on http://127.0.0.1:${port}`));

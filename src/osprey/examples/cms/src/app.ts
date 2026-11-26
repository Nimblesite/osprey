// osprey: namespace=cms
// osprey: module=App

/** Every route of the CMS, registered on one application. [CMS-API] */
import express, { type Request, type Response } from "express";
import { type Config } from "./settings.ts";
import { history } from "./git.ts";
import { isMedia } from "./paths.ts";
import { failed, historyJson, problem, treeJson } from "./replies.ts";
import { asset, opened, previewed, removed, withPath, written } from "./routes.ts";
import { tree } from "./tree.ts";
import { partsOf, progressJson, receive } from "./upload.ts";

function progressed(res: Response, path, outcome) {
  switch (outcome.kind) {
    case "error":
      return failed(res, outcome.message);
    case "success":
      return res.json(progressJson(path, outcome.value));
  }
}

function numbered(config: Config, req: Request, res: Response, path, numbers) {
  switch (numbers.kind) {
    case "error":
      return problem(res, 400, numbers.message);
    case "success": {
      const part = unwrapOr(listGet(numbers.value, 0), 0);
      return progressed(res, path, receive(config, path, part, unwrapOr(listGet(numbers.value, 1), 0), req.body));
    }
  }
}

function uploaded(config: Config, req: Request, res: Response, path) {
  if (!isMedia(path)) {
    return problem(res, 400, `${path} is not an image, video or audio file`);
  }
  const numbers = partsOf(unwrapOr(mapGet(req.query, "part"), ""), unwrapOr(mapGet(req.query, "parts"), ""));
  return numbered(config, req, res, path, numbers);
}

function listed(res: Response, commits) {
  switch (commits.kind) {
    case "success":
      return res.json(historyJson(commits.value));
    case "error":
      return failed(res, commits.message);
  }
}

/** The application over `config`'s working tree. */
export function cms(config: Config) {
  const app = express();
  app.get("/", (_req, res) => asset(config, res, "index.html", "text/html; charset=utf-8"));
  app.get("/editor.css", (_req, res) => asset(config, res, "editor.css", "text/css; charset=utf-8"));
  app.get("/editor.js", (_req, res) => asset(config, res, "editor.js", "text/javascript; charset=utf-8"));
  app.get("/api/tree", (_req, res) => res.json(treeJson(config.roots, tree(config))));
  app.get("/api/file", (req, res) => withPath(config, req, res, (path) => opened(config, res, path)));
  app.put("/api/file", (req, res) => withPath(config, req, res, (path) => written(config, req, res, path)));
  app.delete("/api/file", (req, res) => withPath(config, req, res, (path) => removed(config, res, path)));
  app.post("/api/upload", (req, res) => withPath(config, req, res, (path) => uploaded(config, req, res, path)));
  app.get("/api/media", (req, res) => withPath(config, req, res, (path) => previewed(config, res, path)));
  app.get("/api/history", (_req, res) => listed(res, history(config)));
  return app.use("/", (req, res) => problem(res, 404, `no route for ${req.method} ${req.path}`));
}

/** Serve the CMS on `config.port`, then run `ready`. */
export function serve(config: Config, ready: () => void) {
  return cms(config).listen(config.port, ready);
}

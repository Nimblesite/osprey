// osprey: namespace=cms
// osprey: module=App

/** Every route of the CMS, registered on one application. [CMS-API] */
import express, { type Request, type Response } from "express";
import { type Config } from "./settings.ts";
import { history, publish, unpublished } from "./git.ts";
import { isMedia } from "./paths.ts";
import { failed, historyJson, problem, publishedJson, statusJson, treeJson } from "./replies.ts";
import { asset, opened, previewed, removed, withPath, written } from "./routes.ts";
import { tree } from "./tree.ts";
import { partsOf, progressJson, progressStatus, receive } from "./upload.ts";

function progressed(res, path, outcome) {
  switch (outcome.kind) {
    case "error":
      return failed(res, outcome.message);
    case "success":
      return res.status(progressStatus(outcome.value)).json(progressJson(path, outcome.value));
  }
}

function numbered(config, req, res, path, numbers) {
  switch (numbers.kind) {
    case "error":
      return problem(res, 400, numbers.message);
    case "success": {
      const part = unwrapOr(listGet(numbers.value, 0), 0);
      return progressed(res, path, receive(config, path, part, unwrapOr(listGet(numbers.value, 1), 0), req.body));
    }
  }
}

function uploaded(config, req, res, path) {
  if (!isMedia(path)) {
    return problem(res, 400, `${path} is not an image, video or audio file`);
  }
  const numbers = partsOf(unwrapOr(mapGet(req.query, "part"), ""), unwrapOr(mapGet(req.query, "parts"), ""));
  return numbered(config, req, res, path, numbers);
}

function listed(res, commits) {
  switch (commits.kind) {
    case "success":
      return res.json(historyJson(commits.value));
    case "error":
      return failed(res, commits.message);
  }
}

function status(res: Response, count) {
  switch (count.kind) {
    case "success":
      return res.json(statusJson(count.value, true, ""));
    case "error":
      return res.json(statusJson(0, false, count.message));
  }
}

function published(res, outcome) {
  switch (outcome.kind) {
    case "success":
      return res.json(publishedJson(outcome.value));
    case "error":
      return failed(res, outcome.message);
  }
}

/** The application over `config`'s working tree. */
export function cms(config: Config) {
  const app = express();
  app.get("/", (_req, res) => asset(config, res, "index.html", "text/html; charset=utf-8"));
  app.get("/host.css", (_req, res) => asset(config, res, "host.css", "text/css; charset=utf-8"));
  app.get("/host.js", (_req, res) => asset(config, res, "host.js", "text/javascript; charset=utf-8"));
  app.get("/api/tree", (_req, res) => res.json(treeJson(config.roots, tree(config))));
  app.get("/api/file", (req, res) => withPath(config, req, res, (path) => opened(config, res, path)));
  app.put("/api/file", (req, res) => withPath(config, req, res, (path) => written(config, req, res, path)));
  app.delete("/api/file", (req, res) => withPath(config, req, res, (path) => removed(config, res, path)));
  app.post("/api/upload", (req, res) => withPath(config, req, res, (path) => uploaded(config, req, res, path)));
  app.get("/api/media", (req, res) => withPath(config, req, res, (path) => previewed(config, res, path)));
  app.get("/api/history", (_req, res) => listed(res, history(config)));
  app.get("/api/status", (_req, res) => status(res, unpublished(config)));
  app.post("/api/publish", (_req, res) => published(res, publish(config)));
  return app.use("/", (req, res) => problem(res, 404, `no route for ${req.method} ${req.path}`));
}

/** Serve the CMS on `config.port`, then run `ready`. */
export function serve(config: Config, ready: () => void) {
  return cms(config).listen(config.port, ready);
}

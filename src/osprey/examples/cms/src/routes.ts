// osprey: namespace=cms
// osprey: module=Routes

/** The handlers behind each route of the CMS. [CMS-API] */
import { type Request, type Response } from "express";
import { type Config } from "./settings.ts";
import { removeFile, saveText } from "./git.ts";
import { isMedia, kindFor, safePath } from "./paths.ts";
import { failed, problem, savedJson } from "./replies.ts";
import { encoded } from "./upload.ts";

const TEXT = "text/plain; charset=utf-8";

/** The editor's own file `name`, typed, or a 500 naming what is missing. */
export function asset(config: Config, res: Response, name: string, type: string) {
  const text = readFile(`${config.editor}/${name}`);
  switch (text.kind) {
    case "success":
      return res.type(type).send(text.value);
    case "error":
      return failed(res, text.message);
  }
}

/** Whether `path` names an existing regular file. */
function exists(config: Config, path) {
  return unwrapOr(kindOf(`${config.repo}/${path}`), "") === "file";
}

function checked(config: Config, res: Response, path, work: (path: string) => Response) {
  const safe = safePath(config.roots, path);
  switch (safe.kind) {
    case "success":
      return work(safe.value);
    case "error":
      return problem(res, 400, safe.message);
  }
}

/** Run `work` on the request's `path` once it passes [CMS-PATH-SAFE], answering 400 otherwise. */
export function withPath(config: Config, req: Request, res: Response, work: (path: string) => Response) {
  const given = mapGet(req.query, "path");
  switch (given.kind) {
    case "success":
      return checked(config, res, given.value, work);
    case "error":
      return problem(res, 400, "path is required");
  }
}

function saving(res: Response, path, outcome) {
  switch (outcome.kind) {
    case "success":
      return res.json(savedJson(path, outcome.value));
    case "error":
      return failed(res, outcome.message);
  }
}

/** `GET /api/file`: the Markdown text of `path`. */
export function opened(config: Config, res: Response, path) {
  if (kindFor(path) !== "markdown") {
    return problem(res, 400, `${path} is not a Markdown file`);
  }
  const text = readFile(`${config.repo}/${path}`);
  switch (text.kind) {
    case "success":
      return res.type(TEXT).send(text.value);
    case "error":
      return problem(res, 404, `${path} does not exist`);
  }
}

/** `PUT /api/file`: save the body as `path` and commit. */
export function written(config: Config, req: Request, res: Response, path) {
  return kindFor(path) === "markdown" ? saving(res, path, saveText(config, path, req.body)) : problem(res, 400, `${path} is not a Markdown file`);
}

/** `DELETE /api/file`: remove `path` and commit. */
export function removed(config: Config, res: Response, path) {
  return exists(config, path) ? saving(res, path, removeFile(config, path)) : problem(res, 404, `${path} does not exist`);
}

/** `GET /api/media`: the bytes of media file `path` as base64. */
export function previewed(config: Config, res: Response, path) {
  if (!isMedia(path)) {
    return problem(res, 400, `${path} is not an image, video or audio file`);
  }
  return exists(config, path) ? sent(res, encoded(config, path)) : problem(res, 404, `${path} does not exist`);
}

function sent(res: Response, text) {
  switch (text.kind) {
    case "success":
      return res.type(TEXT).send(text.value);
    case "error":
      return failed(res, text.message);
  }
}

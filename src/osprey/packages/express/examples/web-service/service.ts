// osprey: namespace=routing
// osprey: module=Service
/**
 * Express's own `examples/web-service`, ported to TypeScript flavor: an API
 * key checked by middleware mounted on `/api`, JSON replies, an error handler
 * and a JSON 404. The original is `oracle/upstream/web-service/index.js`, and
 * the test suite compares this application's replies with it. [TS-PORT-EXPRESS]
 */
import express, { type HttpError, type Request, type Response } from "express";

interface Repo {
  readonly name: string;
  readonly url: string;
}

interface User {
  readonly name: string;
}

// create an error with a status. the custom error handler below reads it.
function httpError(status, message) {
  return { status: status, message: message } satisfies HttpError;
}

// map of valid api keys, typically mapped to account info with some sort of
// database like redis. api keys do _not_ serve as authentication, merely to
// track API usage or help prevent malicious behavior etc.
const apiKeys = ["foo", "bar", "baz"];

// these two lists will serve as our faux database
const expressRepo = { name: "express", url: "https://github.com/expressjs/express" } satisfies Repo;
const stylus = { name: "stylus", url: "https://github.com/learnboost/stylus" } satisfies Repo;
const cluster = { name: "cluster", url: "https://github.com/learnboost/cluster" } satisfies Repo;

const repos = [expressRepo, stylus, cluster];

const users = [{ name: "tobi" } satisfies User, { name: "loki" } satisfies User, { name: "jane" } satisfies User];

function userRepos() {
  return mapSet(mapSet(mapSet(new Map<string, Repo[]>(), "tobi", [expressRepo, stylus]), "loki", [stylus]), "jane", [cluster]);
}

// JSON stays at the edge: each record has one encoder.
function repoJson(repo: Repo) {
  return JsonObject([JsonMember("name", JsonString(repo.name)), JsonMember("url", JsonString(repo.url))]);
}

function userJson(user: User) {
  return JsonObject([JsonMember("name", JsonString(user.name))]);
}

function errorJson(message) {
  return JsonObject([JsonMember("error", JsonString(message))]);
}

// here we validate the API key, by mounting this middleware to /api, meaning
// only paths prefixed with "/api" will cause this middleware to be invoked
function checked(req: Request, _res: Response) {
  const key = mapGet(req.query, "api-key");
  switch (key.kind) {
    // key isn't present
    case "error":
      return req.fail(httpError(400, "api key required"));
    case "success":
      return known(req, key.value);
  }
}

// key is empty, or invalid; otherwise all good
function known(req, key) {
  return key === "" ? req.fail(httpError(400, "api key required")) : listContains(apiKeys, key) ? req.next() : req.fail(httpError(401, "invalid api key"));
}

// example: http://localhost:3000/api/user/tobi/repos/?api-key=foo
function reposOf(req: Request, res: Response) {
  const found = andThen(mapGet(req.params, "name"), (name) => mapGet(userRepos(), name));
  switch (found.kind) {
    case "success":
      return res.json(JsonArray(mapList(found.value, repoJson)));
    case "error":
      return req.next();
  }
}

/** The web-service application. */
export function service() {
  const app = express();

  app.use("/api", checked);

  // we now can assume the api key is valid, and simply expose the data

  // example: http://localhost:3000/api/users/?api-key=foo
  app.get("/api/users", (_req, res) => res.json(JsonArray(mapList(users, userJson))));

  // example: http://localhost:3000/api/repos/?api-key=foo
  app.get("/api/repos", (_req, res) => res.json(JsonArray(mapList(repos, repoJson))));

  app.get("/api/user/:name/repos", reposOf);

  return (
    app
      // error handlers receive what a handler passed to `req.fail`.
      .onError((failure, _req, res) => res.status(failure.status).json(errorJson(failure.message)))
      // our custom JSON 404 middleware. Since it's placed last it will be the last
      // middleware called, if all others pass the request on and do not respond.
      .use("/", (_req, res) => res.status(404).json(errorJson("Sorry, can't find that")))
  );
}

/** Serve the application on `port`. */
export function serve(port, ready) {
  return service().listen(port, ready);
}

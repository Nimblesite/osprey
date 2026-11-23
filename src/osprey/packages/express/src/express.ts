// osprey: namespace=express
// osprey: module=Express

/**
 * Express for Osprey: the ported surface of Express 5, served by the native
 * HTTP listener with no Node.js. [TS-PORT-EXPRESS]
 */

import { encodeUrl, escapeHtml } from "./encode.ts";
import { headerOf } from "./headers.ts";
import { queryOf } from "./query.ts";

/** What a handler passes to `req.fail`, and what an error handler receives. */
export interface HttpError {
  readonly status: int;
  readonly message: string;
}

/** An incoming request: the matched route's captures and the raw wire strings. */
export interface Request {
  readonly method: string;
  readonly url: string;
  readonly path: string;
  readonly params: Map<string, string>;
  readonly query: Map<string, string>;
  readonly headers: string;
  readonly body: string;
  /** `req.get(name)` is the value of the request header `name`, matched without regard to case. */
  get(name: string): Requires<Result<string, string>, []>;
  /** A handler returns `req.next()` to pass the request to the next matching handler. */
  next(): Requires<Response, []>;
  /** A handler returns `req.fail(error)` to pass `error` to the error handlers, as `next(error)` does in Express. */
  fail(error: HttpError): Requires<Response, []>;
}

/** What a handler's response asks of the dispatcher: send it, try the next handler, or run the error handlers. */
type Signal = { readonly kind: "replied" } | { readonly kind: "passed" } | { readonly kind: "raised"; readonly error: HttpError };

/** The response a handler builds; each method returns the extended response. */
export interface Response {
  readonly signal: Signal;
  readonly statusCode: int;
  readonly body: string;
  readonly contentType: string;
  readonly headers: string;
  status(code: int): Requires<Response, []>;
  set(name: string, value: string): Requires<Response, []>;
  send(body: string): Requires<Response, []>;
  json(value: Json): Requires<Response, []>;
}

/** A route answers one method at one path; middleware answers every method under a path prefix. */
interface Route {
  readonly method: string;
  readonly path: string;
  readonly prefix: boolean;
  readonly handler: (req: Request, res: Response) => Response;
}

/** An error handler, held in a record as a route's handler is. */
interface Rescue {
  readonly handler: (error: HttpError, req: Request, res: Response) => Response;
}

/**
 * An application: its routes and middleware in registration order, and its
 * error handlers. Each registration returns the extended application.
 */
export interface Application {
  readonly routes: Route[];
  readonly errors: Rescue[];
}

/** What `app.listen` returns: a listener and its handle, or why it could not start. */
export type Server = { readonly kind: "listening"; readonly handle: int } | { readonly kind: "failed"; readonly message: string };

/** Whether a route pattern matched a path, and the captures if it did. */
type Match = { readonly kind: "bound"; readonly params: Map<string, string> } | { readonly kind: "unbound" };

const HTML = "text/html; charset=utf-8";
const JSON_TYPE = "application/json; charset=utf-8";
/** The headers Express's final handler adds to the page it writes. */
const FINAL_HEADERS = "Content-Security-Policy: default-src 'none'\r\nX-Content-Type-Options: nosniff\r\n";

/** `express()` creates an application with no routes. [TS-PORT-EXPRESS-SURFACE]
 * # Examples
 * ```osprey
 * const app = express();
 * print(`[${listLength(app.routes)}]`);
 * ```
 * ```output
 * [0]
 * ```
 */
export function express(): Application {
  return { routes: [], errors: [] };
}

/** `app.get(path, handler)` routes GET requests for `path`.
 * # Examples
 * ```osprey
 * const app = get(express(), "/user/:name", (_req, res) => res.send("hello"));
 * const shown = (routes) => {
 *   const entry = listGet(routes, 0);
 *   switch (entry.kind) {
 *     case "success":
 *       return `${entry.value.method} ${entry.value.path} of ${listLength(routes)}`;
 *     case "error":
 *       return "none";
 *   }
 * };
 * print(`[${shown(app.routes)}]`);
 * ```
 * ```output
 * [GET /user/:name of 1]
 * ```
 */
export function get(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { ...app, routes: withRoute(app.routes, "GET", path, false, handler) };
}

/** `app.post(path, handler)` routes POST requests for `path`.
 * # Examples
 * ```osprey
 * const app = post(express(), "/tasks", (_req, res) => res.send("created"));
 * const shown = (routes) => {
 *   const entry = listGet(routes, 0);
 *   switch (entry.kind) {
 *     case "success":
 *       return `${entry.value.method} ${entry.value.path} of ${listLength(routes)}`;
 *     case "error":
 *       return "none";
 *   }
 * };
 * print(`[${shown(app.routes)}]`);
 * ```
 * ```output
 * [POST /tasks of 1]
 * ```
 */
export function post(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { ...app, routes: withRoute(app.routes, "POST", path, false, handler) };
}

/** `app.put(path, handler)` routes PUT requests for `path`.
 * # Examples
 * ```osprey
 * const app = put(express(), "/tasks/:id", (_req, res) => res.send("updated"));
 * const shown = (routes) => {
 *   const entry = listGet(routes, 0);
 *   switch (entry.kind) {
 *     case "success":
 *       return `${entry.value.method} ${entry.value.path} of ${listLength(routes)}`;
 *     case "error":
 *       return "none";
 *   }
 * };
 * print(`[${shown(app.routes)}]`);
 * ```
 * ```output
 * [PUT /tasks/:id of 1]
 * ```
 */
export function put(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { ...app, routes: withRoute(app.routes, "PUT", path, false, handler) };
}

/** `app.delete(path, handler)` routes DELETE requests for `path`. */
function remove(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { ...app, routes: withRoute(app.routes, "DELETE", path, false, handler) };
}

export { remove as delete };

/**
 * `app.use(path, handler)` runs `handler` for every method on `path` and the
 * paths under it. The handler replies, or returns `req.next()` to pass the
 * request on, or `req.fail(error)`. [TS-PORT-EXPRESS-HANDLERS]
 * # Examples
 * ```osprey
 * const app = use(express(), "/api", (req, _res) => req.next());
 * const shown = (routes) => {
 *   const entry = listGet(routes, 0);
 *   switch (entry.kind) {
 *     case "success":
 *       return `every method at ${entry.value.path} of ${listLength(routes)}`;
 *     case "error":
 *       return "none";
 *   }
 * };
 * print(`[${shown(app.routes)}]`);
 * ```
 * ```output
 * [every method at /api of 1]
 * ```
 */
export function use(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { ...app, routes: withRoute(app.routes, "", path === "/" ? "" : path, true, handler) };
}

/** `express.Router()` creates a router: an application of routes to mount on a path.
 * # Examples
 * ```osprey
 * const router = get(Router(), "/health", (_req, res) => res.send("ok"));
 * print(`[${listLength(router.routes)}]`);
 * ```
 * ```output
 * [1]
 * ```
 */
export function Router(): Application {
  return express();
}

/**
 * `app.mount(path, router)` adds the router's routes and error handlers under
 * `path`, which Express spells `app.use(path, router)`.
 * # Examples
 * ```osprey
 * const router = get(Router(), "/list", (_req, res) => res.send("[]"));
 * const app = mount(express(), "/api", router);
 * const shown = (routes) => {
 *   const entry = listGet(routes, 0);
 *   switch (entry.kind) {
 *     case "success":
 *       return `${entry.value.method} ${entry.value.path} of ${listLength(routes)}`;
 *     case "error":
 *       return "none";
 *   }
 * };
 * print(`[${shown(app.routes)}]`);
 * ```
 * ```output
 * [GET /api/list of 1]
 * ```
 */
export function mount(app: Application, path: string, router: Application): Application {
  return {
    routes: listConcat(app.routes, mapList(router.routes, (route) => ({ ...route, path: mounted(path, route.path) }))),
    errors: listConcat(app.errors, router.errors),
  };
}

/** A route path under a mount point: the router's root is the mount point itself. */
function mounted(prefix, path) {
  return path === "" || path === "/" ? prefix : `${prefix}${path}`;
}

/** `app.onError(handler)` adds an error handler: Express's four-parameter middleware.
 * # Examples
 * ```osprey
 * const app = onError(express(), (error, _req, res) => res.status(error.status).send(error.message));
 * print(`[${listLength(app.errors)}]`);
 * ```
 * ```output
 * [1]
 * ```
 */
export function onError(app: Application, handler: (error: HttpError, req: Request, res: Response) => Response): Application {
  return { ...app, errors: listConcat(app.errors, [{ handler: handler } satisfies Rescue]) };
}

function withRoute(routes, method, path, prefix, handler) {
  return listConcat(routes, [{ method: method, path: path, prefix: prefix, handler: handler } satisfies Route]);
}

function reply(statusCode, body, contentType, headers) {
  return signalled({ kind: "replied" } satisfies Signal, statusCode, body, contentType, headers);
}

function signalled(signal, statusCode, body, contentType, headers) {
  return {
    signal: signal,
    statusCode: statusCode,
    body: body,
    contentType: contentType,
    headers: headers,
    status: (code) => reply(code, body, contentType, headers),
    set: (name, value) => reply(statusCode, body, contentType, `${headers}${name}: ${value}\r\n`),
    send: (text) => reply(statusCode, text, contentType === "" ? HTML : contentType, headers),
    json: (value) => encoded(statusCode, jsonEncode(value), headers),
  } satisfies Response;
}

/** A value that cannot be encoded is a 500, never an empty body. */
function encoded(statusCode, text, headers) {
  switch (text.kind) {
    case "success":
      return reply(statusCode, text.value, JSON_TYPE, headers);
    case "error":
      return failure();
  }
}

function failure() {
  return reply(500, finalPage("Internal Server Error"), HTML, FINAL_HEADERS);
}

/**
 * `app.listen(port, callback)` starts the native listener and runs `callback`.
 * A listener that cannot start is reported on the console, as Node.js reports
 * the unhandled error, and returned as `failed`. [TS-PORT-EXPRESS-SERVE]
 * # Examples
 * ```osprey
 * const server = listen(express(), 70000, () => {});
 * const shown = (server) => {
 *   switch (server.kind) {
 *     case "listening":
 *       return `listening on ${server.handle}`;
 *     case "failed":
 *       return `failed: ${server.message}`;
 *   }
 * };
 * print(`[${shown(server)}]`);
 * ```
 * ```output
 * Error: listen on port 70000 failed: Error
 * [failed: Error]
 * ```
 */
export function listen(app: Application, port: int, callback: () => void): Server {
  const started = start(app, port);
  switch (started.kind) {
    case "success": {
      callback();
      return { kind: "listening", handle: started.value };
    }
    case "error": {
      console.log(`Error: listen on port ${port} failed: ${started.message}`);
      return { kind: "failed", message: started.message };
    }
  }
}

/** `server.close()` stops the listener, which lets the process exit.
 * # Examples
 * ```osprey
 * const stopped = close({ kind: "failed", message: "port held" } satisfies Server);
 * const shown = (outcome) => {
 *   switch (outcome.kind) {
 *     case "success":
 *       return "listener stopped";
 *     case "error":
 *       return outcome.message;
 *   }
 * };
 * print(`[${shown(stopped)}]`);
 * ```
 * ```output
 * [port held]
 * ```
 */
export function close(server: Server) {
  switch (server.kind) {
    case "listening":
      return httpStopServer(server.handle);
    case "failed":
      return Error(server.message);
  }
}

function start(app, port) {
  return andThen(httpCreateServer(port, "127.0.0.1"), (server) => serving(server, app));
}

/** As in Node.js, an open server keeps the process alive after the entry returns. */
function serving(server, app) {
  const listening = andThen(
    httpListen(server, (method, url, headers, body) => native(settled(dispatch(app, method, url, headers, body)))),
    (_listening) => httpKeepAlive(server),
  );
  switch (listening.kind) {
    case "success":
      return Success(server);
    case "error":
      return released(server, listening.message);
  }
}

/** A server that never listened is released; both failures are kept. */
function released(server, message) {
  const stopped = httpStopServer(server);
  switch (stopped.kind) {
    case "success":
      return Error(message);
    case "error":
      return Error(`${message}; releasing the server failed: ${stopped.message}`);
  }
}

/** A failure inside the port is a 500, never a crash. */
function settled(outcome: Result<Response, string>) {
  switch (outcome.kind) {
    case "success":
      return outcome.value;
    case "error":
      return failure();
  }
}

function native(res) {
  const contentType = res.contentType === "" ? "" : `Content-Type: ${res.contentType}\r\n`;
  return {
    status: res.statusCode,
    headers: `X-Powered-By: Express\r\n${contentType}${res.headers}`,
    contentType: res.contentType,
    streamFd: -1,
    isComplete: true,
    partialBody: res.body,
  } satisfies HttpResponse;
}

function dispatch(app, method, url, headers, body) {
  const path = pathOf(url);
  const passed = signalled({ kind: "passed" } satisfies Signal, 200, "", "", "");
  const base: Request = {
    method: method,
    url: url,
    path: path,
    params: new Map<string, string>(),
    query: queryOf(url),
    headers: headers,
    body: body,
    get: (name) => headerOf(headers, name),
    next: () => passed,
    fail: (error) => signalled({ kind: "raised", error: error } satisfies Signal, 200, "", "", ""),
  };
  return andThen(split(path, "/"), (parts) => route(app, 0, base, parts));
}

/** The first route or middleware, from `index` in registration order, whose method and pattern match. */
function route(app, index, base, parts) {
  const candidate = listGet(app.routes, index);
  switch (candidate.kind) {
    case "success":
      return attempt(candidate.value, app, index, base, parts);
    case "error":
      return notFound(base.method, base.path);
  }
}

function attempt(candidate, app, index, base, parts) {
  if (candidate.method !== "" && candidate.method !== base.method) {
    return route(app, satAdd(index, 1), base, parts);
  }
  return andThen(bind(candidate.path, parts, endsWith(base.path, "/"), candidate.prefix), (matched) => served(matched, candidate, app, index, base, parts));
}

function served(matched, candidate, app, index, base, parts) {
  switch (matched.kind) {
    case "bound":
      return followed(candidate.handler({ ...base, params: matched.params }, reply(200, "", "", "")), app, index, base, parts);
    case "unbound":
      return route(app, satAdd(index, 1), base, parts);
  }
}

/** What a handler's response asked for: it is sent, or the next handler runs, or the error handlers do. */
function followed(response, app, index, base, parts) {
  const signal = response.signal;
  switch (signal.kind) {
    case "replied":
      return Success(response);
    case "passed":
      return route(app, satAdd(index, 1), base, parts);
    case "raised":
      return Success(caught(app, 0, signal.error, base));
  }
}

/** The first error handler from `index`; with none left, Express's final handler answers with the error's status. */
function caught(app, index, error, base) {
  const rescue = listGet(app.errors, index);
  switch (rescue.kind) {
    case "success":
      return recaught(handled(rescue.value, error, base), app, index, error, base);
    case "error":
      return reply(error.status, finalPage(error.message), HTML, FINAL_HEADERS);
  }
}

function handled(rescue, error, base) {
  return rescue.handler(error, base, reply(200, "", "", ""));
}

/** An error handler that passes hands the same error on; one that fails hands on its own. */
function recaught(response, app, index, error, base) {
  const signal = response.signal;
  switch (signal.kind) {
    case "replied":
      return response;
    case "passed":
      return caught(app, satAdd(index, 1), error, base);
    case "raised":
      return caught(app, satAdd(index, 1), signal.error, base);
  }
}

function pathOf(url) {
  const mark = indexOf(url, "?");
  switch (mark.kind) {
    case "success":
      return take(url, mark.value);
    case "error":
      return url;
  }
}

/** Match a route pattern against a split path, capturing each `:name` segment. */
function bind(pattern, parts, slashed, prefix) {
  return andThen(split(pattern, "/"), (patterns) => bound(patterns, parts, slashed, prefix));
}

/**
 * Express routes are not strict: one trailing slash on the request path is
 * ignored, so `/user/` reaches the route `/user`. Middleware matches a prefix:
 * its pattern binds the leading segments and the rest of the path is free.
 */
function bound(patterns, parts, slashed, prefix) {
  const extra = listLength(parts) === satAdd(listLength(patterns), 1);
  const whole = listLength(patterns) === listLength(parts) || (extra && slashed);
  return (prefix ? listLength(patterns) <= listLength(parts) : whole)
    ? bindFrom(patterns, parts, 0, new Map<string, string>())
    : Success({ kind: "unbound" } satisfies Match);
}

function bindFrom(patterns, parts, index, params) {
  if (index === listLength(patterns)) {
    return Success({ kind: "bound", params: params } satisfies Match);
  }
  return andThen(segment(patterns, index), (pattern) =>
    andThen(segment(parts, index), (part) => bindSegment(patterns, parts, index, params, pattern, part)),
  );
}

/** The segment at `index`, or the list read's failure as a message. */
function segment(segments, index) {
  const read = listGet(segments, index);
  switch (read.kind) {
    case "success":
      return Success(read.value);
    case "error":
      return Error("index out of bounds");
  }
}

/** A `:name` segment captures as written; a literal one matches without regard to case. */
function bindSegment(patterns, parts, index, params, pattern, part) {
  if (startsWith(pattern, ":")) {
    return part === ""
      ? Success({ kind: "unbound" } satisfies Match)
      : bindFrom(patterns, parts, satAdd(index, 1), mapSet(params, drop(pattern, 1), part));
  }
  return toLowerCase(pattern) === toLowerCase(part)
    ? bindFrom(patterns, parts, satAdd(index, 1), params)
    : Success({ kind: "unbound" } satisfies Match);
}

/** Express's final handler: `Cannot METHOD /path`, with its security headers. */
function notFound(method, path) {
  return andThen(escapeHtml(encodeUrl(path)), (shown) =>
    Success(reply(404, finalPage(`Cannot ${method} ${shown}`), HTML, FINAL_HEADERS)),
  );
}

function finalPage(message) {
  return `<!DOCTYPE html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>Error</title>\n</head>\n<body>\n<pre>${message}</pre>\n</body>\n</html>\n`;
}

/*
 * JSON request bodies, decoded as `express.json()` decodes them: only a JSON
 * content type is read, and the document must be an object or an array.
 * [TS-PORT-EXPRESS-HANDLERS]
 */


/**
 * `jsonBody(req)` is the request's JSON document: `JsonNull` when the request
 * is not JSON, as Express leaves `req.body` unset, and the error `malformed
 * JSON` when it is JSON but not an object or an array, for which Express's
 * parser answers 400.
 * # Examples
 * ```osprey
 * const requestWith = (type, body) => {
 *   return {
 *     method: "POST",
 *     url: "/api/tasks",
 *     path: "/api/tasks",
 *     params: new Map<string, string>(),
 *     query: new Map<string, string>(),
 *     headers: `Content-Type: ${type}\r\n`,
 *     body: body,
 *     get: (_name) => Error("no such header"),
 *     next: () => reply(200, "", "", ""),
 *     fail: (error) => reply(error.status, "", "", ""),
 *   } satisfies Request;
 * };
 * const document = jsonBody(requestWith("application/json", "[\"ship\",\"it\"]"));
 * const plain = jsonBody(requestWith("text/plain", "[\"ship\",\"it\"]"));
 * const broken = jsonBody(requestWith("application/json", "{\"title\":"));
 * const shown = (document) => {
 *   switch (document.kind) {
 *     case "success":
 *       return `${document.value}`;
 *     case "error":
 *       return document.message;
 *   }
 * };
 * print(`[${shown(document)}] [${shown(plain)}] [${shown(broken)}]`);
 * ```
 * ```output
 * [JsonArray([JsonString("ship"), JsonString("it")])] [JsonNull] [malformed JSON]
 * ```
 */
export function jsonBody(req: Request) {
  return isJson(headerOf(req.headers, "Content-Type")) ? decoded(trim(req.body)) : Success(JsonNull);
}

function isJson(type) {
  switch (type.kind) {
    case "success": {
      const media = toLowerCase(type.value);
      return startsWith(media, "application/json") || contains(media, "+json");
    }
    case "error":
      return false;
  }
}

function decoded(text) {
  const first = take(text, 1);
  return first === "{" || first === "[" ? parsed(jsonDecode(text)) : malformed();
}

function parsed(document) {
  switch (document.kind) {
    case "success":
      return Success(document.value);
    case "error":
      return malformed();
  }
}

function malformed() {
  return Error("malformed JSON");
}

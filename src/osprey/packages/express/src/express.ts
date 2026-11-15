// osprey: namespace=express
// osprey: module=Express

/** An incoming request: the matched route's captures plus the raw wire strings. */
export interface Request {
  readonly method: string;
  readonly url: string;
  readonly path: string;
  readonly params: Map<string, string>;
  readonly headers: string;
  readonly body: string;
}

/** The response a handler builds; each method returns the extended response. */
export interface Response {
  readonly statusCode: int;
  readonly body: string;
  readonly contentType: string;
  readonly headers: string;
  status(code: int): Requires<Response, []>;
  set(name: string, value: string): Requires<Response, []>;
  send(body: string): Requires<Response, []>;
}

interface Route {
  readonly method: string;
  readonly path: string;
  readonly handler: (req: Request, res: Response) => Response;
}

/** An application: an ordered route table; each registration returns the extended application. */
export interface Application {
  readonly routes: Route[];
}

const HTML = "text/html; charset=utf-8";

/** `express()` creates an application with no routes. [TS-PORT-EXPRESS-SURFACE] */
export function express(): Application {
  return { routes: [] };
}

/** `app.get(path, handler)` routes GET requests for `path`. */
export function get(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { routes: withRoute(app.routes, "GET", path, handler) };
}

/** `app.post(path, handler)` routes POST requests for `path`. */
export function post(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { routes: withRoute(app.routes, "POST", path, handler) };
}

/** `app.put(path, handler)` routes PUT requests for `path`. */
export function put(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { routes: withRoute(app.routes, "PUT", path, handler) };
}

function withRoute(routes: Route[], method: string, path: string, handler: (req: Request, res: Response) => Response): Route[] {
  return listConcat(routes, [{ method: method, path: path, handler: handler } satisfies Route]);
}

function reply(statusCode: int, body: string, contentType: string, headers: string): Response {
  return {
    statusCode: statusCode,
    body: body,
    contentType: contentType,
    headers: headers,
    status: (code) => reply(code, body, contentType, headers),
    set: (name, value) => reply(statusCode, body, contentType, `${headers}${name}: ${value}\r\n`),
    send: (text) => reply(statusCode, text, contentType === "" ? HTML : contentType, headers),
  };
}

/** `app.listen(port, callback)` starts the native listener. [TS-PORT-EXPRESS-SERVE] */
export function listen(app: Application, port: int, callback: () => void): Result<int, string> {
  const created = httpCreateServer(port, "127.0.0.1");
  switch (created.kind) {
    case "success":
      return listening(created.value, app.routes, callback);
    case "error":
      return Error(created.message);
  }
}

function listening(server: int, routes: Route[], callback: () => void): Result<int, string> {
  const started = httpListen(server, (method, url, headers, body) => wire(dispatch(routes, method, url, headers, body)));
  switch (started.kind) {
    case "success": {
      callback();
      return Success(server);
    }
    case "error":
      return Error(started.message);
  }
}

function wire(res: Response): HttpResponse {
  const contentType = res.contentType === "" ? "" : `Content-Type: ${res.contentType}\r\n`;
  return {
    status: res.statusCode,
    headers: `X-Powered-By: Express\r\n${contentType}${res.headers}`,
    contentType: res.contentType,
    streamFd: -1,
    isComplete: true,
    partialBody: res.body,
  };
}

function dispatch(routes: Route[], method: string, url: string, headers: string, body: string): Response {
  const path = pathOf(url);
  const base: Request = { method: method, url: url, path: path, params: new Map<string, string>(), headers: headers, body: body };
  return route(routes, 0, base);
}

function route(routes: Route[], index: int, base: Request): Response {
  const candidate = listGet(routes, index);
  switch (candidate.kind) {
    case "success":
      return attempt(candidate.value, routes, index, base);
    case "error":
      return notFound(base.method, base.path);
  }
}

function attempt(candidate: Route, routes: Route[], index: int, base: Request): Response {
  if (candidate.method !== base.method) {
    return route(routes, satAdd(index, 1), base);
  }
  const bound = bind(candidate.path, base.path);
  switch (bound.kind) {
    case "success":
      return candidate.handler({ ...base, params: bound.value }, reply(200, "", "", ""));
    case "error":
      return route(routes, satAdd(index, 1), base);
  }
}

function pathOf(url: string): string {
  const mark = indexOf(url, "?");
  switch (mark.kind) {
    case "success":
      return take(url, mark.value);
    case "error":
      return url;
  }
}

function segments(path: string): string[] {
  const parts = split(path, "/");
  switch (parts.kind) {
    case "success":
      return parts.value;
    case "error":
      return [];
  }
}

function at(parts: string[], index: int): string {
  const part = listGet(parts, index);
  switch (part.kind) {
    case "success":
      return part.value;
    case "error":
      return "";
  }
}

/** Match a route pattern, capturing each `:name` segment. */
function bind(pattern: string, path: string): Result<Map<string, string>, string> {
  const patterns = segments(pattern);
  const parts = segments(path);
  return listLength(patterns) === listLength(parts)
    ? bindFrom(patterns, parts, 0, new Map<string, string>())
    : Error("segment count");
}

function bindFrom(patterns: string[], parts: string[], index: int, params: Map<string, string>): Result<Map<string, string>, string> {
  if (index === listLength(patterns)) {
    return Success(params);
  }
  const pattern = at(patterns, index);
  const part = at(parts, index);
  if (startsWith(pattern, ":")) {
    return part === "" ? Error("empty capture") : bindFrom(patterns, parts, satAdd(index, 1), mapSet(params, drop(pattern, 1), part));
  }
  return pattern === part ? bindFrom(patterns, parts, satAdd(index, 1), params) : Error("literal segment");
}

function swap(text: string, from: string, to: string): string {
  const swapped = replace(text, from, to);
  switch (swapped.kind) {
    case "success":
      return swapped.value;
    case "error":
      return "";
  }
}

/** The five replacements of `escape-html`, which Express applies to every reflected path. */
function escapeHtml(text: string): string {
  return swap(swap(swap(swap(swap(text, "&", "&amp;"), "<", "&lt;"), ">", "&gt;"), "\"", "&quot;"), "'", "&#39;");
}

/** Express's final handler: `Cannot METHOD /path`, with its security headers. */
function notFound(method: string, path: string): Response {
  const page = `<!DOCTYPE html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>Error</title>\n</head>\n<body>\n<pre>Cannot ${method} ${escapeHtml(path)}</pre>\n</body>\n</html>\n`;
  return reply(404, page, HTML, "Content-Security-Policy: default-src 'none'\r\nX-Content-Type-Options: nosniff\r\n");
}

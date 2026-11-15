// osprey: namespace=express
// osprey: module=Express

/**
 * Express for Osprey: the ported surface of Express 5, served by the native
 * HTTP listener with no Node.js. [TS-PORT-EXPRESS]
 */

/** An incoming request: the matched route's captures and the raw wire strings. */
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

/** An application: an ordered route table. Each registration returns the extended application. */
export interface Application {
  readonly routes: Route[];
}

/** What `app.listen` returns: a listener and its handle, or why it could not start. */
export type Server = { readonly kind: "listening"; readonly handle: int } | { readonly kind: "failed"; readonly message: string };

/** Whether a route pattern matched a path, and the captures if it did. */
type Match = { readonly kind: "bound"; readonly params: Map<string, string> } | { readonly kind: "unbound" };

const HTML = "text/html; charset=utf-8";
const HEX = "0123456789ABCDEF";
/** The printable ASCII characters from `!` (33) to `~` (126), by code. */
const PRINTABLE = "!\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\\]^_`abcdefghijklmnopqrstuvwxyz{|}~";
const FIRST_PRINTABLE = 33;
/** The headers Express's final handler adds to the page it writes. */
const FINAL_HEADERS = "Content-Security-Policy: default-src 'none'\r\nX-Content-Type-Options: nosniff\r\n";

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

/** `app.delete(path, handler)` routes DELETE requests for `path`. */
function remove(app: Application, path: string, handler: (req: Request, res: Response) => Response): Application {
  return { routes: withRoute(app.routes, "DELETE", path, handler) };
}

export { remove as delete };

function withRoute(routes, method, path, handler) {
  return listConcat(routes, [{ method: method, path: path, handler: handler } satisfies Route]);
}

function reply(statusCode, body, contentType, headers) {
  return {
    statusCode: statusCode,
    body: body,
    contentType: contentType,
    headers: headers,
    status: (code) => reply(code, body, contentType, headers),
    set: (name, value) => reply(statusCode, body, contentType, `${headers}${name}: ${value}\r\n`),
    send: (text) => reply(statusCode, text, contentType === "" ? HTML : contentType, headers),
  } satisfies Response;
}

/**
 * `app.listen(port, callback)` starts the native listener and runs `callback`.
 * A listener that cannot start is reported on the console, as Node.js reports
 * the unhandled error, and returned as `failed`. [TS-PORT-EXPRESS-SERVE]
 */
export function listen(app: Application, port: int, callback: () => void): Server {
  const started = start(app.routes, port);
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

/** `server.close()` stops the listener, which lets the process exit. */
export function close(server: Server) {
  switch (server.kind) {
    case "listening":
      return httpStopServer(server.handle);
    case "failed":
      return Error(server.message);
  }
}

function start(routes, port) {
  return andThen(httpCreateServer(port, "127.0.0.1"), (server) => serving(server, routes));
}

/** As in Node.js, an open server keeps the process alive after the entry returns. */
function serving(server, routes) {
  const listening = andThen(
    httpListen(server, (method, url, headers, body) => wire(dispatch(routes, method, url, headers, body))),
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

/** The runtime's response record; a failure inside the port is a 500, never a crash. */
function wire(outcome: Result<Response, string>) {
  switch (outcome.kind) {
    case "success":
      return native(outcome.value);
    case "error":
      return native(reply(500, finalPage("Internal Server Error"), HTML, FINAL_HEADERS));
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

function dispatch(routes, method, url, headers, body) {
  const path = pathOf(url);
  const base: Request = { method: method, url: url, path: path, params: new Map<string, string>(), headers: headers, body: body };
  return andThen(split(path, "/"), (parts) => route(routes, 0, base, parts));
}

/** The first route, in registration order, whose method and pattern match. */
function route(routes, index, base, parts) {
  const candidate = listGet(routes, index);
  switch (candidate.kind) {
    case "success":
      return attempt(candidate.value, routes, index, base, parts);
    case "error":
      return notFound(base.method, base.path);
  }
}

function attempt(candidate, routes, index, base, parts) {
  if (candidate.method !== base.method) {
    return route(routes, satAdd(index, 1), base, parts);
  }
  return andThen(bind(candidate.path, parts), (matched) => served(matched, candidate, routes, index, base, parts));
}

function served(matched, candidate, routes, index, base, parts) {
  switch (matched.kind) {
    case "bound":
      return Success(candidate.handler({ ...base, params: matched.params }, reply(200, "", "", "")));
    case "unbound":
      return route(routes, satAdd(index, 1), base, parts);
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
function bind(pattern, parts) {
  return andThen(split(pattern, "/"), (patterns) => bound(patterns, parts));
}

/**
 * Express routes are not strict: one trailing slash on the request path is
 * ignored, so `/user/` reaches the route `/user`.
 */
function bound(patterns, parts) {
  const extra = listLength(parts) === satAdd(listLength(patterns), 1);
  return listLength(patterns) === listLength(parts) || (extra && lastIsEmpty(parts))
    ? bindFrom(patterns, parts, 0, new Map<string, string>())
    : Success({ kind: "unbound" } satisfies Match);
}

function lastIsEmpty(parts) {
  const last = listGet(parts, satSub(listLength(parts), 1));
  switch (last.kind) {
    case "success":
      return last.value === "";
    case "error":
      return false;
  }
}

function bindFrom(patterns, parts, index, params) {
  if (index === listLength(patterns)) {
    return Success({ kind: "bound", params: params } satisfies Match);
  }
  return andThen(listGet(patterns, index), (pattern) =>
    andThen(listGet(parts, index), (part) => bindSegment(patterns, parts, index, params, pattern, part)),
  );
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

/** The five replacements of `escape-html`, applied to every reflected path. */
function escapeHtml(text) {
  return andThen(replace(text, "&", "&amp;"), (amp) =>
    andThen(replace(amp, "<", "&lt;"), (lt) =>
      andThen(replace(lt, ">", "&gt;"), (gt) =>
        andThen(replace(gt, "\"", "&quot;"), (quot) => replace(quot, "'", "&#39;")),
      ),
    ),
  );
}

/** `encodeurl`: percent-encode every byte a URL cannot carry as written. */
function encodeUrl(text) {
  return encodeFrom(text, 0, "");
}

function encodeFrom(text, index, encoded) {
  const byte = byteOr(text, index);
  if (byte < 0) {
    return encoded;
  }
  return encodeFrom(text, satAdd(index, 1), `${encoded}${encodedByte(text, index, byte)}`);
}

/** The byte at `index`, or -1 past the end of `text`. */
function byteOr(text, index) {
  const byte = byteAt(text, index);
  switch (byte.kind) {
    case "success":
      return byte.value;
    case "error":
      return -1;
  }
}

function encodedByte(text, index, byte) {
  if (byte === 37) {
    return isHex(byteOr(text, satAdd(index, 1))) && isHex(byteOr(text, satAdd(index, 2))) ? "%" : "%25";
  }
  return urlSafe(byte) ? take(drop(PRINTABLE, satSub(byte, FIRST_PRINTABLE)), 1) : percent(byte);
}

/** Whether `encodeurl` leaves this byte as written. */
function urlSafe(byte) {
  return byte === 33 || (byte >= 35 && byte <= 59) || byte === 61 || (byte >= 63 && byte <= 95) || (byte >= 97 && byte <= 122) || byte === 124 || byte === 126;
}

function isHex(byte) {
  return (byte >= 48 && byte <= 57) || (byte >= 65 && byte <= 70) || (byte >= 97 && byte <= 102);
}

function percent(byte) {
  return `%${hexDigit(shiftRight(byte, 4))}${hexDigit(bitAnd(byte, 15))}`;
}

function hexDigit(value) {
  return take(drop(HEX, value), 1);
}

// osprey: namespace=routing

/**
 * The Express package suite: the routes and web-service examples served
 * natively over real loopback HTTP, each reply checked against what Express
 * 5.3.0 sent for the same request in `examples.expected.json`, then the error
 * handler chain, query strings, JSON bodies, a mounted router and request
 * headers. [TS-PORT-EXPRESS-ORACLE]
 */
import express, { close, type HttpError, type Request, type Response, type Server } from "express";
import { tasks } from "../examples/routes/tasks.ts";
import { service } from "../examples/web-service/service.ts";
import { api } from "../examples/json-api/api.ts";

/** The GC and ARC runs of this suite may overlap, so each takes the first free port. */
const PORTS = [18921, 18923, 18925, 18927, 18929];
const HTML = "text/html; charset=utf-8";
const JSON_TYPE = "application/json; charset=utf-8";
const USERS = "[{\"name\":\"tobi\"},{\"name\":\"loki\"},{\"name\":\"jane\"}]";
const NOT_FOUND = "{\"error\":\"Sorry, can't find that\"}";
const NO_KEY = "{\"error\":\"api key required\"}";
const BAD_KEY = "{\"error\":\"invalid api key\"}";
const INVALID_PORT = 70000;
/** A port the suite holds open itself, so a second listener on it fails the same way in every run. */
const HELD_PORT = 18941;
const HOLD_ATTEMPTS = 400;
const HOLD_WAIT_MS = 25;

interface Listener {
  readonly server: Server;
  readonly port: int;
}

function page(message) {
  return `<!DOCTYPE html>\n<html lang="en">\n<head>\n<meta charset="utf-8">\n<title>Error</title>\n</head>\n<body>\n<pre>${message}</pre>\n</body>\n</html>\n`;
}

/** Whether nothing listens on `port`: a quiet bind that is released at once. */
function free(port) {
  const probe = httpCreateServer(port, "127.0.0.1");
  switch (probe.kind) {
    case "success":
      return released(httpListen(probe.value, (_method, _url, _headers, _body) => idle()), httpStopServer(probe.value));
    case "error":
      return false;
  }
}

/** Hold `HELD_PORT` with a quiet listener, waiting while an overlapping run holds it. */
function held(attempt) {
  if (attempt === HOLD_ATTEMPTS) {
    return Error(`port ${HELD_PORT} stayed busy`);
  }
  const holder = httpCreateServer(HELD_PORT, "127.0.0.1");
  switch (holder.kind) {
    case "success":
      return holding(holder.value, httpListen(holder.value, (_method, _url, _headers, _body) => idle()), attempt);
    case "error":
      return retried(attempt);
  }
}

function holding(holder, listened, attempt) {
  switch (listened.kind) {
    case "success":
      return Success(holder);
    case "error":
      return abandoned(httpStopServer(holder), attempt);
  }
}

/** A holder that could not listen is released before the next attempt. */
function abandoned(stopped, attempt) {
  switch (stopped.kind) {
    case "success":
      return retried(attempt);
    case "error":
      return Error(`releasing port ${HELD_PORT} failed: ${stopped.message}`);
  }
}

function retried(attempt) {
  sleep(HOLD_WAIT_MS);
  return held(satAdd(attempt, 1));
}

function released(listened, stopped) {
  switch (listened.kind) {
    case "success":
      return succeeded(stopped);
    case "error":
      return false;
  }
}

function succeeded(outcome) {
  switch (outcome.kind) {
    case "success":
      return true;
    case "error":
      return false;
  }
}

function idle() {
  return { status: 503, headers: "", contentType: "", streamFd: -1, isComplete: true, partialBody: "" } satisfies HttpResponse;
}

/** Serve the application named `which` on the first free port from `index` on. */
function served(which, index) {
  const port = listGet(PORTS, index);
  switch (port.kind) {
    case "success":
      return free(port.value) ? Success({ server: listening(which, port.value), port: port.value } satisfies Listener) : served(which, satAdd(index, 1));
    case "error":
      return Error("no loopback test port is free");
  }
}

/** Each application listens where it is built, so its handlers stay statically known. */
function listening(which, port) {
  return which === "routes"
    ? tasks().listen(port, () => {})
    : which === "web-service"
      ? service().listen(port, () => {})
      : which === "json-api"
        ? api().listen(port, () => {})
        : faulty().listen(port, () => {});
}

function checked(outcome, status, body) {
  return typed(outcome, status, body, HTML);
}

function typed(outcome, status, body, contentType) {
  switch (outcome.kind) {
    case "error":
      return check("HTTP transport", "success", outcome.message);
    case "success": {
      expect(httpResponseStatus(outcome.value), status);
      expect(httpResponseBody(outcome.value), Success(body));
      expect(httpResponseHeader(outcome.value, "Content-Type"), Success(contentType));
      expect(httpResponseHeader(outcome.value, "X-Powered-By"), Success("Express"));
      expect(httpResponseHeader(outcome.value, "Content-Length"), Success(toString(byteLength(body))));
      return expect(httpResponseFree(outcome.value), Success(0));
    }
  }
}

/** A reply's extra header, such as `Location`, then the reply itself. */
function headed(outcome, name, value, status, body) {
  switch (outcome.kind) {
    case "error":
      return check("HTTP transport", "success", outcome.message);
    case "success": {
      expect(httpResponseHeader(outcome.value, name), Success(value));
      return checked(outcome, status, body);
    }
  }
}

/** Express's final handler: the page and both of its security headers. */
function missing(outcome, shown) {
  switch (outcome.kind) {
    case "error":
      return check("HTTP transport", "success", outcome.message);
    case "success": {
      expect(httpResponseHeader(outcome.value, "Content-Security-Policy"), Success("default-src 'none'"));
      expect(httpResponseHeader(outcome.value, "X-Content-Type-Options"), Success("nosniff"));
      return checked(outcome, 404, page(shown));
    }
  }
}

function routed(client) {
  checked(httpGetResponse(client, "/", ""), 200, "Osprey serves these routes natively.");
  headed(httpGetResponse(client, "/health", ""), "Cache-Control", "no-store", 200, "ok");
  headed(httpGetResponse(client, "/HEALTH/", ""), "Cache-Control", "no-store", 200, "ok");
  headed(httpPostResponse(client, "/tasks", "", ""), "Location", "/tasks/current", 201, "Task created");
  headed(httpPostResponse(client, "/tasks/", "", ""), "Location", "/tasks/current", 201, "Task created");
  checked(httpGetResponse(client, "/tasks/7", ""), 200, "Task found");
  checked(httpGetResponse(client, "/TASKS/7/", ""), 200, "Task found");
  checked(httpGetResponse(client, "/tasks/current", ""), 200, "Task found");
  expect(httpPut(client, "/tasks/current", "", ""), Success(200));
  expect(httpDelete(client, "/tasks/current", ""), Success(410));
  return expect(httpDelete(client, "/tasks/current?force=true", ""), Success(410));
}

function unrouted(client) {
  missing(httpGetResponse(client, "/tasks", ""), "Cannot GET /tasks");
  missing(httpGetResponse(client, "/tasks/", ""), "Cannot GET /tasks/");
  missing(httpGetResponse(client, "/tasks/7/extra", ""), "Cannot GET /tasks/7/extra");
  missing(httpGetResponse(client, "/tasks/current/extra?probe=kept-out", ""), "Cannot GET /tasks/current/extra");
  missing(httpGetResponse(client, "/%41%zz", ""), "Cannot GET /%41%25zz");
  missing(httpGetResponse(client, "/50%25off", ""), "Cannot GET /50%25off");
  missing(httpPostResponse(client, "/health", "", ""), "Cannot POST /health");
  missing(httpGetResponse(client, "/<b>&amp;", ""), "Cannot GET /%3Cb%3E&amp;amp;");
  expect(httpPut(client, "/tasks", "", ""), Success(404));
  return expect(httpDelete(client, "/", ""), Success(404));
}

/** A listener that cannot start is reported, and closing the failure reports why. */
function refusedOn(port) {
  const rival = tasks().listen(port, () => {});
  switch (rival.kind) {
    case "listening":
      return check(`listener on port ${port}`, "failed", "listening");
    case "failed":
      return expect(succeeded(close(rival)), false);
  }
}

/** A second listener on a port in use binds, fails to listen, and is released. */
function occupied() {
  const holder = held(0);
  switch (holder.kind) {
    case "error":
      return check("held port", "success", holder.message);
    case "success": {
      refusedOn(HELD_PORT);
      return expect(httpStopServer(holder.value), Success(0));
    }
  }
}

/** The web-service example: its key middleware, JSON routes, error handler and JSON 404. */
function keyed(client) {
  typed(httpGetResponse(client, "/api/users?api-key=foo", ""), 200, USERS, JSON_TYPE);
  typed(httpGetResponse(client, "/API/Users/?x=1&api-key=b%61r&y=2", ""), 200, USERS, JSON_TYPE);
  typed(httpGetResponse(client, "/api/repos?api-key=baz", ""), 200, "[{\"name\":\"express\",\"url\":\"https://github.com/expressjs/express\"},{\"name\":\"stylus\",\"url\":\"https://github.com/learnboost/stylus\"},{\"name\":\"cluster\",\"url\":\"https://github.com/learnboost/cluster\"}]", JSON_TYPE);
  typed(httpGetResponse(client, "/api/user/loki/repos?api-key=foo", ""), 200, "[{\"name\":\"stylus\",\"url\":\"https://github.com/learnboost/stylus\"}]", JSON_TYPE);
  typed(httpGetResponse(client, "/api/user/jane/repos/?api-key=foo", ""), 200, "[{\"name\":\"cluster\",\"url\":\"https://github.com/learnboost/cluster\"}]", JSON_TYPE);
  typed(httpGetResponse(client, "/api/user/nobody/repos?api-key=foo", ""), 404, NOT_FOUND, JSON_TYPE);
  typed(httpGetResponse(client, "/api/missing?api-key=foo", ""), 404, NOT_FOUND, JSON_TYPE);
  typed(httpGetResponse(client, "/api?api-key=foo", ""), 404, NOT_FOUND, JSON_TYPE);
  typed(httpGetResponse(client, "/apix/users", ""), 404, NOT_FOUND, JSON_TYPE);
  typed(httpGetResponse(client, "/", ""), 404, NOT_FOUND, JSON_TYPE);
  typed(httpPostResponse(client, "/api/users?api-key=foo", "", ""), 404, NOT_FOUND, JSON_TYPE);
  typed(httpGetResponse(client, "/api/users", ""), 400, NO_KEY, JSON_TYPE);
  typed(httpGetResponse(client, "/api/users?api-key=", ""), 400, NO_KEY, JSON_TYPE);
  typed(httpGetResponse(client, "/api/users?api-key", ""), 400, NO_KEY, JSON_TYPE);
  typed(httpGetResponse(client, "/api/missing", ""), 400, NO_KEY, JSON_TYPE);
  typed(httpGetResponse(client, "/api/users?api-key=fo+o", ""), 401, BAD_KEY, JSON_TYPE);
  return typed(httpGetResponse(client, "/api/users?other=foo&api-key=nope", ""), 401, BAD_KEY, JSON_TYPE);
}

function problem(status, message) {
  return { status: status, message: message } satisfies HttpError;
}

/** The query value named by the path, or a 404 when the request did not send it. */
function named(req: Request, res: Response) {
  const query = req.query;
  const found = andThen(mapGet(req.params, "name"), (name) => mapGet(query, name));
  switch (found.kind) {
    case "success":
      return res.send(`[${found.value}]`);
    case "error":
      return res.status(404).send("absent");
  }
}

/**
 * An application whose error handlers pass and fail in turn: the first passes
 * a 503 on and answers the rest, the second fails again with its own error,
 * and with no handler left Express's final handler answers.
 */
function faulty() {
  return express()
    .use("/down", (req, res) => req.fail(problem(503, "down")))
    .use("/teapot", (req, res) => req.fail(problem(418, "short and stout")))
    .get("/fine", (req, res) => res.send("fine"))
    .get("/typed", (req, res) => res.type("text/markdown; charset=utf-8").status(201).send("# plain"))
    .get("/retyped", (req, res) => res.type("text/plain").type("image/svg+xml").send("<svg/>"))
    .get("/query/:name", named)
    .onError((failure, req, res) => (failure.status === 503 ? req.next() : res.status(failure.status).send(failure.message)))
    .onError((failure, req, res) => req.fail(problem(502, `${failure.message} again`)));
}

function rescued(client) {
  checked(httpGetResponse(client, "/fine", ""), 200, "fine");
  typed(httpGetResponse(client, "/fine", ""), 200, "fine", HTML);
  typed(httpGetResponse(client, "/typed", ""), 201, "# plain", "text/markdown; charset=utf-8");
  typed(httpGetResponse(client, "/retyped", ""), 200, "<svg/>", "image/svg+xml");
  checked(httpGetResponse(client, "/teapot", ""), 418, "short and stout");
  checked(httpGetResponse(client, "/teapot/and/more", ""), 418, "short and stout");
  missing(httpGetResponse(client, "/other", ""), "Cannot GET /other");
  return expect(httpGet(client, "/down", ""), Success(502));
}

/** Names, values, repeats, empty parts and escapes, as `querystring.parse` reads them. */
function queried(client) {
  const query = "a=1&&b&c=%41%7a%20x&d=e=f&a=2&g=%zz%C3%A9+h&=lost&i=";
  checked(httpGetResponse(client, `/query/a?${query}`, ""), 200, "[1]");
  checked(httpGetResponse(client, `/query/b?${query}`, ""), 200, "[]");
  checked(httpGetResponse(client, `/query/c?${query}`, ""), 200, "[Az x]");
  checked(httpGetResponse(client, `/query/d?${query}`, ""), 200, "[e=f]");
  checked(httpGetResponse(client, `/query/g?${query}`, ""), 200, "[%zz%C3%A9 h]");
  checked(httpGetResponse(client, `/query/i?${query}`, ""), 200, "[]");
  checked(httpGetResponse(client, `/query/lost?${query}`, ""), 404, "absent");
  checked(httpGetResponse(client, "/query/a", ""), 404, "absent");
  return checked(httpGetResponse(client, "/query/a?", ""), 404, "absent");
}

function faults(client) {
  rescued(client);
  return queried(client);
}

function everyRoute(client) {
  routed(client);
  return unrouted(client);
}

/** JSON bodies, a router mounted on `/tasks`, and a header read with `req.get`. */
function bodies(client) {
  const json = "Content-Type: application/json\r\n";
  const invalid = "{\"error\":\"text must be a non-empty string\"}";
  const malformed = "{\"error\":\"malformed JSON\"}";
  typed(httpGetResponse(client, "/notes", ""), 200, "[{\"id\":1,\"text\":\"buy milk\"}]", JSON_TYPE);
  typed(httpPostResponse(client, "/notes", "{\"text\":\"call mum\"}", json), 201, "{\"id\":2,\"text\":\"call mum\"}", JSON_TYPE);
  typed(httpPostResponse(client, "/notes", " [1] ", json), 422, invalid, JSON_TYPE);
  typed(httpPostResponse(client, "/notes", "{\"text\":3}", "Content-Type: Application/JSON; charset=utf-8\r\n"), 422, invalid, JSON_TYPE);
  typed(httpPostResponse(client, "/notes", "{\"text\":\"x\"}", "Content-Type: application/vnd.api+json\r\n"), 201, "{\"id\":2,\"text\":\"x\"}", JSON_TYPE);
  typed(httpPostResponse(client, "/notes", "{\"text\":", json), 400, malformed, JSON_TYPE);
  typed(httpPostResponse(client, "/notes", "true", json), 400, malformed, JSON_TYPE);
  typed(httpPostResponse(client, "/notes", "{\"text\":\"plain\"}", "Content-Type: text/plain\r\n"), 422, invalid, JSON_TYPE);
  typed(httpPostResponse(client, "/notes", "{\"text\":\"none\"}", ""), 422, invalid, JSON_TYPE);
  typed(httpGetResponse(client, "/tasks", ""), 200, "{\"tasks\":[\"write\",\"test\"]}", JSON_TYPE);
  typed(httpGetResponse(client, "/Tasks/", ""), 200, "{\"tasks\":[\"write\",\"test\"]}", JSON_TYPE);
  typed(httpGetResponse(client, "/tasks/7", "X-Agent: osprey\r\n"), 200, "{\"id\":\"7\",\"agent\":\"osprey\"}", JSON_TYPE);
  typed(httpGetResponse(client, "/tasks/7", "x-AGENT:   spaced  \r\n"), 200, "{\"id\":\"7\",\"agent\":\"spaced\"}", JSON_TYPE);
  typed(httpGetResponse(client, "/tasks/8", "X-Other: 1\r\n"), 200, "{\"id\":\"8\",\"agent\":\"unknown\"}", JSON_TYPE);
  return typed(httpGetResponse(client, "/tasks/7/extra", ""), 404, "{\"error\":\"not found\"}", JSON_TYPE);
}

/** The requests of the application named `which`. */
function exercise(which, client) {
  return which === "routes" ? everyRoute(client) : which === "web-service" ? keyed(client) : which === "json-api" ? bodies(client) : faults(client);
}

function exercised(listener, which) {
  const client = httpCreateClient(`http://127.0.0.1:${listener.port}`, 5000);
  switch (client.kind) {
    case "error":
      return check("HTTP client", "success", client.message);
    case "success": {
      exercise(which, client.value);
      return expect(httpCloseClient(client.value), Success(0));
    }
  }
}

function lifecycle(opened, which) {
  switch (opened.kind) {
    case "error":
      return check("loopback listener", "success", opened.message);
    case "success": {
      exercised(opened.value, which);
      return expect(close(opened.value.server), Success(0));
    }
  }
}


test("Express routes example over real loopback HTTP", () => lifecycle(served("routes", 0), "routes"));
test("Express web-service example over real loopback HTTP", () => lifecycle(served("web-service", 0), "web-service"));
test("Express error handlers and query strings", () => lifecycle(served("faults", 0), "faults"));
test("Express JSON bodies, a mounted router and request headers", () => lifecycle(served("json-api", 0), "json-api"));
test("Express listen on a port that cannot be bound", () => refusedOn(INVALID_PORT));
test("Express listen on a port another listener holds", occupied);

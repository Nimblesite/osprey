// osprey: namespace=routing

/**
 * The Express package suite: the routes example's application served natively
 * over real loopback HTTP, each reply checked against what Express 5.3.0 sent
 * for the same request in `examples.expected.json`. [TS-PORT-EXPRESS-ORACLE]
 */
import express, { close, type Server } from "express";
import { tasks } from "../examples/routes/tasks.ts";

/** The GC and ARC runs of this suite may overlap, so each takes the first free port. */
const PORTS = [18921, 18923, 18925, 18927, 18929];
const HTML = "text/html; charset=utf-8";
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

/** Serve the routes example on the first free port from `index` on. */
function served(index) {
  const port = listGet(PORTS, index);
  switch (port.kind) {
    case "success":
      return free(port.value) ? Success({ server: tasks().listen(port.value, () => {}), port: port.value } satisfies Listener) : served(satAdd(index, 1));
    case "error":
      return Error("no loopback test port is free");
  }
}

function checked(outcome, status, body) {
  switch (outcome.kind) {
    case "error":
      return check("HTTP transport", "success", outcome.message);
    case "success": {
      expect(httpResponseStatus(outcome.value), status);
      expect(httpResponseBody(outcome.value), Success(body));
      expect(httpResponseHeader(outcome.value, "Content-Type"), Success(HTML));
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

function exercised(listener) {
  const client = httpCreateClient(`http://127.0.0.1:${listener.port}`, 5000);
  switch (client.kind) {
    case "error":
      return check("HTTP client", "success", client.message);
    case "success": {
      routed(client.value);
      unrouted(client.value);
      return expect(httpCloseClient(client.value), Success(0));
    }
  }
}

function lifecycle(opened) {
  switch (opened.kind) {
    case "error":
      return check("loopback listener", "success", opened.message);
    case "success": {
      exercised(opened.value);
      return expect(close(opened.value.server), Success(0));
    }
  }
}

test("Express routes example over real loopback HTTP", () => lifecycle(served(0)));
test("Express listen on a port that cannot be bound", () => refusedOn(INVALID_PORT));
test("Express listen on a port another listener holds", occupied);

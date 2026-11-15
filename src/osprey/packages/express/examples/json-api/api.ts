// osprey: namespace=routing
// osprey: module=Api

/**
 * The Osprey port of `oracle/upstream/json-api/index.js`, an original
 * reference application: JSON request bodies decoded and validated, a router
 * mounted on `/tasks`, and a request header read with `req.get`. The tests
 * compare every reply with the reference running on real Express.
 * [TS-PORT-EXPRESS]
 */
import express, { jsonBody, type HttpError, type Request, type Response } from "express";

interface Note {
  readonly id: int;
  readonly text: string;
}

const notes = [{ id: 1, text: "buy milk" } satisfies Note];

/** Express's parser answers a body it cannot decode with 400. */
const MALFORMED = 400;
const INVALID = 422;

function problem(status, message) {
  return { status: status, message: message } satisfies HttpError;
}

// JSON stays at the edge: each record has one encoder.
function noteJson(note: Note) {
  return JsonObject([JsonMember("id", JsonInt(note.id)), JsonMember("text", JsonString(note.text))]);
}

function errorJson(message) {
  return JsonObject([JsonMember("error", JsonString(message))]);
}

function created(req, res) {
  const body = jsonBody(req);
  switch (body.kind) {
    case "error":
      return req.fail(problem(MALFORMED, body.message));
    case "success":
      return noted(req, res, andThen(jsonField(body.value, "text"), (text) => jsonText(text)));
  }
}

/** A note needs a non-empty string `text`; the next id follows the stored notes. */
// The annotations work around #402: the compiler cannot yet prove the effects
// of a method chain on an unannotated response.
function noted(req: Request, res: Response, text) {
  const value = unwrapOr(text, "");
  if (value === "") {
    return req.fail(problem(INVALID, "text must be a non-empty string"));
  }
  return res.status(201).json(noteJson({ id: satAdd(listLength(notes), 1), text: value } satisfies Note));
}

function taskOf(req, res) {
  const id = unwrapOr(mapGet(req.params, "id"), "");
  const agent = unwrapOr(req.get("X-Agent"), "");
  return res.json(JsonObject([JsonMember("id", JsonString(id)), JsonMember("agent", JsonString(agent === "" ? "unknown" : agent))]));
}

function taskRoutes() {
  const tasks = express.Router();
  tasks.get("/", (req, res) => res.json(JsonObject([JsonMember("tasks", JsonArray([JsonString("write"), JsonString("test")]))])));
  tasks.get("/:id", taskOf);
  return tasks;
}

/** The application. */
export function api() {
  const app = express();
  app.get("/notes", (req, res) => res.json(JsonArray(mapList(notes, noteJson))));
  app.post("/notes", created);
  app.mount("/tasks", taskRoutes());
  return app
    .onError((failure, req, res) => res.status(failure.status).json(errorJson(failure.message)))
    .use("/", (req, res) => res.status(404).json(errorJson("not found")));
}

/** Serve the application on `port`. */
export function serve(port, ready) {
  return api().listen(port, ready);
}

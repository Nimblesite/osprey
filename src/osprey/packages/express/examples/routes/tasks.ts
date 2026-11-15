// osprey: namespace=routing
// osprey: module=Tasks
/** The routes example's application, shared by the example and the package suite. */
import express from "express";

/** The routes example's application: four methods, headers and statuses. */
export function tasks() {
  const app = express();

  app.get("/", (req, res) => {
    res.send("Osprey serves these routes natively.");
  });

  app.get("/health", (req, res) => res.set("Cache-Control", "no-store").send("ok"));

  app.post("/tasks", (req, res) => {
    res.status(201);
    res.set("Location", "/tasks/current");
    res.send("Task created");
  });

  app.get("/tasks/:id", (req, res) => res.send("Task found"));

  return app
    .put("/tasks/current", (req, res) => res.send("Task replaced"))
    .delete("/tasks/current", (req, res) => res.status(410).send("Task removed"));
}

/** Serve the application on `port`. */
export function serve(port, ready) {
  return tasks().listen(port, ready);
}

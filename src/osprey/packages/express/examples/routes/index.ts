import express from "express";

const app = express();
const port = 3000;

app.get("/", (req, res) => {
  res.send("Osprey serves these routes natively.");
});

app.get("/health", (req, res) => res.set("Cache-Control", "no-store").send("ok"));

app.post("/tasks", (req, res) => {
  res.status(201);
  res.set("Location", "/tasks/current");
  res.send("Task created");
});

app
  .put("/tasks/current", (req, res) => res.send("Task replaced"))
  .delete("/tasks/current", (req, res) => res.status(410).send("Task removed"));

app.listen(port, () => {
  console.log(`Routes example listening on port ${port}`);
});

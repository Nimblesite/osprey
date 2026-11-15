// Comparison bench: each example, the same file, served by the Osprey port
// (native, GC and ARC) and by the pinned real Express on Node.js, under the
// same loopback load. Every timed reply is checked against the Express
// recording before it counts. Run from this directory after `npm ci`:
//   node bench.mjs
// Writes target/express/benchmark/results.json and prints a markdown table.
// [TS-PORT-EXPRESS-BENCH]
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFile, mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import http from "node:http";
import { createRequire } from "node:module";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { listening } from "./exchange.mjs";

const require = createRequire(import.meta.url);
const here = path.dirname(fileURLToPath(import.meta.url));
const pkg = path.dirname(here);
const root = path.resolve(pkg, "../../../..");
const osprey = process.env.OSPREY ?? path.join(root, "target/debug/osprey");
const out = path.join(root, "target/express/benchmark");
const PORT = 3000;
const CONCURRENCY = 16;
const WARMUP = 1000;
const REQUESTS = 10000;
const TRIALS = 5;
const RSS_INTERVAL_MS = 50;

const read = async (file) => JSON.parse(await readFile(path.join(pkg, "tests", file), "utf8"));
const corpus = await read("corpus.json");
const recorded = await read("examples.expected.json");

/** The requests timed for each example: the recorded ones the port routes, plus its 404. */
const WORKLOADS = {
  "hello-world": [
    ["GET", "/"],
    ["GET", "/missing"],
  ],
  routes: [
    ["GET", "/"],
    ["GET", "/health"],
    ["POST", "/tasks"],
    ["PUT", "/tasks/current"],
    ["DELETE", "/tasks/current"],
    ["GET", "/tasks"],
  ],
};

/** Each workload request with the reply Express recorded for it. */
function workload(example) {
  return WORKLOADS[example].map(([method, requested]) => {
    const found = recorded.examples[example].find(({ request }) => request.method === method && request.path === requested);
    assert.ok(found, `${example}: ${method} ${requested} is not in the recording; rerun record.mjs`);
    return found;
  });
}

const sha256 = async (file) => createHash("sha256").update(await readFile(file)).digest("hex");

async function native(example, memory) {
  const binary = path.join(out, `${example}-${memory}`);
  execFileSync(osprey, [path.join(pkg, "examples", example), "--compile", "--quiet", "--no-warnings", `--memory=${memory}`, "-o", binary], {
    stdio: ["ignore", "inherit", "inherit"],
  });
  return { name: `osprey-${memory}`, command: binary, args: [], cwd: out, hash: await sha256(binary) };
}

/** Node.js runs the example's own files beside the pinned Express, as record.mjs does. */
async function node(example) {
  const run = path.join(here, ".bench");
  await rm(run, { recursive: true, force: true });
  await mkdir(run);
  for (const file of await readdir(path.join(pkg, "examples", example))) {
    if (file.endsWith(".ts")) await copyFile(path.join(pkg, "examples", example, file), path.join(run, file));
  }
  return { name: "node-express", command: process.execPath, args: ["index.ts"], cwd: run, hash: await sha256(path.join(run, "index.ts")) };
}

/** One request on a fresh connection; resolves with its latency in microseconds once the reply checks. */
function timed(entry) {
  const { request, reply } = entry;
  const started = process.hrtime.bigint();
  return new Promise((resolve, reject) => {
    const sent = http.request({ host: "127.0.0.1", port: PORT, method: request.method, path: request.path, agent: false }, (response) => {
      const chunks = [];
      response.on("data", (chunk) => chunks.push(chunk));
      response.on("end", () => {
        try {
          assert.equal(response.statusCode, reply.status, `${request.method} ${request.path}: status`);
          assert.equal(Buffer.concat(chunks).toString("utf8"), reply.body, `${request.method} ${request.path}: body`);
          assert.equal(response.headers["content-type"], reply.headers["content-type"], `${request.method} ${request.path}: content-type`);
          resolve(Number(process.hrtime.bigint() - started) / 1000);
        } catch (error) {
          reject(error);
        }
      });
    });
    sent.on("error", reject);
    sent.end();
  });
}

/** `count` requests cycling through `entries`, `CONCURRENCY` in flight. */
async function load(entries, count) {
  const latencies = [];
  let next = 0;
  const worker = async () => {
    while (next < count) {
      const index = next;
      next += 1;
      latencies.push(await timed(entries[index % entries.length]));
    }
  };
  const started = process.hrtime.bigint();
  await Promise.all(Array.from({ length: CONCURRENCY }, worker));
  const seconds = Number(process.hrtime.bigint() - started) / 1e9;
  return { seconds, latencies };
}

function rssKiB(pid) {
  try {
    return Number(execFileSync("ps", ["-o", "rss=", "-p", String(pid)], { encoding: "utf8" }).trim());
  } catch {
    return Number.NaN;
  }
}

const median = (values) => {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
};
const nearestRank = (values, p) => [...values].sort((a, b) => a - b)[Math.max(0, Math.ceil((p / 100) * values.length) - 1)];

/** One trial: a fresh server, a warmup, then the timed requests with sampled RSS. */
async function trial(server, entries) {
  const child = spawn(server.command, server.args, { cwd: server.cwd, stdio: "ignore" });
  try {
    await listening(PORT);
    await load(entries, WARMUP);
    const samples = [];
    const sampler = setInterval(() => samples.push(rssKiB(child.pid)), RSS_INTERVAL_MS);
    const { seconds, latencies } = await load(entries, REQUESTS);
    clearInterval(sampler);
    assert.equal(child.exitCode, null, `${server.name}: the server must still be running`);
    return {
      requestsPerSecond: REQUESTS / seconds,
      medianLatencyMicros: median(latencies),
      p95LatencyMicros: nearestRank(latencies, 95),
      peakRssKiB: Math.max(...samples.filter(Number.isFinite)),
    };
  } finally {
    child.kill();
    await new Promise((resolve) => child.once("exit", resolve));
  }
}

function summarize(trials) {
  return {
    trials,
    medianRequestsPerSecond: median(trials.map((t) => t.requestsPerSecond)),
    medianOfMedianLatencyMicros: median(trials.map((t) => t.medianLatencyMicros)),
    medianP95LatencyMicros: median(trials.map((t) => t.p95LatencyMicros)),
    peakRssKiB: Math.max(...trials.map((t) => t.peakRssKiB)),
  };
}

await mkdir(out, { recursive: true });
const results = {
  spec: "[TS-PORT-EXPRESS-BENCH]",
  date: new Date().toISOString(),
  host: { cpu: os.cpus()[0]?.model, cores: os.cpus().length, memoryGiB: Math.round(os.totalmem() / 2 ** 30), platform: `${os.type()} ${os.release()} ${os.arch()}` },
  express: require("express/package.json").version,
  node: process.version,
  osprey: execFileSync(osprey, ["--version"], { encoding: "utf8" }).trim(),
  method: { concurrency: CONCURRENCY, warmup: WARMUP, requests: REQUESTS, trials: TRIALS, connection: "fresh per request", rssIntervalMs: RSS_INTERVAL_MS },
  examples: {},
};
try {
  for (const example of Object.keys(WORKLOADS)) {
    assert.ok(corpus.examples[example], `${example} is not in the corpus`);
    const entries = workload(example);
    const servers = [await native(example, "gc"), await native(example, "arc"), await node(example)];
    const trials = Object.fromEntries(servers.map((server) => [server.name, []]));
    // Trials rotate through the servers so drift on the host hits each alike.
    for (let index = 0; index < TRIALS; index += 1) {
      for (const server of servers) trials[server.name].push(await trial(server, entries));
    }
    results.examples[example] = {
      requests: entries.map(({ request }) => `${request.method} ${request.path}`),
      servers: Object.fromEntries(servers.map((server) => [server.name, { sha256: server.hash, ...summarize(trials[server.name]) }])),
    };
  }
} finally {
  await rm(path.join(here, ".bench"), { recursive: true, force: true });
}
await writeFile(path.join(out, "results.json"), `${JSON.stringify(results, null, 2)}\n`);

const rows = Object.entries(results.examples).flatMap(([example, { servers }]) =>
  Object.entries(servers).map(
    ([name, s]) =>
      `| ${example} | ${name} | ${Math.round(s.medianRequestsPerSecond)} | ${Math.round(s.medianOfMedianLatencyMicros)} | ${Math.round(s.medianP95LatencyMicros)} | ${Math.round(s.peakRssKiB / 1024)} |`,
  ),
);
console.log(["| Example | Server | Requests/s (median) | Median latency µs | p95 latency µs | Peak RSS MiB |", "| --- | --- | ---: | ---: | ---: | ---: |", ...rows].join("\n"));
console.log(`results: ${path.relative(root, path.join(out, "results.json"))}`);

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const localBin = path.join(root, "node_modules/.bin/eleventy");
const sharedBin = path.resolve(root, "../website/node_modules/.bin/eleventy");
const bin = fs.existsSync(localBin) ? localBin : sharedBin;

if (!fs.existsSync(bin)) {
  console.error("Eleventy binary not found. Run npm install.");
  process.exit(1);
}

const result = spawnSync(bin, process.argv.slice(2), {
  cwd: root,
  stdio: "inherit",
});

process.exit(result.status ?? 1);

import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const sharedPlaywrightBin = path.resolve(root, "../website/node_modules/.bin/playwright");
const sharedNodeModules = path.resolve(root, "../website/node_modules");

const result = spawnSync(sharedPlaywrightBin, ["test", "--config", path.join(root, "playwright.config.mjs")], {
  cwd: root,
  stdio: "inherit",
  env: {
    ...process.env,
    NODE_PATH: [sharedNodeModules, process.env.NODE_PATH].filter(Boolean).join(path.delimiter),
  },
});

process.exit(result.status ?? 1);

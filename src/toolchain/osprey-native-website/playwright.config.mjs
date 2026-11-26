import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { defineConfig } = require("../website/node_modules/@playwright/test");
const root = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  testDir: path.join(root, "tests"),
  timeout: 30_000,
  retries: 0,
  reporter: "list",
  use: {
    browserName: "chromium",
    headless: true,
    viewport: { width: 1440, height: 900 },
  },
});

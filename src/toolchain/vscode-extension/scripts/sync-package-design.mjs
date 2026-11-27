// Build the package workspace assets from the registry's canonical mark and design tokens.
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";

const media = new URL("../media/", import.meta.url);
const output = new URL("registry/", media);
const catalog = new URL("../../../osprey/packages/manager/web/assets/", import.meta.url);
// A Windows checkout may give the stylesheet CRLF line ends: read them as LF.
const css = readFileSync(new URL("registry.css", catalog), "utf8").replace(/\r\n/gu, "\n");
const tokens = /^:root \{\n([^}]+)\}/u.exec(css)?.[1].split("\n").filter(line => line.trimStart().startsWith("--"));
if (!tokens?.length) throw new Error("The registry stylesheet has no design tokens");
mkdirSync(output, { recursive: true });
writeFileSync(new URL("package-workspace.css", output),
  `:root {\n${tokens.join("\n")}\n}\n` + readFileSync(new URL("package-workspace.css", media), "utf8"));
copyFileSync(new URL("registry-mark.svg", catalog), new URL("registry-mark.svg", output));

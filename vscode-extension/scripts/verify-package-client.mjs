import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const os = process.platform === "win32" ? "win32" : process.platform === "darwin" ? "darwin" : "linux";
const executable = process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg";
const client = path.join(extension, "bin", `${os}-${process.arch === "arm64" ? "arm64" : "x64"}`, executable);
if (!fs.existsSync(client)) throw new Error(`Missing bundled package client: ${client}. Run make _vsix_bundle before packaging.`);

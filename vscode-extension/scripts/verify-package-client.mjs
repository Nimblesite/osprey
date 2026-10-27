import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const os = process.platform === "win32" ? "win32" : process.platform === "darwin" ? "darwin" : "linux";
const executable = process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg";
const client = path.join(extension, "bin", `${os}-${process.arch === "arm64" ? "arm64" : "x64"}`, executable);
if (!fs.existsSync(client)) throw new Error(`Missing bundled package client: ${client}. Run make _vsix_bundle before packaging.`);
const workspace = path.join(path.dirname(client), "package-workspace.wasm");
if (!fs.existsSync(workspace)) throw new Error(`Missing bundled package workspace: ${workspace}. Run make _vsix_bundle before packaging.`);
if (!WebAssembly.validate(fs.readFileSync(workspace))) throw new Error(`Invalid package workspace WASM: ${workspace}`);
// The local package registry [PACKAGE-LOCAL-REGISTRY]: the server and the staged
// packages it serves. Not bundled on Windows yet (its tree walk needs POSIX find).
if (process.platform !== "win32") {
  const server = path.join(path.dirname(client), "osprey-registry");
  if (!fs.existsSync(server)) throw new Error(`Missing bundled local registry server: ${server}. Run make _vsix_bundle before packaging.`);
  const packages = path.join(path.dirname(client), "registry", "@osprey");
  if (!fs.existsSync(packages) || fs.readdirSync(packages).length === 0) throw new Error(`Missing bundled local registry packages: ${packages}. Run make _vsix_bundle before packaging.`);
}

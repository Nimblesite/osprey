import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
for (const name of ["wasi", "wasi-runner", "wasi-worker"]) {
  const file = path.join(extension, "out/client/src/osprey-ui", `${name}.js`);
  if (!fs.existsSync(file) || fs.statSync(file).size === 0) throw new Error(`Missing bundled WASI runtime module: ${file}`);
}
const os = process.platform === "win32" ? "win32" : process.platform === "darwin" ? "darwin" : "linux";
const executable = process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg";
const client = path.join(extension, "bin", `${os}-${process.arch === "arm64" ? "arm64" : "x64"}`, executable);
if (!fs.existsSync(client)) throw new Error(`Missing bundled package client: ${client}. Run make _vsix_bundle before packaging.`);
for (const name of ["package-workspace", "workbench"]) {
  const artifact = path.join(path.dirname(client), `${name}.wasm`);
  const label = name.replaceAll("-", " ");
  if (!fs.existsSync(artifact)) throw new Error(`Missing bundled ${label}: ${artifact}. Run make _vsix_bundle before packaging.`);
  const bytes = fs.readFileSync(artifact);
  if (!WebAssembly.validate(bytes)) throw new Error(`Invalid ${label} WASM: ${artifact}`);
  const exports = WebAssembly.Module.exports(new WebAssembly.Module(bytes));
  if (!exports.some(item => item.name === "_start" && item.kind === "function")
    || !exports.some(item => item.name === "memory" && item.kind === "memory")) {
    throw new Error(`Missing WASI application exports in ${name}: ${artifact}`);
  }
}
const assets = path.join(path.dirname(client), "workbench");
const lessons = path.join(assets, "lessons");
function bundledFile(source) {
  const content = fs.readFileSync(path.join(assets, source));
  if (content.length === 0 || content.subarray(0, 48).toString().startsWith("version https://git-lfs.github.com/spec/v1")) {
    throw new Error(`Missing binary content for workbench asset: ${source}`);
  }
  return content;
}
function relativeAsset(source) {
  return typeof source === "string" && !path.isAbsolute(source) && !/[\\:?#]/.test(source)
    && source.split("/").every(part => part !== ".." && part !== "." && part !== "");
}
for (const source of ["osprey-logo.png", "chalk-study.png", "fonts/Geist-LICENSE.txt", "fonts/STIX-Two-Text-LICENSE.txt", "fonts/STIX-Two-Math-LICENSE.txt"]) bundledFile(source);
for (const source of ["workbench.css", "workbench-fonts.css"]) {
  const css = bundledFile(source).toString("utf8");
  for (const match of css.matchAll(/url\(["']?([^\s)"']+)["']?\)/g)) {
    const file = match[1];
    if (!relativeAsset(file)) {
      throw new Error(`Invalid workbench stylesheet asset: ${file}`);
    }
    bundledFile(file);
  }
}
function verifyProofLibraries() {
  const catalogue = JSON.parse(bundledFile("proof-libraries.json").toString("utf8"));
  if (!Array.isArray(catalogue.libraries) || catalogue.libraries.length === 0) throw new Error("Invalid workbench proof library inventory");
  const seen = new Set();
  for (const entry of catalogue.libraries) {
    if (!entry || typeof entry.title !== "string" || entry.title.trim() === "" || !relativeAsset(entry.path)
      || !entry.path.startsWith("proof-libraries/") || seen.has(entry.path)) throw new Error("Invalid workbench proof library entry");
    seen.add(entry.path);
    bundledFile(entry.path);
  }
  for (const name of ["LICENSE.lean4", "LICENSE.openai-math"]) bundledFile(`proof-libraries/${name}`);
}
verifyProofLibraries();
function verifySymbols() {
  const symbols = JSON.parse(bundledFile("symbols.json").toString("utf8"));
  if (!symbols || Array.isArray(symbols) || typeof symbols !== "object" || Object.keys(symbols).length === 0) throw new Error("Invalid workbench symbol inventory");
  for (const source of Object.values(symbols)) {
    if (!relativeAsset(source) || !/^(icons|symbols)\/.+\.svg$/.test(source)) throw new Error("Invalid workbench symbol asset");
    if (!bundledFile(source).toString("utf8").startsWith("<svg ")) throw new Error(`Invalid symbol SVG: ${source}`);
  }
  bundledFile("icons/LICENSE.txt");
}
verifySymbols();
const catalogue = JSON.parse(fs.readFileSync(path.join(lessons, "catalogue.json"), "utf8"));
if (catalogue.format !== 1 || !Array.isArray(catalogue.lessons) || catalogue.lessons.length === 0) {
  throw new Error("Invalid bundled workbench catalogue");
}
function verifyAsset(source, slug) {
  if (!relativeAsset(source) || !source.startsWith(`media/${slug}/`)) {
    throw new Error(`Invalid workbench media path: ${source}`);
  }
  bundledFile(`lessons/${source}`);
}
for (const slug of catalogue.lessons) {
  if (typeof slug !== "string" || !/^[a-z0-9-]+$/.test(slug)) throw new Error("Invalid workbench lesson slug");
  const lesson = JSON.parse(fs.readFileSync(path.join(lessons, `${slug}.json`), "utf8"));
  if (lesson.slug !== slug || !Array.isArray(lesson.blocks)) throw new Error(`Invalid bundled workbench lesson: ${slug}`);
  for (const block of lesson.blocks) {
    if (block.kind === "image") verifyAsset(block.source, slug);
    if (block.kind === "video") for (const key of ["source", "poster", "captions"]) verifyAsset(block[key], slug);
  }
}

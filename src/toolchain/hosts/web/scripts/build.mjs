// Builds an Osprey Native application for the browser ([NATIVE-HOSTS-FAMILY]): the application is
// compiled to wasm32 and inlined into one script beside the shared renderer, its stylesheet and a
// page, all static files any web server can serve (serve.mjs does, on this machine).
//   node scripts/build.mjs --app <folder> --title <title> --out <folder> [--argument <launch argument>]
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "esbuild";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../../../../..");
const extension = path.resolve(repo, "src/toolchain/vscode-extension");

function option(name, fallback) {
  const at = process.argv.indexOf(name);
  if (at >= 0 && process.argv[at + 1]) return process.argv[at + 1];
  if (fallback === undefined) throw new Error(`build.mjs needs ${name}`);
  return fallback;
}

function compiled(app, out) {
  const wasm = path.join(out, "app.wasm");
  const compiler = process.env.OSPREY_BIN ?? path.join(repo, "target/release/osprey");
  execFileSync(compiler, [app, "--compile", "--target=wasm32", "--quiet", "--no-warnings", "-o", wasm], { stdio: "inherit" });
  return readFileSync(wasm).toString("base64");
}

// Node's `fs` is the Codicon loader's alone, and the page takes its styles prebuilt.
const noFs = { name: "no-fs", setup(on) {
  on.onResolve({ filter: /^fs$/ }, () => ({ path: "fs", namespace: "no-fs" }));
  on.onLoad({ filter: /.*/, namespace: "no-fs" }, () => ({ contents: "export const readFileSync = () => { throw new Error('no files in the browser'); };" }));
} };

async function styles(out) {
  const script = path.join(out, "styles.cjs");
  await build({ entryPoints: [path.join(here, "styles.ts")], bundle: true, platform: "node", format: "cjs", outfile: script, logLevel: "warning" });
  return execFileSync(process.execPath, [script], { env: { ...process.env, NODE_PATH: path.join(extension, "node_modules") }, maxBuffer: 1 << 26 }).toString();
}

async function script(wasm) {
  const result = await build({ entryPoints: [path.join(here, "../src/main.ts")], bundle: true, format: "iife", target: "es2022",
    minify: true, write: false, plugins: [noFs], define: { OSPREY_APP_WASM: JSON.stringify(wasm) }, logLevel: "warning" });
  return result.outputFiles[0].text;
}

// [UI-COMPONENTS-FRAMED] The typefaces an application bundles in assets/fonts travel inside its
// stylesheet, each named by its file, so a frame's `frameFont = "Inter"` draws fonts/Inter.ttf.
function faces(app) {
  const folder = path.join(app, "assets", "fonts");
  if (!existsSync(folder)) return "";
  return readdirSync(folder).filter((name) => /^[A-Z][A-Za-z0-9-]{0,63}\.(ttf|otf)$/.test(name)).sort().map((name) => {
    const [family, kind] = name.split(".");
    const data = readFileSync(path.join(folder, name)).toString("base64");
    return `@font-face{font-family:"${family}";src:url(data:font/${kind};base64,${data}) format("${kind === "otf" ? "opentype" : "truetype"}");font-weight:100 900;font-display:block}`;
  }).join("");
}

// The `<html>` attribute that hands the application its first launch argument (bridge.ts reads it;
// without one the application gets the page's own origin).
function launched(argument) {
  return argument === "" ? "" : ` data-osprey-argument="${argument.replaceAll("&", "&amp;").replaceAll('"', "&quot;").replaceAll("<", "&lt;")}"`;
}

function page(title, argument) {
  return `<!doctype html><html lang="en"${launched(argument)}><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover"><title>${title}</title><link rel="stylesheet" href="host.css"><script defer src="host.js"></script></head><body><div class="osp-shell"><div id="osprey-diagnostics" hidden></div><div id="osprey-runtime-status" role="status" hidden>Updating · previous results shown</div><div id="osprey-root"><div class="osp-loading"><span class="osp-spinner" aria-hidden="true"></span><p>Loading ${title}…</p></div></div></div></body></html>`;
}

const app = path.resolve(option("--app"));
const out = path.resolve(option("--out"));
const title = option("--title", "Osprey");
mkdirSync(out, { recursive: true });
execFileSync(process.execPath, [path.join(extension, "scripts/generate-native-abi.mjs")], { stdio: "inherit" });
const files = { page: page(title, option("--argument", "")), script: await script(compiled(app, out)), style: faces(app) + await styles(out) };
writeFileSync(path.join(out, "index.html"), files.page);
writeFileSync(path.join(out, "host.js"), files.script);
writeFileSync(path.join(out, "host.css"), files.style);
console.log(`[web] ${title}: ${out}`);

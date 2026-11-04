import { access, mkdir, stat } from "node:fs/promises";
import { constants } from "node:fs";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { build } from "esbuild";
import { embed } from "./embed.mjs";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const packageRoot = path.resolve(scriptDirectory, "..");
const repoRoot = path.resolve(packageRoot, "../../../..");
const clientRoot = path.resolve(packageRoot, "../client");
const defaultWasm = path.join(packageRoot, "build/talon-client.wasm");
const outputModule = path.resolve(packageRoot, "../src/web/bundle.ospml");
const hostOnly = process.argv.includes("--host-only");
const noCompile = process.argv.includes("--no-compile");

function argumentValue(name, fallback) {
  const index = process.argv.indexOf(name);
  return index >= 0 && process.argv[index + 1] ? process.argv[index + 1] : fallback;
}

async function exists(filename) {
  try {
    await access(filename, constants.F_OK);
    return true;
  } catch {
    return false;
  }
}

async function findCompiler() {
  const candidates = [
    process.env.OSPREY_BIN,
    path.join(repoRoot, "target/release/osprey"),
    path.join(repoRoot, "bin/osprey"),
  ].filter(Boolean);
  for (const candidate of candidates) {
    if (await exists(candidate)) return candidate;
  }
  return null;
}

async function firstTool(candidates) {
  for (const candidate of candidates.filter(Boolean)) {
    if (await exists(candidate)) return candidate;
  }
  return null;
}

async function wasmEnvironment() {
  const env = { ...process.env };
  env.OSPREY_WASM_CC ??= await firstTool([
    "/opt/homebrew/opt/llvm/bin/clang",
    "/usr/local/opt/llvm/bin/clang",
  ]);
  env.OSPREY_WASM_LD ??= await firstTool([
    "/opt/homebrew/opt/lld/bin/wasm-ld",
    "/usr/local/opt/lld/bin/wasm-ld",
    "/opt/homebrew/opt/llvm/bin/wasm-ld",
  ]);
  return Object.fromEntries(Object.entries(env).filter(([, value]) => value != null));
}

async function shouldReuseWasm(wasmPath) {
  if (!noCompile && !process.env.OSPREY_WEB_WASM) return false;
  if (!(await exists(wasmPath))) {
    throw new Error(`WebAssembly input does not exist: ${wasmPath}`);
  }
  return true;
}

async function clientCompiler(wasmPath) {
  const compiler = await findCompiler();
  const entry = path.join(clientRoot, "src/main.ospml");
  if (compiler && (await exists(entry))) return compiler;
  if (await exists(wasmPath)) {
    console.warn("Osprey compiler/client entry unavailable; embedding the existing wasm build");
    return null;
  }
  throw new Error(
    `Cannot compile ${clientRoot}. Build target/release/osprey and ensure client/src/main.ospml exists.`,
  );
}

async function runClientCompiler(compiler, wasmPath) {
  await mkdir(path.dirname(wasmPath), { recursive: true });
  const options = {
    cwd: repoRoot, encoding: "utf8", stdio: "pipe", env: await wasmEnvironment(),
  };
  return spawnSync(compiler, [clientRoot, "--target=wasm32", "--compile", "-o", wasmPath], options);
}

function reportCompilation(result) {
  if (result.status !== 0) {
    process.stderr.write(result.stdout ?? "");
    process.stderr.write(result.stderr ?? "");
    throw new Error(`Osprey client compilation failed with exit code ${result.status}`);
  }
  process.stdout.write(result.stdout ?? "");
}

async function compileClient(wasmPath) {
  if (await shouldReuseWasm(wasmPath)) return;
  const compiler = await clientCompiler(wasmPath);
  if (!compiler) return;
  reportCompilation(await runClientCompiler(compiler, wasmPath));
}

await mkdir(path.join(packageRoot, "dist"), { recursive: true });
await build({
  entryPoints: [path.join(packageRoot, "src/index.jsx")],
  outfile: path.join(packageRoot, "dist/host.js"),
  bundle: true,
  minify: true,
  sourcemap: false,
  legalComments: "none",
  platform: "browser",
  target: ["es2022"],
  // Generated JavaScript is embedded in an Osprey string literal. Osprey also
  // spells interpolation `${...}`, so lower JS template literals to ordinary
  // concatenation and keep the generated module unambiguous.
  supported: { "template-literal": false },
  jsx: "automatic",
  logLevel: "info",
});

if (!hostOnly) {
  const wasmPath = path.resolve(
    argumentValue("--wasm", process.env.OSPREY_WEB_WASM || defaultWasm),
  );
  await compileClient(wasmPath);
  const result = await embed({ wasmPath, outputPath: outputModule });
  const outputSize = (await stat(result.outputPath)).size;
  console.log(
    `generated ${path.relative(repoRoot, result.outputPath)} (${outputSize} B; ` +
      `${result.wasmBytes} B wasm embedded)`,
  );
}

import * as assert from "node:assert/strict";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { spawnSync } from "node:child_process";
import { createAssetResolver } from "../../client/src/osprey-ui/local-files";

suite("native approved local assets", () => {
  let temporary: string;
  let root: string;
  setup(() => {
    temporary = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-assets-"));
    root = path.join(temporary, "lessons");
    fs.mkdirSync(path.join(root, "media"), { recursive: true });
    fs.writeFileSync(path.join(root, "media", "proof.vtt"), "WEBVTT\n");
    fs.writeFileSync(path.join(temporary, "private.txt"), "private");
    fs.mkdirSync(path.join(root, "..drafts"), { recursive: true });
    fs.writeFileSync(path.join(root, "..notes.txt"), "notes");
    fs.writeFileSync(path.join(root, "..drafts", "file.vtt"), "WEBVTT\n");
    fs.symlinkSync(path.join(temporary, "private.txt"), path.join(root, "escape.vtt"));
  });
  teardown(() => fs.rmSync(temporary, { recursive: true, force: true }));

  test("only existing files below the approved real root reach the URI mapper", () => {
    const mapped: string[] = [];
    const resolve = createAssetResolver(root, (file) => { mapped.push(file); return `approved:${file}`; });
    const expected = fs.realpathSync(path.join(root, "media", "proof.vtt"));
    assert.equal(resolve("media/proof.vtt"), `approved:${expected}`);
    assert.deepEqual(mapped, [expected]);
    assert.throws(() => resolve("media"), /file/i);
    assert.throws(() => resolve("missing.vtt"), /ENOENT/);
  });

  test("ordinary names beginning with two dots are inside the approved root", () => {
    const mapped: string[] = [];
    const resolve = createAssetResolver(root, (file) => { mapped.push(file); return `approved:${file}`; });
    const notes = fs.realpathSync(path.join(root, "..notes.txt"));
    const draft = fs.realpathSync(path.join(root, "..drafts", "file.vtt"));
    assert.equal(resolve("..notes.txt"), `approved:${notes}`);
    assert.equal(resolve("..drafts/file.vtt"), `approved:${draft}`);
    assert.deepEqual(mapped, [notes, draft]);
  });

  test("absolute paths traversal URL syntax and escaping symlinks are refused", () => {
    const resolve = createAssetResolver(root, () => { assert.fail("unapproved file reached mapper"); });
    for (const source of ["", "/private.txt", "../private.txt", "media/../../private.txt", "media/./proof.vtt", "media//proof.vtt", "C:\\private.txt", "https://a.test/a", "media/proof.vtt?x=1", "media/proof.vtt#x", "media/proof.vtt\0", "escape.vtt"]) {
      assert.throws(() => resolve(source), /asset|outside/i, source);
    }
  });
});

suite("workbench bundle completeness", () => {
  let root: string;
  let assets: string;
  const extension = path.resolve(__dirname, "../../..");
  const verify = () => spawnSync(process.execPath, [path.join(root, "scripts/verify-package-client.mjs")],
    { encoding: "utf8", env: { ...process.env, ELECTRON_RUN_AS_NODE: "1" } });
  const registryCheck = (part: string): void => {
    const result = verify(), required = process.platform !== "win32";
    assert.strictEqual(result.status, required ? 1 : 0, result.stderr);
    if (required) assert.ok(result.stderr.includes(`Missing bundled local registry ${part}`), result.stderr);
  };
  setup(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-bundle-"));
    const bundle = path.join(root, "bin", `${process.platform}-${process.arch}`);
    assets = path.join(bundle, "workbench");
    fs.mkdirSync(path.join(root, "scripts"), { recursive: true });
    fs.cpSync(path.join(extension, "scripts/verify-package-client.mjs"), path.join(root, "scripts/verify-package-client.mjs"));
    fs.cpSync(path.join(extension, "out/client/src/osprey-ui"), path.join(root, "out/client/src/osprey-ui"), { recursive: true });
    fs.cpSync(path.join(extension, "../apps/workbench/assets"), assets, { recursive: true });
    fs.cpSync(path.join(extension, "../apps/workbench/lessons"), path.join(assets, "lessons"), { recursive: true });
    fs.copyFileSync(path.join(extension, "icon.png"), path.join(assets, "osprey-logo.png"));
    fs.mkdirSync(path.join(assets, "proof-libraries"));
    fs.writeFileSync(path.join(assets, "proof-libraries.json"), JSON.stringify({ libraries: [{ title: "Fixture proof", path: "proof-libraries/fixture.ndjson" }] }));
    for (const name of ["fixture.ndjson", "LICENSE.lean4", "LICENSE.openai-math"]) fs.writeFileSync(path.join(assets, "proof-libraries", name), "fixture\n");
    fs.writeFileSync(path.join(bundle, process.platform === "win32" ? "osprey-pkg.exe" : "osprey-pkg"), "fixture");
    fs.writeFileSync(path.join(bundle, process.platform === "win32" ? "osprey-registry.exe" : "osprey-registry"), "fixture");
    fs.mkdirSync(path.join(bundle, "registry", "@osprey", "testing"), { recursive: true });
    fs.writeFileSync(path.join(bundle, "registry", "@osprey", "testing", "osprey.toml"), 'name = "@osprey/testing"\n');
    for (const name of ["workbench", "package-workspace"]) fs.copyFileSync(path.join(extension, "bin", `${process.platform}-${process.arch}`, `${name}.wasm`), path.join(bundle, `${name}.wasm`));
  });
  teardown(() => fs.rmSync(root, { recursive: true, force: true }));

  test("prepublish compiles WASI modules before verifying a clean output directory", function () {
    this.timeout(60_000);
    const { scripts } = JSON.parse(fs.readFileSync(path.join(extension, "package.json"), "utf8"));
    for (const name of ["client", "test", "node_modules"]) fs.symlinkSync(path.join(extension, name), path.join(root, name), "junction");
    fs.copyFileSync(path.join(extension, "tsconfig.json"), path.join(root, "tsconfig.json"));
    fs.copyFileSync(path.join(extension, "scripts/verify-package-workspace.mjs"), path.join(root, "scripts/verify-package-workspace.mjs"));
    fs.writeFileSync(path.join(root, "package.json"), JSON.stringify({ scripts: {
      "vscode:prepublish": scripts["vscode:prepublish"], "sync-icon": "node --eval 0", "compile": scripts.compile,
    } }));
    fs.rmSync(path.join(root, "out"), { recursive: true });
    assert.ok(!fs.existsSync(path.join(root, "out")), "regression must start without compiled output");
    const result = spawnSync(process.platform === "win32" ? "npm.cmd" : "npm", ["run", "vscode:prepublish"],
      { cwd: root, encoding: "utf8", shell: process.platform === "win32", env: { ...process.env, ELECTRON_RUN_AS_NODE: "1" } });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.ok(result.stdout.includes("Package workspace WASM startup: PASS"), result.stdout);
    for (const name of ["wasi", "wasi-runner", "wasi-worker"]) assert.ok(fs.statSync(path.join(root, "out/client/src/osprey-ui", `${name}.js`)).size > 0);
  });

  test("packaging enforces the host's local-registry server and package inventory", () => {
    const bundle = path.dirname(assets), server = path.join(bundle, process.platform === "win32" ? "osprey-registry.exe" : "osprey-registry");
    fs.rmSync(server);
    registryCheck("server");
    fs.writeFileSync(server, "fixture");
    const packages = path.join(bundle, "registry", "@osprey");
    fs.rmSync(packages, { recursive: true });
    registryCheck("packages");
    fs.mkdirSync(packages);
    registryCheck("packages");
  });

  test("packaging rejects a missing WASI worker runtime module", () => {
    assert.equal(verify().status, 0, "complete bundle must pass");
    for (const name of ["wasi", "wasi-runner", "wasi-worker"]) {
      const file = path.join(root, "out/client/src/osprey-ui", `${name}.js`), original = fs.readFileSync(file);
      try {
        fs.rmSync(file);
        const result = verify();
        assert.notEqual(result.status, 0, `packaging accepted missing ${name}`);
        assert.ok(result.stderr.includes(`${name}.js`), result.stderr);
        fs.writeFileSync(file, "");
        const empty = verify();
        assert.notEqual(empty.status, 0, `packaging accepted empty ${name}`);
        assert.ok(empty.stderr.includes(`${name}.js`), empty.stderr);
      } finally { fs.writeFileSync(file, original); }
    }
  });

  test("packaging rejects missing workbench styles fonts licences and logo", () => {
    assert.equal(verify().status, 0, "complete bundle must pass");
    for (const name of ["symbols.json", "icons/moon.svg", "icons/LICENSE.txt", "workbench.css", "workbench-fonts.css", "fonts/STIXTwoMath-400-normal-0.woff2", "fonts/Geist-LICENSE.txt", "osprey-logo.png", "chalk-study.png", "proof-libraries.json", "proof-libraries/fixture.ndjson", "proof-libraries/LICENSE.lean4", "proof-libraries/LICENSE.openai-math"]) {
      const file = path.join(assets, name), original = fs.readFileSync(file);
      try {
        fs.rmSync(file);
        const result = verify();
        assert.notEqual(result.status, 0, `packaging accepted missing ${name}`);
        assert.ok(result.stderr.includes(name), result.stderr);
      } finally { fs.writeFileSync(file, original); }
    }
  });

  test("packaging rejects empty or LFS-pointer assets and stylesheet traversal", () => {
    const file = path.join(assets, "fonts/STIXTwoMath-400-normal-0.woff2");
    const original = fs.readFileSync(file);
    for (const content of ["", "version https://git-lfs.github.com/spec/v1\noid sha256:unavailable\n"]) {
      try {
        fs.writeFileSync(file, content);
        const result = verify();
        assert.notEqual(result.status, 0);
        assert.ok(result.stderr.includes("STIXTwoMath-400-normal-0.woff2"), result.stderr);
      } finally { fs.writeFileSync(file, original); }
    }
    fs.appendFileSync(path.join(assets, "workbench.css"), '\n@import url("../private.css");');
    const refused = verify();
    assert.notEqual(refused.status, 0);
    assert.ok(refused.stderr.includes("Invalid workbench stylesheet asset"), refused.stderr);
  });

  test("packaging rejects a valid WASM module without a WASI application entry", () => {
    fs.writeFileSync(path.join(assets, "../workbench.wasm"), Buffer.from([0, 97, 115, 109, 1, 0, 0, 0]));
    const result = verify();
    assert.notEqual(result.status, 0, "empty WASM module was accepted as an application");
    assert.ok(result.stderr.includes("WASI application exports"), result.stderr);
  });

  test("packaging refuses empty or escaping proof library inventories", () => {
    for (const libraries of [[], [{ title: "Proof", path: "proof-libraries/../private.ndjson" }], [{ title: "", path: "proof-libraries/fixture.ndjson" }]]) {
      fs.writeFileSync(path.join(assets, "proof-libraries.json"), JSON.stringify({ libraries }));
      const result = verify();
      assert.notEqual(result.status, 0, "invalid proof inventory was accepted");
      assert.ok(result.stderr.includes("proof librar"), result.stderr);
    }
  });
});

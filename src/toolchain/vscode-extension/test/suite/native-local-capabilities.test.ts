// Implements [LESSONS-FILES] and [LESSONS-PROGRESS] host-glue regressions:
// the local executor reads only approved bundled assets, keeps progress
// inside one rooted store directory, and refuses every other request
// explicitly. Real files, real symlinks, real bounds; fixtures are cleaned.
import * as assert from "assert";
import fs = require("fs");
import type { ReadPosition, ReadSyncOptions } from "fs";
import * as os from "os";
import * as path from "path";
import { createLocalExecutor } from "../../client/src/osprey-ui/local-capabilities";
import type { NativeRequest } from "../../client/src/osprey-ui/protocol";

const catalogue = JSON.stringify({ format: 1, slugs: ["sines"] });
const progress = "{\"completed\":[\"sines/one\"]}";
const BOUND = 8 * 1024 * 1024;

interface FileEntry { readonly path: string; readonly text: string; readonly error: string; }
interface Response { readonly result: unknown; readonly slices?: readonly { key: string; text: string }[]; }

function ask(kind: string, members: Record<string, unknown>): NativeRequest {
  return { ticket: "lessons", kind, ...members };
}

function held(text: string): Map<string, string> {
  return new Map([["lessons:progress", text]]);
}

function filesOf(response: Response): FileEntry[] {
  assert.ok(recordHas(response.result, "files"));
  return (response.result as { files: FileEntry[] }).files;
}

function recordHas(value: unknown, key: string): boolean {
  return typeof value === "object" && value !== null && key in value;
}

/**
 * Fault injection: hand back at most `maximum` bytes per read. A short read is
 * a legal result for a real file, so the production reader must not assume that
 * one call returns everything it asked for. `real` is captured so the patched
 * function never calls itself.
 */
function limitedReads(maximum: number, real: typeof fs.readSync): typeof fs.readSync {
  function limited(fd: number, buffer: NodeJS.ArrayBufferView, offset: number, length: number, position: ReadPosition | null): number;
  function limited(fd: number, buffer: NodeJS.ArrayBufferView, opts?: ReadSyncOptions): number;
  function limited(fd: number, buffer: NodeJS.ArrayBufferView, offsetOrOptions?: number | ReadSyncOptions | null, length = 0, position: ReadPosition | null = null): number {
    const positioned = typeof offsetOrOptions === "number";
    const offset = positioned ? offsetOrOptions : offsetOrOptions?.offset ?? 0;
    const wanted = positioned ? length : offsetOrOptions?.length ?? buffer.byteLength - offset;
    const at = positioned ? position : offsetOrOptions?.position ?? null;
    return real(fd, buffer, offset, Math.min(wanted, maximum), at);
  }
  return limited;
}

suite("native local capabilities", () => {
  let root: string;
  let assets: string;
  let store: string;
  let outside: string;
  let execute: ReturnType<typeof createLocalExecutor>;

  suiteSetup(() => {
    root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-local-capabilities-"));
    assets = path.join(root, "assets");
    store = path.join(root, "store");
    outside = path.join(root, "outside");
    fs.mkdirSync(path.join(assets, "lessons"), { recursive: true });
    fs.mkdirSync(outside, { recursive: true });
    fs.writeFileSync(path.join(assets, "lessons", "catalogue.json"), catalogue);
    execute = createLocalExecutor({ assetRoot: assets, storeRoot: store });
  });

  suiteTeardown(() => {
    fs.rmSync(root, { recursive: true, force: true });
  });

  test("read-texts returns approved bundled files and delivers the files slice", async () => {
    const response = await execute(ask("read-texts", {
      paths: ["lessons/catalogue.json"], limit: 65536, slice: "lessons:texts",
    }), new Map()) as Response;
    assert.deepStrictEqual(filesOf(response), [{ path: "lessons/catalogue.json", text: catalogue, error: "" }]);
    const slice = response.slices?.find((item) => item.key === "lessons:texts");
    assert.ok(slice);
    assert.deepStrictEqual(JSON.parse(slice.text), { files: filesOf(response) });
  });

  test("read-texts reports refused assets per file and keeps the readable ones", async () => {
    fs.writeFileSync(path.join(outside, "secret.json"), "{}");
    fs.symlinkSync(path.join(outside, "secret.json"), path.join(assets, "lessons", "linked.json"));
    const response = await execute(ask("read-texts", {
      paths: ["../secret.json", "lessons/linked.json", "lessons/missing.json", "/etc/hosts", "lessons/catalogue.json"],
      limit: 65536, slice: "lessons:texts",
    }), new Map()) as Response;
    const files = filesOf(response);
    for (const refused of files.slice(0, 4)) {
      assert.strictEqual(refused.text, "");
      assert.ok(refused.error !== "");
    }
    assert.deepStrictEqual(files[4], { path: "lessons/catalogue.json", text: catalogue, error: "" });
  });

  test("read-texts honours the requested byte bound exactly", async () => {
    fs.writeFileSync(path.join(assets, "lessons", "small.json"), "ab");
    const exact = await execute(ask("read-texts", { paths: ["lessons/small.json"], limit: 2, slice: "s" }), new Map()) as Response;
    assert.deepStrictEqual(filesOf(exact), [{ path: "lessons/small.json", text: "ab", error: "" }]);
    const refused = await execute(ask("read-texts", { paths: ["lessons/catalogue.json"], limit: 2, slice: "s" }), new Map()) as Response;
    assert.strictEqual(filesOf(refused)[0].text, "");
    assert.ok(filesOf(refused)[0].error.includes("bound"));
  });

  test("store-write persists a host-held slice and store-read reads it back", async () => {
    const written = await execute(ask("store-write", { name: "lessons/progress.json", slice: "lessons:progress" }), held(progress)) as Response;
    assert.deepStrictEqual(written.result, { path: "lessons/progress.json" });
    const saved = path.join(store, "lessons", "progress.json");
    assert.strictEqual(fs.readFileSync(saved, "utf8"), progress);
    assert.deepStrictEqual(fs.readdirSync(store), ["lessons"]);
    if (process.platform !== "win32") {
      assert.strictEqual(fs.statSync(path.join(store, "lessons")).mode & 0o777, 0o700);
      assert.strictEqual(fs.statSync(saved).mode & 0o777, 0o600);
    }
    const read = await execute(ask("store-read", { name: "lessons/progress.json", limit: 65536, slice: "lessons:progress" }), new Map()) as Response;
    assert.deepStrictEqual(read.result, { found: true });
    assert.strictEqual(read.slices?.find((item) => item.key === "lessons:progress")?.text, progress);
  });

  test("store-read reports a missing file as not found without a slice delivery", async () => {
    const read = await execute(ask("store-read", { name: "lessons/absent.json", limit: 65536, slice: "lessons:progress" }), new Map()) as Response;
    assert.deepStrictEqual(read.result, { found: false });
    assert.deepStrictEqual(read.slices, []);
  });

  test("store-read refuses a file beyond its byte bound", async () => {
    await assert.rejects(
      execute(ask("store-read", { name: "lessons/progress.json", limit: 4, slice: "lessons:progress" }), new Map()),
      /bound/,
    );
  });

  test("store names never escape the rooted progress directory", async () => {
    for (const name of ["../escape.json", "a/../escape.json", "/etc/passwd", "a//b.json", ".", "lessons/../../escape.json", ""]) {
      await assert.rejects(execute(ask("store-write", { name, slice: "lessons:progress" }), held(progress)), /store name/i);
      await assert.rejects(execute(ask("store-read", { name, limit: 65536, slice: "s" }), new Map()), /store name/i);
    }
    assert.ok(!fs.existsSync(path.join(root, "escape.json")));
  });

  test("symlinked store paths are refused, not followed", async () => {
    const secret = path.join(outside, "progress.json");
    fs.writeFileSync(secret, "escaped");
    fs.symlinkSync(secret, path.join(store, "link.json"));
    fs.symlinkSync(outside, path.join(store, "linked-dir"));
    await assert.rejects(execute(ask("store-read", { name: "link.json", limit: 65536, slice: "s" }), new Map()), /progress directory/);
    await assert.rejects(execute(ask("store-write", { name: "link.json", slice: "lessons:progress" }), held(progress)), /progress directory/);
    await assert.rejects(execute(ask("store-write", { name: "linked-dir/progress.json", slice: "lessons:progress" }), held(progress)), /progress directory/);
    assert.strictEqual(fs.readFileSync(secret, "utf8"), "escaped");
  });

  test("store-write demands the host-held slice it is asked to persist", async () => {
    await assert.rejects(execute(ask("store-write", { name: "lessons/absent.json", slice: "lessons:progress" }), new Map()), /host slice/);
    await assert.rejects(execute(ask("store-write", { name: "lessons/absent.json", slice: "" }), held(progress)), /slice/);
    assert.ok(!fs.existsSync(path.join(store, "lessons", "absent.json")));
  });

  test("store-write refuses an over-bound host slice", async () => {
    const huge = new Map([["lessons:progress", "x".repeat(BOUND + 1)]]);
    await assert.rejects(execute(ask("store-write", { name: "lessons/huge.json", slice: "lessons:progress" }), huge), /bound/);
  });

  test("unsupported requests fail explicitly", async () => {
    for (const kind of ["http", "run", "reveal", "recognise", "store-list", "store-delete", "fingerprint", "host-call", "choose-file"]) {
      await assert.rejects(execute(ask(kind, {}), new Map()), new RegExp(`Unsupported local capability: ${kind}`));
    }
  });

  test("store refuses symlink ancestors when intermediate directories do not exist", async () => {
    fs.symlinkSync(outside, path.join(store, "nested-link"));
    const name = "nested-link/new/progress.json";
    await assert.rejects(execute(ask("store-write", { name, slice: "lessons:progress" }), held(progress)), /progress directory/);
    assert.ok(!fs.existsSync(path.join(outside, "new")));
  });

  test("store-write bounds UTF8 bytes rather than UTF16 characters", async () => {
    await assert.rejects(execute(ask("store-write", { name: "utf8.json", slice: "lessons:progress" }), held("∧".repeat(BOUND / 2))), /bound/);
    assert.ok(!fs.existsSync(path.join(store, "utf8.json")));
  });

  test("combined text reads cannot exceed the protocol slice byte bound", async () => {
    fs.writeFileSync(path.join(assets, "large.txt"), "x".repeat(BOUND / 2));
    await assert.rejects(execute(ask("read-texts", {
      paths: ["large.txt", "large.txt"], limit: BOUND, slice: "large",
    }), new Map()), /slice.*bound/);
  });

  test("malformed requests fail explicitly instead of guessing", async () => {
    await assert.rejects(execute(ask("read-texts", { paths: "lessons/catalogue.json", limit: 65536, slice: "s" }), new Map()), /paths/);
    await assert.rejects(execute(ask("read-texts", { paths: ["lessons/catalogue.json"], slice: "s" }), new Map()), /limit/);
    await assert.rejects(execute(ask("read-texts", { paths: ["lessons/catalogue.json"], limit: 0, slice: "s" }), new Map()), /limit/);
    await assert.rejects(execute(ask("read-texts", { paths: ["lessons/catalogue.json"], limit: 65536 }), new Map()), /slice/);
    await assert.rejects(execute(ask("store-read", { name: "lessons/progress.json", slice: "s" }), new Map()), /limit/);
  });

  test("explicit file selection reads bounded text into the requested host slice", async () => {
    const selected = path.join(outside, "proof.ospml");
    fs.writeFileSync(selected, "p ∧ q");
    const dialogs = { open: async (types: readonly string[]) => { assert.deepStrictEqual(types, ["ospml"]); return selected; }, save: async () => undefined };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, ...{ dialogs } });
    const response = await files(ask("choose-file", { types: ["ospml"], limit: 7, slice: "document" }), new Map()) as Response;
    assert.deepStrictEqual(response, { result: { path: selected, cancelled: false }, slices: [{ key: "document", text: "p ∧ q" }] });
    await assert.rejects(files(ask("choose-file", { types: ["ospml"], limit: 6, slice: "document" }), new Map()), /bound/);
  });

  test("save-file writes only the Osprey host slice to an explicitly chosen path", async () => {
    const selected = path.join(outside, "export.html");
    const dialogs = { open: async () => undefined, save: async (name: string) => { assert.strictEqual(name, "proof.html"); return selected; } };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, ...{ dialogs } });
    const response = await files(ask("save-file", { name: "proof.html", slice: "export" }), new Map([["export", "<math>∧</math>"]])) as Response;
    assert.deepStrictEqual(response.result, { path: selected, cancelled: false });
    assert.strictEqual(fs.readFileSync(selected, "utf8"), "<math>∧</math>");
    assert.ok(!fs.readdirSync(outside).some(name => name.includes(".tmp-")));
  });

  test("cancelled file dialogs deliver no text and write nothing", async () => {
    const dialogs = { open: async () => undefined, save: async () => undefined };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, ...{ dialogs } });
    for (const request of [ask("choose-file", { types: [], limit: 10, slice: "input" }), ask("save-file", { name: "proof.txt", slice: "export" })]) {
      const response = await files(request, new Map([["export", "proof"]])) as Response;
      assert.deepStrictEqual(response, { result: { path: "", cancelled: true }, slices: [] });
    }
  });

  test("document requests validate bounds names and slices before showing dialogs", async () => {
    const unexpected = async (): Promise<undefined> => assert.fail("invalid request reached a system dialog");
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs: { open: unexpected, save: unexpected } });
    for (const request of [ask("choose-file", { types: ["../txt"], limit: 1, slice: "x" }),
      ask("choose-file", { types: [], limit: BOUND + 1, slice: "x" }),
      ask("save-file", { name: "../escape", slice: "export" }), ask("save-file", { name: "proof.txt", slice: "absent" })]) {
      await assert.rejects(files(request, new Map([["export", "proof"]])), /filter|limit|name|absent/);
    }
    await assert.rejects(files(ask("save-file", { name: "large.txt", slice: "export" }), new Map([["export", "∧".repeat(BOUND / 2)]])), /bound/);
  });

  test("selected binary data cannot silently become replacement text", async () => {
    const selected = path.join(outside, "malformed.txt");
    fs.writeFileSync(selected, Buffer.from([0xc3, 0x28]));
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store,
      dialogs: { open: async () => selected, save: async () => undefined } });
    await assert.rejects(files(ask("choose-file", { types: [], limit: 10, slice: "input" }), new Map()), /UTF-8|encoded data/i);
  });

  test("bounded reads assemble every byte when the filesystem returns short reads", async () => {
    const selected = path.join(outside, "short-reads.ospml");
    const text = "p ∧ q ≤ ⊤\n".repeat(3);
    fs.writeFileSync(selected, text);
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store,
      dialogs: { open: async () => selected, save: async () => undefined } });
    const real = fs.readSync;
    fs.readSync = limitedReads(2, real);
    try {
      const response = await files(ask("choose-file", { types: [], limit: 65536, slice: "document" }), new Map()) as Response;
      assert.deepStrictEqual(response.slices, [{ key: "document", text }]);
      const tooSmall = Buffer.byteLength(text, "utf8") - 1;
      await assert.rejects(files(ask("choose-file", { types: [], limit: tooSmall, slice: "document" }), new Map()), /bound/);
    } finally { fs.readSync = real; }
  });

  test("export never follows or removes a pre-existing temporary symlink", async () => {
    const selected = path.join(outside, "safe-export.txt"), secret = path.join(outside, "untouched.txt");
    fs.writeFileSync(secret, "private original");
    const now = Date.now;
    Date.now = () => 123456;
    const temporary = `${selected}.tmp-${process.pid}-123456`;
    fs.symlinkSync(secret, temporary);
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store,
      dialogs: { open: async () => undefined, save: async () => selected } });
    try {
      await assert.rejects(files(ask("save-file", { name: "safe-export.txt", slice: "export" }), new Map([["export", "replacement"]])), /EEXIST/);
      assert.strictEqual(fs.readFileSync(secret, "utf8"), "private original");
      assert.ok(fs.lstatSync(temporary).isSymbolicLink(), "refusal removed a file the adapter did not create");
      assert.ok(!fs.existsSync(selected));
    } finally { Date.now = now; }
  });
});

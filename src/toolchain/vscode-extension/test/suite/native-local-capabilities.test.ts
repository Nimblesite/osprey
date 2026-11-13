// Implements [LESSONS-FILES] and [LESSONS-PROGRESS] host-glue regressions:
// the local executor reads only approved bundled assets, keeps progress
// inside one rooted store directory, and refuses every other request
// explicitly. Real files, real symlinks, real bounds; fixtures are cleaned.
// Requests arrive as the frame's ask rows ([NATIVE-SHM-ASK]), not a JSON envelope.
import * as assert from "assert";
import fs = require("fs");
import type { ReadPosition, ReadSyncOptions } from "fs";
import * as os from "os";
import * as path from "path";
import { Ask, type Completion } from "../../client/src/osprey-ui/asks";
import { createLocalExecutor } from "../../client/src/osprey-ui/local-capabilities";

const catalogue = JSON.stringify({ format: 1, slugs: ["sines"] });
const progress = "{\"completed\":[\"sines/one\"]}";
const BOUND = 8 * 1024 * 1024;

interface FileEntry { readonly path: string; readonly text: string; readonly error: string; }

/** An ask with one row per argument: its position, its whole number and its text. */
function ask(code: number, ticket: string, fields: readonly (readonly [number, number, string])[]): Ask {
  return new Ask(code, ticket, fields.map(([field, number, text]) => ({ field, number, text })));
}

const texts = (field: number, ...values: string[]): (readonly [number, number, string])[] =>
  values.map((text) => [field, 0, text] as const);
const whole = (field: number, number: number): readonly [number, number, string] => [field, number, ""];

const READ_TEXTS = 4;
const STORE_READ = 6;
const STORE_WRITE = 7;
const CHOOSE_FILE = 13;
const SAVE_FILE = 15;

function documentOf(completion: Completion): Record<string, unknown> {
  const value: unknown = JSON.parse(completion.detail);
  assert.ok(typeof value === "object" && value !== null && !Array.isArray(value), completion.detail);
  return value as Record<string, unknown>;
}

function filesOf(completion: Completion): FileEntry[] {
  const files = documentOf(completion).files;
  assert.ok(Array.isArray(files));
  return files as FileEntry[];
}

function held(text: string): Map<string, string> {
  return new Map([["lessons:progress", text]]);
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

  test("read-texts returns approved bundled files in its result document", async () => {
    const response = await execute(ask(READ_TEXTS, "lessons", [...texts(1, "lessons/catalogue.json"), whole(2, 65536), ...texts(3, "lessons:texts")]), new Map());
    assert.deepStrictEqual(filesOf(response), [{ path: "lessons/catalogue.json", text: catalogue, error: "" }]);
    assert.deepStrictEqual(documentOf(response), { files: filesOf(response) });
  });

  test("read-texts reports refused assets per file and keeps the readable ones", async () => {
    fs.writeFileSync(path.join(outside, "secret.json"), "{}");
    fs.symlinkSync(path.join(outside, "secret.json"), path.join(assets, "lessons", "linked.json"));
    const response = await execute(ask(READ_TEXTS, "lessons", [...texts(1, "../secret.json", "lessons/linked.json", "lessons/missing.json", "/etc/hosts", "lessons/catalogue.json"),
      whole(2, 65536), ...texts(3, "lessons:texts")]), new Map());
    const files = filesOf(response);
    for (const refused of files.slice(0, 4)) {
      assert.strictEqual(refused.text, "");
      assert.ok(refused.error !== "");
    }
    assert.deepStrictEqual(files[4], { path: "lessons/catalogue.json", text: catalogue, error: "" });
  });

  test("read-texts honours the requested byte bound exactly", async () => {
    fs.writeFileSync(path.join(assets, "lessons", "small.json"), "ab");
    const exact = await execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/small.json"), whole(2, 2), ...texts(3, "s")]), new Map());
    assert.deepStrictEqual(filesOf(exact), [{ path: "lessons/small.json", text: "ab", error: "" }]);
    const refused = await execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/catalogue.json"), whole(2, 2), ...texts(3, "s")]), new Map());
    assert.strictEqual(filesOf(refused)[0]!.text, "");
    assert.ok(filesOf(refused)[0]!.error.includes("bound"));
  });

  test("store-write persists a host-held slice and store-read reads it back", async () => {
    const written = await execute(ask(STORE_WRITE, "lessons", [...texts(1, "lessons/progress.json"), ...texts(2, "lessons:progress")]), held(progress));
    assert.deepStrictEqual(documentOf(written), { path: "lessons/progress.json" });
    const saved = path.join(store, "lessons", "progress.json");
    assert.strictEqual(fs.readFileSync(saved, "utf8"), progress);
    assert.deepStrictEqual(fs.readdirSync(store), ["lessons"]);
    if (process.platform !== "win32") {
      assert.strictEqual(fs.statSync(path.join(store, "lessons")).mode & 0o777, 0o700);
      assert.strictEqual(fs.statSync(saved).mode & 0o777, 0o600);
    }
    const read = await execute(ask(STORE_READ, "lessons", [...texts(1, "lessons/progress.json"), whole(2, 65536), ...texts(3, "lessons:progress")]), new Map());
    assert.deepStrictEqual(documentOf(read), { found: true, text: progress });
  });

  test("store-read reports a missing file as not found without content", async () => {
    const read = await execute(ask(STORE_READ, "lessons", [...texts(1, "lessons/absent.json"), whole(2, 65536), ...texts(3, "lessons:progress")]), new Map());
    assert.deepStrictEqual(documentOf(read), { found: false });
  });

  test("store-read refuses a file beyond its byte bound", async () => {
    await assert.rejects(
      execute(ask(STORE_READ, "lessons", [...texts(1, "lessons/progress.json"), whole(2, 4), ...texts(3, "lessons:progress")]), new Map()),
      /bound/,
    );
  });

  test("store names never escape the rooted progress directory", async () => {
    for (const name of ["../escape.json", "a/../escape.json", "/etc/passwd", "a//b.json", ".", "lessons/../../escape.json", ""]) {
      await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(1, name), ...texts(2, "lessons:progress")]), held(progress)), /store name|no valid name/i);
      await assert.rejects(execute(ask(STORE_READ, "t", [...texts(1, name), whole(2, 65536), ...texts(3, "s")]), new Map()), /store name|no valid name/i);
    }
    assert.ok(!fs.existsSync(path.join(root, "escape.json")));
  });

  test("symlinked store paths are refused, not followed", async () => {
    const secret = path.join(outside, "progress.json");
    fs.writeFileSync(secret, "escaped");
    fs.symlinkSync(secret, path.join(store, "link.json"));
    fs.symlinkSync(outside, path.join(store, "linked-dir"));
    await assert.rejects(execute(ask(STORE_READ, "t", [...texts(1, "link.json"), whole(2, 65536), ...texts(3, "s")]), new Map()), /progress directory/);
    await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(1, "link.json"), ...texts(2, "lessons:progress")]), held(progress)), /progress directory/);
    await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(1, "linked-dir/progress.json"), ...texts(2, "lessons:progress")]), held(progress)), /progress directory/);
    assert.strictEqual(fs.readFileSync(secret, "utf8"), "escaped");
  });

  test("store-write demands the host-held slice it is asked to persist", async () => {
    await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(1, "lessons/absent.json"), ...texts(2, "lessons:progress")]), new Map()), /host slice/);
    await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(1, "lessons/absent.json")]), held(progress)), /slice/);
    assert.ok(!fs.existsSync(path.join(store, "lessons", "absent.json")));
  });

  test("store-write refuses an over-bound host slice", async () => {
    const huge = new Map([["lessons:progress", "x".repeat(BOUND + 1)]]);
    await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(1, "lessons/huge.json"), ...texts(2, "lessons:progress")]), huge), /bound/);
  });

  test("unsupported requests fail explicitly", async () => {
    const codes: readonly (readonly [number, string])[] = [[10, "http"], [1, "run"], [17, "reveal"], [20, "recognise"],
      [8, "store-list"], [9, "store-delete"], [5, "fingerprint"], [11, "host-call"]];
    for (const [code, kind] of codes) {
      await assert.rejects(execute(ask(code, "t", []), new Map()), new RegExp(`Unsupported local capability: ${kind}`));
    }
  });

  test("store refuses symlink ancestors when intermediate directories do not exist", async () => {
    fs.symlinkSync(outside, path.join(store, "nested-link"));
    const name = "nested-link/new/progress.json";
    await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(1, name), ...texts(2, "lessons:progress")]), held(progress)), /progress directory/);
    assert.ok(!fs.existsSync(path.join(outside, "new")));
  });

  test("store-write bounds UTF8 bytes rather than UTF16 characters", async () => {
    await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(1, "utf8.json"), ...texts(2, "lessons:progress")]), held("∧".repeat(BOUND / 2))), /bound/);
    assert.ok(!fs.existsSync(path.join(store, "utf8.json")));
  });

  test("combined text reads cannot exceed their byte bound", async () => {
    fs.writeFileSync(path.join(assets, "large.txt"), "x".repeat(BOUND / 2));
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "large.txt", "large.txt"), whole(2, BOUND), ...texts(3, "large")]), new Map()), /bound/);
  });

  test("malformed asks fail explicitly instead of guessing", async () => {
    await assert.rejects(execute(ask(READ_TEXTS, "t", [whole(2, 65536), ...texts(3, "s")]), new Map()), /paths/);
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/catalogue.json"), ...texts(3, "s")]), new Map()), /limit/);
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/catalogue.json"), whole(2, 0), ...texts(3, "s")]), new Map()), /limit/);
    await assert.rejects(execute(ask(STORE_READ, "t", [...texts(1, "lessons/progress.json"), ...texts(3, "s")]), new Map()), /limit/);
  });

  test("explicit file selection completes with the bare path and writes no slice", async () => {
    const selected = path.join(outside, "proof.ospml");
    fs.writeFileSync(selected, "p ∧ q");
    const dialogs = { open: async (types: readonly string[]) => { assert.deepStrictEqual(types, ["ospml"]); return selected; }, save: async () => undefined };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs });
    const response = await files(ask(CHOOSE_FILE, "document", [...texts(1, "ospml"), whole(2, 7), ...texts(3, "document")]), new Map());
    assert.deepStrictEqual(response, { detail: selected });
  });

  test("save-file writes only the Osprey host slice to an explicitly chosen path", async () => {
    const selected = path.join(outside, "export.html");
    const dialogs = { open: async () => undefined, save: async (name: string) => { assert.strictEqual(name, "proof.html"); return selected; } };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs });
    const response = await files(ask(SAVE_FILE, "export", [...texts(1, "proof.html"), ...texts(2, "export")]), new Map([["export", "<math>∧</math>"]]));
    assert.deepStrictEqual(response, { detail: selected });
    assert.strictEqual(fs.readFileSync(selected, "utf8"), "<math>∧</math>");
    assert.ok(!fs.readdirSync(outside).some(name => name.includes(".tmp-")));
  });

  test("cancelled file dialogs complete with an empty path and write nothing", async () => {
    const dialogs = { open: async () => undefined, save: async () => undefined };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs });
    for (const request of [ask(CHOOSE_FILE, "input", [...texts(1, "txt"), whole(2, 10), ...texts(3, "input")]),
      ask(SAVE_FILE, "export", [...texts(1, "proof.txt"), ...texts(2, "export")])]) {
      const response = await files(request, new Map([["export", "proof"]]));
      assert.deepStrictEqual(response, { detail: "" });
    }
  });

  test("document requests validate bounds names and slices before showing dialogs", async () => {
    const unexpected = async (): Promise<undefined> => assert.fail("invalid request reached a system dialog");
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs: { open: unexpected, save: unexpected } });
    for (const request of [ask(CHOOSE_FILE, "x", [...texts(1, "../txt"), whole(2, 1), ...texts(3, "x")]),
      ask(CHOOSE_FILE, "x", [whole(2, BOUND + 1), ...texts(3, "x")]),
      ask(SAVE_FILE, "x", [...texts(1, "../escape"), ...texts(2, "export")]),
      ask(SAVE_FILE, "x", [...texts(1, "proof.txt"), ...texts(2, "absent")])]) {
      await assert.rejects(files(request, new Map([["export", "proof"]])), /filter|limit|name|absent/);
    }
    await assert.rejects(files(ask(SAVE_FILE, "x", [...texts(1, "large.txt"), ...texts(2, "export")]),
      new Map([["export", "∧".repeat(BOUND / 2)]])), /bound/);
  });

  test("read-texts refuses invalid UTF-8 instead of substitution text", async () => {
    fs.writeFileSync(path.join(assets, "malformed.txt"), Buffer.from([0xc3, 0x28]));
    const response = await execute(ask(READ_TEXTS, "t", [...texts(1, "malformed.txt"), whole(2, 10), ...texts(3, "input")]), new Map());
    assert.strictEqual(filesOf(response)[0]!.text, "");
    assert.ok(filesOf(response)[0]!.error.match(/UTF-8|encoded data/i), filesOf(response)[0]!.error);
  });

  test("bounded reads assemble every byte when the filesystem returns short reads", async () => {
    const text = "p ∧ q ≤ ⊤\n".repeat(3);
    fs.writeFileSync(path.join(assets, "short-reads.ospml"), text);
    const real = fs.readSync;
    fs.readSync = limitedReads(2, real);
    try {
      const response = await execute(ask(READ_TEXTS, "t", [...texts(1, "short-reads.ospml"), whole(2, 65536), ...texts(3, "document")]), new Map());
      assert.deepStrictEqual(filesOf(response), [{ path: "short-reads.ospml", text, error: "" }]);
      const tooSmall = Buffer.byteLength(text, "utf8") - 1;
      const refused = await execute(ask(READ_TEXTS, "t", [...texts(1, "short-reads.ospml"), whole(2, tooSmall), ...texts(3, "document")]), new Map());
      assert.strictEqual(filesOf(refused)[0]!.text, "");
      assert.ok(filesOf(refused)[0]!.error.includes("bound"));
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
      await assert.rejects(files(ask(SAVE_FILE, "export", [...texts(1, "safe-export.txt"), ...texts(2, "export")]),
        new Map([["export", "replacement"]])), /EEXIST/);
      assert.strictEqual(fs.readFileSync(secret, "utf8"), "private original");
      assert.ok(fs.lstatSync(temporary).isSymbolicLink(), "refusal removed a file the adapter did not create");
      assert.ok(!fs.existsSync(selected));
    } finally { Date.now = now; }
  });
});

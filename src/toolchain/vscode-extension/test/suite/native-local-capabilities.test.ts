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

/** The one host-held slice a reading adapter delivered: its key and parsed text. */
function sliceOf(completion: Completion): { key: string; text: string } {
  assert.strictEqual(completion.slices?.length, 1, completion.detail);
  const [delivered] = completion.slices ?? [];
  assert.ok(delivered !== undefined);
  return delivered;
}

/** The files a read-texts delivered into its slice, each with its text. */
function filesOf(completion: Completion): FileEntry[] {
  const files: unknown = (JSON.parse(sliceOf(completion).text) as Record<string, unknown>).files;
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

  test("read-texts returns approved bundled files and delivers the files slice", async () => {
    const response = await execute(ask(READ_TEXTS, "lessons", [...texts(1, "lessons/catalogue.json"), whole(2, 65536), ...texts(3, "lessons:texts")]), new Map());
    assert.deepStrictEqual(filesOf(response), [{ path: "lessons/catalogue.json", text: catalogue, error: "" }]);
    // The completion names each file and its fault; the text itself travels only in the slice the app reads.
    assert.deepStrictEqual(documentOf(response), { files: filesOf(response).map(({ path: source, error }) => ({ path: source, error })) });
    assert.strictEqual(sliceOf(response).key, "lessons:texts");
  });

  test("read-texts returns approved bundled files and delivers the files slice (extra: several files share one slice delivery)", async () => {
    fs.writeFileSync(path.join(assets, "lessons", "one.json"), "one");
    fs.writeFileSync(path.join(assets, "lessons", "two.json"), "two");
    const response = await execute(ask(READ_TEXTS, "lessons", [...texts(1, "lessons/one.json", "lessons/two.json", "lessons/missing.json"),
      whole(2, 65536), ...texts(3, "lessons:texts")]), new Map());
    // One slice carries every file's text in read order; the document names each fault.
    assert.deepStrictEqual(JSON.parse(sliceOf(response).text), { files: [
      { path: "lessons/one.json", text: "one", error: "" },
      { path: "lessons/two.json", text: "two", error: "" },
      { path: "lessons/missing.json", text: "", error: filesOf(response)[2]!.error }] });
    assert.match(filesOf(response)[2]!.error, /no such file|ENOENT/i);
    assert.strictEqual((documentOf(response).files as { error: string }[])[2]!.error !== "", true);
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
    assert.deepStrictEqual(documentOf(read), { found: true });
    assert.deepStrictEqual(sliceOf(read), { key: "lessons:progress", text: progress });
  });

  test("store-read reports a missing file as not found without a slice delivery", async () => {
    const read = await execute(ask(STORE_READ, "lessons", [...texts(1, "lessons/absent.json"), whole(2, 65536), ...texts(3, "lessons:progress")]), new Map());
    assert.deepStrictEqual(documentOf(read), { found: false });
    assert.strictEqual(read.slices, undefined);
  });

  test("store-read reports a missing file as not found without a slice delivery (extra: a directory is not a found document)", async () => {
    fs.mkdirSync(path.join(store, "lessons", "nested-folder"), { recursive: true });
    const read = await execute(ask(STORE_READ, "t", [...texts(1, "lessons/nested-folder"), whole(2, 65536), ...texts(3, "s")]), new Map());
    assert.deepStrictEqual(documentOf(read), { found: false });
    assert.strictEqual(read.slices, undefined);
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

  test("combined text reads cannot exceed the protocol slice byte bound", async () => {
    fs.writeFileSync(path.join(assets, "large.txt"), "x".repeat(BOUND / 2));
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "large.txt", "large.txt"), whole(2, BOUND), ...texts(3, "large")]), new Map()), /bound/);
  });

  test("combined text reads cannot exceed the protocol slice byte bound (extra: one bound-sized file alone still delivers)", async () => {
    const text = "x".repeat(BOUND - 100);
    fs.writeFileSync(path.join(assets, "whole.txt"), text);
    const response = await execute(ask(READ_TEXTS, "t", [...texts(1, "whole.txt"), whole(2, BOUND), ...texts(3, "whole")]), new Map());
    assert.strictEqual(JSON.parse(sliceOf(response).text).files[0].text, text);
    fs.writeFileSync(path.join(assets, "over.txt"), text + "x".repeat(120));
    const perFile = await execute(ask(READ_TEXTS, "t", [...texts(1, "over.txt"), whole(2, 10), ...texts(3, "whole")]), new Map());
    assert.strictEqual(filesOf(perFile)[0]!.text, "");
    assert.ok(filesOf(perFile)[0]!.error.includes("bound"));
  });

  test("malformed requests fail explicitly instead of guessing", async () => {
    await assert.rejects(execute(ask(READ_TEXTS, "t", [whole(2, 65536), ...texts(3, "s")]), new Map()), /paths/);
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/catalogue.json"), ...texts(3, "s")]), new Map()), /limit/);
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/catalogue.json"), whole(2, 0), ...texts(3, "s")]), new Map()), /limit/);
    await assert.rejects(execute(ask(STORE_READ, "t", [...texts(1, "lessons/progress.json"), ...texts(3, "s")]), new Map()), /limit/);
  });

  test("malformed requests fail explicitly instead of guessing (extra: repeated and missing slice keys are caught early)", async () => {
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/catalogue.json"), whole(2, 65536)]), new Map()), /slice/);
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/catalogue.json"), whole(2, 65536), ...texts(3, "")]), new Map()), /slice/);
    await assert.rejects(execute(ask(READ_TEXTS, "t", [...texts(1, "lessons/catalogue.json"), whole(2, -1), ...texts(3, "s")]), new Map()), /limit/);
    await assert.rejects(execute(ask(STORE_WRITE, "t", [...texts(2, "lessons:progress")]), held(progress)), /name/);
  });

  test("explicit file selection reads bounded text into the requested host slice", async () => {
    // Over the envelope the selected text rode a slice; over asks the completion carries the bare
    // path and its limit is validated before any dialog opens — the same bounded-selection fact.
    const selected = path.join(outside, "proof.ospml");
    fs.writeFileSync(selected, "p ∧ q");
    const dialogs = { open: async (types: readonly string[]) => { assert.deepStrictEqual(types, ["ospml"]); return selected; }, save: async () => undefined };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs });
    const response = await files(ask(CHOOSE_FILE, "document", [...texts(1, "ospml"), whole(2, 7), ...texts(3, "document")]), new Map());
    assert.deepStrictEqual(response, { detail: selected });
    await assert.rejects(files(ask(CHOOSE_FILE, "document", [...texts(1, "ospml"), whole(2, 0), ...texts(3, "document")]), new Map()), /between 1 and/);
  });

  test("explicit file selection reads bounded text into the requested host slice (extra: only absolute paths the dialog returned are accepted)", async () => {
    const dialogs = { open: async () => "relative/proof.ospml", save: async () => undefined };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs });
    await assert.rejects(files(ask(CHOOSE_FILE, "document", [...texts(1, "ospml"), whole(2, 7), ...texts(3, "document")]), new Map()), /absolute/);
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

  test("cancelled file dialogs deliver no text and write nothing", async () => {
    const dialogs = { open: async () => undefined, save: async () => undefined };
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs });
    for (const request of [ask(CHOOSE_FILE, "input", [...texts(1, "txt"), whole(2, 10), ...texts(3, "input")]),
      ask(SAVE_FILE, "export", [...texts(1, "proof.txt"), ...texts(2, "export")])]) {
      const response = await files(request, new Map([["export", "proof"]]));
      assert.deepStrictEqual(response, { detail: "" });
    }
    assert.ok(!fs.existsSync(path.join(outside, "proof.txt")), "a cancelled export writes nothing");
  });

  test("cancelled file dialogs deliver no text and write nothing (extra: an empty filter list never reaches the dialog)", async () => {
    const unexpected = async (): Promise<undefined> => assert.fail("an invalid filter reached a system dialog");
    const files = createLocalExecutor({ assetRoot: assets, storeRoot: store, dialogs: { open: unexpected, save: unexpected } });
    await assert.rejects(files(ask(CHOOSE_FILE, "input", [...texts(1), whole(2, 10), ...texts(3, "input")]), new Map()), /filter/);
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

  test("selected binary data cannot silently become replacement text", async () => {
    // Selection now crosses as a bare path; the host's one text boundary is the approved read, so
    // the same fact lives there: malformed bytes are refused, never replaced.
    fs.writeFileSync(path.join(assets, "malformed.txt"), Buffer.from([0xc3, 0x28]));
    const response = await execute(ask(READ_TEXTS, "t", [...texts(1, "malformed.txt"), whole(2, 10), ...texts(3, "input")]), new Map());
    assert.strictEqual(filesOf(response)[0]!.text, "");
    assert.ok(filesOf(response)[0]!.error.match(/UTF-8|encoded data/i), filesOf(response)[0]!.error);
  });

  test("selected binary data cannot silently become replacement text (extra: a truncated multi-byte sequence is refused too)", async () => {
    fs.writeFileSync(path.join(assets, "truncated.txt"), Buffer.from([0xe2, 0x88]));
    const response = await execute(ask(READ_TEXTS, "t", [...texts(1, "truncated.txt"), whole(2, 10), ...texts(3, "input")]), new Map());
    assert.strictEqual(filesOf(response)[0]!.text, "");
    assert.ok(filesOf(response)[0]!.error.match(/UTF-8|encoded data/i), filesOf(response)[0]!.error);
    fs.writeFileSync(path.join(assets, "valid.txt"), Buffer.from([0xe2, 0x88, 0xa7]));
    const valid = await execute(ask(READ_TEXTS, "t", [...texts(1, "valid.txt"), whole(2, 10), ...texts(3, "input")]), new Map());
    assert.strictEqual(filesOf(valid)[0]!.text, "∧");
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

// Shared package fixtures for command contracts and real browser flows.

import * as path from "path";
import * as fs from "fs";
import { createHash } from "crypto";
import * as assert from "assert";
import { compileSdkWasm, runSdkWasmJson } from "./osprey-test-env";
import { BUILD_MS } from "./profiler-kit";
import { parseEnvelope, record, type NativeRequest } from "../../client/src/osprey-ui/protocol";
import { PackageCommandDeps, PkgReport, Ui } from "../../client/src/package-commands";

export const okAdd = (key: string): PkgReport => ({ status: 0, lines: [`added ${key} e1.r2`] });

/** A failing frontend report with a stable reason code, as the CLI prints. */
export const failed = (code: string): PkgReport => ({ status: 1, lines: [code] });

export interface RecordedCall {
  command: string;
  cwd: string;
}

export interface FrontendStub {
  calls: RecordedCall[];
  run(command: string, cwd: string): Promise<PkgReport>;
}

/** A frontend runner that records each (command, cwd) and answers in order. */
export function stubFrontend(results: PkgReport[]): FrontendStub {
  const calls: RecordedCall[] = [];
  return {
    calls,
    run: async (command, cwd) => {
      calls.push({ command, cwd });
      return results[Math.min(calls.length - 1, results.length - 1)] ?? { status: 1, lines: ["stub"] };
    },
  };
}

/** Every UI surface the commands use, recording calls and answering a script. */
export interface UiScript {
  picks?: any[]; // one answer per showQuickPick call
  messages?: string[]; // labels of every shown message, appended
  confirm?: boolean[]; // one answer per showWarningMessage with a modal choice
  progress?: string[]; // titles reported through withProgress
}

export interface RecordedUi extends Ui {
  picks: any[];
  messages: string[];
  progress: string[];
}

export function fakeUi(script: UiScript = {}): RecordedUi {
  const state = { answers: [...(script.picks ?? [])] };
  const messages: string[] = [];
  const progress: string[] = [];
  const confirms = [...(script.confirm ?? [true])];
  return {
    messages,
    progress,
    get picks() {
      return state.answers;
    },
    set picks(answers) {
      state.answers = answers;
    },
    showQuickPick: async <T>(items: T[]) =>
      (state.answers.length > 0 ? state.answers.shift() : items[0]) as T | undefined,
    showWarningMessage: async (message: string) => {
      messages.push(message);
      return confirms.shift() === false ? undefined : { title: "Add Anyway" };
    },
    showErrorMessage: (message: string) => {
      messages.push(message);
      return Promise.resolve(undefined);
    },
    showInformationMessage: (message: string) => {
      messages.push(message);
      return Promise.resolve(undefined);
    },
    withProgress: <T>(options: { title: string }, task: () => Thenable<T>) => {
      progress.push(options.title);
      return task();
    },
  };
}

export interface DepsScript {
  projects?: string[];
  manifest?: string;
  dirty?: string[];
  listing?: string;
  frontend?: PkgReport[];
  cancelled?: boolean;
}

/** Deps exactly as the commands see them: static workspace, static registry. */
export function fakeDeps(script: DepsScript = {}): PackageCommandDeps & { frontend: FrontendStub; ui: RecordedUi } {
  const frontend = stubFrontend(script.frontend ?? [okAdd("@demo/one")]);
  const mutate = frontend.run;
  frontend.run = async (command, cwd) => {
    if (command === "--json tree --direct --project .") {
      frontend.calls.push({ command, cwd });
      return { status: 0, lines: ["@other/util", "@demo/pyadd"] };
    }
    return mutate(command, cwd);
  };
  const ui = fakeUi();
  return {
    ui,
    frontend,
    registry: () => "http://127.0.0.1:8787",
    projectRoots: () => script.projects ?? [path.resolve("/work/app")],
    manifestText: (root) => (script.manifest === undefined || path.resolve(root) !== path.resolve(script.projects?.[0] ?? path.resolve("/work/app")) ? undefined : script.manifest),
    dirtyFiles: () => script.dirty ?? [],
    fetchListing: async () => script.listing ?? '{"packages":[]}',
    cancelled: () => script.cancelled ?? false,
  };
}

function integerBytes(value: number, width: number): Buffer {
  const encoded = Buffer.alloc(width);
  if (width === 8) encoded.writeBigUInt64BE(BigInt(value));
  else encoded.writeUInt32BE(value);
  return encoded;
}

function sourceDigest(files: readonly { path: string; text: string }[]): string {
  const entries = files.map((file) => Buffer.concat([
    integerBytes(Buffer.byteLength(file.path), 4), Buffer.from(file.path), Buffer.from([0]),
    integerBytes(Buffer.byteLength(file.text), 8), createHash("sha256").update(file.text).digest(),
  ]));
  const envelope = Buffer.concat([Buffer.from("osprey.source.v1\0"), integerBytes(files.length, 4), ...entries]);
  return `sha256:${createHash("sha256").update(envelope).digest("hex")}`;
}

export function servedPackage(name: string, dependency = "") {
  const manifest = `[project]\nname = "${name}"\nsource_roots = ["src"]\n[package]\nname = "@demo/${name}"\nlicense = "MIT"\nsummary = "Browser fixture ${name}"\nkeywords = ["browser-e2e"]\n` +
    (dependency ? `[dependencies]\n"@demo/${dependency}" = { epoch = 1, min_release = 1 }\n` : "");
  const files = [
    { path: "LICENSE", text: fs.readFileSync(path.resolve(__dirname, "../../../../LICENSE"), "utf8") },
    { path: "osprey.toml", text: manifest },
    { path: "src/main.osp", text: `namespace demo_${name};\nmodule Api { export fn value() = 42 }\n` },
  ];
  return { key: `@demo/${name}`, epoch: 1, ordinal: 1, digest: sourceDigest(files), files };
}

export function installRegistry() {
  const packages = [servedPackage("greeting", "maths"), servedPackage("maths")];
  const listing = { packages: packages.map(({ key, epoch, ordinal, digest }) => ({ key, releases: [{ epoch, ordinal, digest }] })) };
  const sources = new Map(packages.map((pkg) => [`/v1/releases/${pkg.digest}/source`, pkg]));
  return { listing, sources, packages };
}


const SDK_HOST_SOURCE = String.raw`namespace sdk_host_probe
import vscode::Editor
import ui::Runtime
import ui::Keyed
import ui::Ui
import ui::Doc
import ui::Requests

next (result, continuation) = match result
    Error message => Runtime::step ("error: " + message)
    Success request => continuation request

raw (ticket, space, operation, arguments) = Requests::ask (ticket, ui::HostCall(hostNamespace = space, hostOperation = operation, hostArguments = arguments, hostTimeout = 10000, hostLimit = 1024, hostSlice = ""))
rejected () = [
    raw ("sdk/no-doc-args", "vscode", "document.open", JsonNull),
    raw ("sdk/remote-doc", "vscode", "document.open", JsonObject [Doc::str ("uri", "vscode-remote://ssh-test/work/file"), Doc::num ("line", 1), Doc::num ("column", 1)]),
    raw ("sdk/no-config-args", "vscode", "configuration.read", JsonNull),
    raw ("sdk/no-reveal-args", "vscode", "webview.reveal", JsonNull),
    raw ("sdk/bad-namespace", "other", "workspace.snapshot", JsonObject []),
    raw ("sdk/unknown-operation", "vscode", "impossible.operation", JsonObject []),
]

finish (args, opened, first, second) =
    next (Editor::workspaces "sdk/snapshot", \snapshot =>
    next (Editor::revealView ("sdk/reveal", "osprey.packages"), \revealed =>
    next (Editor::revealView ("sdk/bad-view", "osprey.unknown"), \bad =>
    next (Editor::openDocument ("sdk/missing", listGet args 3 ?: "", 1, 1), \missing =>
        Runtime::asking ("ok", listConcat [opened, first, second, snapshot, revealed, bad, missing] (rejected ()))))))

initial context =
    args = context.arguments
    next (Editor::openDocument ("sdk/open", listGet args 0 ?: "", 2, 3), \opened =>
    next (Editor::configuration ("sdk/config-one", "osprey.packages", listGet args 1 ?: ""), \first =>
    next (Editor::configuration ("sdk/config-two", "osprey.packages", listGet args 2 ?: ""), \second =>
        finish (args, opened, first, second))))

regions (_before, _after, frames, _) = [Keyed::region (frames, "sdk-host", [], \() => Ui::button ("SDK host probe", (ui::Symbols::name ui::Info), 0))]
palette = ui::Theme(tokens = [], spacing = [4], feedback = 160, settle = 220)
main () = print (Runtime::start (input (), initial, regions, \value => JsonString value, palette))
`;

let sdkHostModule: Promise<WebAssembly.Module> | undefined;

export function compileSdkHost(): Promise<WebAssembly.Module> {
  return sdkHostModule ??= compileSdkWasm(SDK_HOST_SOURCE, BUILD_MS);
}

export async function sdkHostRequests(arguments_: readonly string[]): Promise<readonly NativeRequest[]> {
  const envelope = await runSdkWasmJson(await compileSdkHost(), JSON.stringify({ platform: "vscode", arguments: arguments_ }));
  const parsed = parseEnvelope(JSON.stringify(envelope));
  assert.deepStrictEqual(parsed.diagnostics, []);
  const state: unknown = JSON.parse(parsed.state);
  assert.ok(record(state));
  assert.strictEqual(state.store, "ok", parsed.state);
  assert.strictEqual(parsed.requests.length, 13, "real SDK app emits every requested operation");
  return parsed.requests;
}

export function sdkRequest(requests: readonly NativeRequest[], ticket: string): NativeRequest {
  const request = requests.find((candidate) => candidate.ticket === ticket);
  assert.ok(request, `real SDK request ${ticket} exists`);
  return request;
}

// Shared package fixtures for command contracts and real browser flows.

import * as path from "path";
import * as fs from "fs";
import { createHash } from "crypto";
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

/**
 * Answer `showQuickPick` with nothing while collecting one `field` of every
 * offered item — how the tests observe a list the user then declines.
 */
export function recordQuickPicks<Field extends "label" | "detail">(
  deps: { ui: RecordedUi },
  field: Field,
): string[] {
  const seen: string[] = [];
  deps.ui.showQuickPick = async <T>(items: T[]) => {
    for (const item of items) {
      seen.push(String((item as unknown as Record<Field, unknown>)[field]));
    }
    return undefined;
  };
  return seen;
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
    { path: "LICENSE", text: fs.readFileSync(path.resolve(__dirname, "../../../../../../LICENSE"), "utf8") },
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



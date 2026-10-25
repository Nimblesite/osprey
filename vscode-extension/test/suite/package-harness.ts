// Shared fake-environment for the package UX suites ([PACKAGE-API],
// [PACKAGE-FOLDER-REGISTRY]): a recording UI, a stub frontend runner and
// canned registry listings, so every command-flow test observes exactly what
// the user would see and what the Osprey CLI would be asked to do — no real
// process, network or editor involved.

import * as path from "path";
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

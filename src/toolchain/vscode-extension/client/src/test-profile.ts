// The Test Explorer's **Profile** run profile ([TESTING-PROFILE]): the same
// discovery, filtering, and TAP mapping the Run profile uses, but each suite
// executes as `osprey <file> --run --profile` inside a per-suite artifact
// directory. When the run finishes, the artifacts become the flame panel and
// the inline heat decorations the standalone profiler command already renders
// ([PROF-VSCODE-FLAME], [PROF-VSCODE-HEAT]).
//
// The vscode-free decisions (which directory, what the summary line says) are
// pure exported helpers in test-explorer-docs.ts and here; this file is the
// thin wiring, mirroring profiler/profile-run.ts.

import * as os from "os";
import * as path from "path";
import { promises as fs } from "fs";
import * as vscode from "vscode";
import { loadArtifacts, artifactPaths } from "./profiler/profile-run";
import type { HeatDecorationManager } from "./profiler/heat-decorations";
import { showFlamePanel } from "./profiler/profiler-panel";
import { formatSummaryHeader } from "./profiler/summary";
import { profileRunHeader } from "./test-explorer-docs";
import {
  executeRunRequest,
  verdictSink,
  type TestRunSink,
} from "./test-explorer";
import type { RunMode } from "./test-explorer-parse";

/** Prefix of the per-request profile artifact root, under the OS temp dir. */
const PROFILE_DIR_PREFIX = "osprey-test-profile-";

/** Create the artifact root one Profile request writes beneath. */
export async function makeProfileRoot(
  tmpDir: string = os.tmpdir(),
): Promise<string> {
  return fs.mkdtemp(path.join(tmpDir, PROFILE_DIR_PREFIX));
}

/** The RunMode a Profile request executes under ([TESTING-PROFILE]). */
export function profileMode(root: string): RunMode {
  return { kind: "profile", dir: root };
}

/**
 * One suite's profiling outcome: where its artifacts landed and — when the
 * suite actually produced a profile — the loaded summary text and flame model.
 */
export interface ProfiledSuite {
  readonly uri: vscode.Uri;
  readonly dir: string;
  readonly loaded: boolean;
  readonly detail: string;
}

/**
 * Opens a loaded profile for viewing. Injectable so tests observe which suite
 * was presented without opening a webview; the return value is discarded.
 */
export type FlamePresenter = (
  ...args: Parameters<typeof showFlamePanel>
) => unknown;

/**
 * Load one suite's profiler artifacts and render them: the flame panel, the
 * heat decorations, and a summary line for the run output. Never throws — a
 * suite that failed to compile simply has no artifacts, and the run still
 * reports its TAP verdicts.
 */
export function presentProfile(
  uri: vscode.Uri,
  dir: string,
  heat: HeatDecorationManager | undefined,
  show: FlamePresenter = showFlamePanel,
): ProfiledSuite {
  const loaded = loadArtifacts(artifactPaths(dir, uri.fsPath));
  if (!loaded.ok) {
    return { uri, dir, loaded: false, detail: loaded.error };
  }
  show(loaded.value.model, loaded.value.summary, uri.fsPath);
  heat?.apply(loaded.value.summary);
  return {
    uri,
    dir,
    loaded: true,
    detail: formatSummaryHeader(loaded.value.summary),
  };
}

/**
 * A TestRunSink that forwards every verdict to `run` and turns each suite's
 * profiler artifacts into the flame panel + heat decorations, appending what
 * happened to the run output ([TESTING-PROFILE]).
 */
export function profileSink(
  run: vscode.TestRun,
  heat: HeatDecorationManager | undefined,
  present: typeof presentProfile = presentProfile,
): TestRunSink {
  return {
    ...verdictSink(run),
    addProfile: (uri, dir) => {
      const outcome = present(uri, dir, heat);
      run.appendOutput(
        profileRunHeader(uri.fsPath, dir).replace(/\n/g, "\r\n") +
          `${outcome.detail}\r\n`,
      );
    },
  };
}

/** The handler behind the Profile run profile. */
export function makeProfileHandler(
  controller: vscode.TestController,
  resolveCompiler: () => string,
  heat: () => HeatDecorationManager | undefined,
): (
  request: vscode.TestRunRequest,
  token: vscode.CancellationToken,
) => Promise<void> {
  return async (request, token) => {
    const root = await makeProfileRoot();
    await executeRunRequest(
      controller,
      request,
      profileSink(controller.createTestRun(request), heat()),
      token,
      resolveCompiler,
      profileMode(root),
    );
  };
}

/**
 * Register the Profile run profile on `controller`. It is a non-default Run
 * profile, so VS Code offers it under the Testing view's "Run with Profile…"
 * menu beside Run and Coverage — the play button keeps launching the plain
 * Run profile ([TESTING-PROFILE]).
 */
export function registerTestProfileProfile(
  controller: vscode.TestController,
  resolveCompiler: () => string,
  heat: () => HeatDecorationManager | undefined,
): vscode.TestRunProfile {
  return controller.createRunProfile(
    "Profile",
    vscode.TestRunProfileKind.Run,
    makeProfileHandler(controller, resolveCompiler, heat),
    false,
  );
}

// The `Osprey: Profile Current File` command ([PROF-VSCODE-FLAME]): compile +
// run the active file with `osprey <file> --run --profile` in a per-run temp
// dir, then load `<stem>.profile.json` + `<stem>.speedscope.json` and open the
// flame panel + heat decorations. Path building, artifact loading, and stderr
// tailing are pure exported helpers; the command shell stays thin.

import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import { isOspreyFile } from "../extension";
import { runCompiler } from "../test-explorer";
import type { ExecResult } from "../test-explorer-parse";
import {
  buildFlameModel,
  parseSpeedscope,
  type FlameModel,
  type Result,
} from "./flame-model";
import { HeatDecorationManager } from "./heat-decorations";
import { showFlamePanel } from "./profiler-panel";
import {
  formatSummaryHeader,
  parseSummary,
  type ProfileSummary,
} from "./summary";

export interface ProfileArtifacts {
  stem: string;
  profileJson: string;
  speedscope: string;
  cpuprofile: string;
  folded: string;
}

/** The files `--profile` writes into the run's cwd for a given source. Pure. */
export function artifactPaths(
  dir: string,
  sourcePath: string,
): ProfileArtifacts {
  const stem = path.basename(sourcePath, path.extname(sourcePath));
  return {
    stem,
    profileJson: path.join(dir, `${stem}.profile.json`),
    speedscope: path.join(dir, `${stem}.speedscope.json`),
    cpuprofile: path.join(dir, `${stem}.cpuprofile`),
    folded: path.join(dir, `${stem}.folded`),
  };
}

const STDERR_TAIL_LINES = 12;

/** The last non-empty stderr lines, for compact error messages. Pure. */
export function stderrTail(
  stderr: string,
  maxLines: number = STDERR_TAIL_LINES,
): string {
  const lines = stderr.split(/\r?\n/).filter((line) => line.trim() !== "");
  return lines.slice(-maxLines).join("\n");
}

export interface LoadedProfile {
  model: FlameModel;
  summary: ProfileSummary;
}

function readIfExists(filePath: string): string | undefined {
  try {
    return fs.readFileSync(filePath, "utf8");
  } catch {
    return undefined;
  }
}

/** Read + validate both artifacts of one run. Never throws. */
export function loadArtifacts(
  artifacts: ProfileArtifacts,
): Result<LoadedProfile> {
  const summaryText = readIfExists(artifacts.profileJson);
  const speedscopeText = readIfExists(artifacts.speedscope);
  if (summaryText === undefined || speedscopeText === undefined) {
    return {
      ok: false,
      error: `profiler output missing: expected ${artifacts.profileJson} and ${artifacts.speedscope}`,
    };
  }
  const summary = parseSummary(summaryText);
  if (!summary.ok) {
    return summary;
  }
  const doc = parseSpeedscope(speedscopeText);
  return doc.ok
    ? {
        ok: true,
        value: { model: buildFlameModel(doc.value), summary: summary.value },
      }
    : doc;
}

interface LastRun extends LoadedProfile {
  sourcePath: string;
}

function appendStream(output: vscode.OutputChannel, text: string): void {
  if (text !== "") {
    output.append(text.endsWith("\n") ? text : `${text}\n`);
  }
}

function appendProcessOutput(
  output: vscode.OutputChannel,
  result: ExecResult,
): void {
  appendStream(output, result.stdout);
  appendStream(output, result.stderr);
  output.appendLine(`Profiler process exited with code ${result.exitCode}`);
}

function finishRun(
  result: ExecResult,
  tmpDir: string,
  sourcePath: string,
  output: vscode.OutputChannel,
  heat: HeatDecorationManager,
  remember: (run: LastRun) => void,
): boolean {
  const loaded = loadArtifacts(artifactPaths(tmpDir, sourcePath));
  if (!loaded.ok) {
    output.appendLine(loaded.error);
    const detail = stderrTail(result.stderr) || loaded.error;
    void vscode.window.showErrorMessage(`Osprey profiling failed: ${detail}`);
    return false;
  }
  output.appendLine(formatSummaryHeader(loaded.value.summary));
  output.appendLine(`Profile artifacts kept in ${tmpDir}`);
  remember({ ...loaded.value, sourcePath });
  showFlamePanel(loaded.value.model, loaded.value.summary, sourcePath);
  heat.apply(loaded.value.summary);
  return true;
}

/** Runs the profiler CLI and resolves with its outcome; must honor the token. */
export type ProgressRunner = (
  exec: (token: vscode.CancellationToken) => Promise<ExecResult>,
) => Thenable<ExecResult>;

const PROGRESS_TITLE = "Osprey: profiling…";

/**
 * The default ProgressRunner: a cancellable notification whose Cancel button
 * feeds the token through runCompiler, which kills the whole process tree —
 * so a hung profiled program can always be stopped from the UI.
 */
export const runWithProfilingProgress: ProgressRunner = (exec) =>
  vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: PROGRESS_TITLE,
      cancellable: true,
    },
    (_progress, token) => exec(token),
  );

/**
 * The command body, with the compiler injectable so tests can point it at a
 * stub CLI, and the progress runner injectable so tests can drive
 * cancellation. Resolves true when a flame panel was opened.
 */
export async function profileActiveFile(
  compiler: string,
  output: vscode.OutputChannel,
  heat: HeatDecorationManager,
  remember: (run: LastRun) => void,
  progress: ProgressRunner = runWithProfilingProgress,
): Promise<boolean> {
  const editor = vscode.window.activeTextEditor;
  if (editor === undefined || !isOspreyFile(editor.document.fileName)) {
    void vscode.window.showInformationMessage(
      "Open a .osp or .ospml file to profile.",
    );
    return false;
  }
  await editor.document.save();
  const sourcePath = editor.document.fileName;
  const tmpDir = await fs.promises.mkdtemp(
    path.join(os.tmpdir(), "osprey-profile-"),
  );
  output.show(true);
  output.appendLine(`Profiling ${sourcePath} → ${tmpDir}`);
  heat.clear();
  let cancelled = false;
  const result = await progress((token) => {
    token.onCancellationRequested(() => {
      cancelled = true;
    });
    return runCompiler(
      compiler,
      [sourcePath, "--run", "--profile"],
      tmpDir,
      process.env,
      token,
    );
  });
  if (cancelled) {
    output.appendLine("Profiling cancelled.");
    return false;
  }
  appendProcessOutput(output, result);
  return finishRun(result, tmpDir, sourcePath, output, heat, remember);
}

/** Command ids — overridable ONLY so tests can register beside the real ones. */
export interface ProfilerCommandIds {
  profile: string;
  openLast: string;
}

export const PROFILER_COMMANDS: ProfilerCommandIds = {
  profile: "osprey.profileCurrentFile",
  openLast: "osprey.profiler.openLast",
};

/** Register the profiler commands, heat manager, and config/editor listeners. */
export function registerProfilerCommands(
  context: vscode.ExtensionContext,
  resolveCompiler: () => string,
  ids: ProfilerCommandIds = PROFILER_COMMANDS,
): HeatDecorationManager {
  const output = vscode.window.createOutputChannel("Osprey Profiler");
  const heat = new HeatDecorationManager();
  let lastRun: LastRun | undefined;
  context.subscriptions.push(
    output,
    { dispose: () => heat.dispose() },
    vscode.window.onDidChangeVisibleTextEditors(() => heat.refresh()),
    vscode.workspace.onDidChangeConfiguration((event) => {
      if (event.affectsConfiguration("osprey.profiler.inlineHeat")) {
        heat.refresh();
      }
    }),
    vscode.commands.registerCommand(ids.profile, () =>
      profileActiveFile(resolveCompiler(), output, heat, (run) => {
        lastRun = run;
      }),
    ),
    vscode.commands.registerCommand(ids.openLast, () => {
      if (lastRun === undefined) {
        void vscode.window.showInformationMessage(
          'No profile captured yet — run "Osprey: Profile Current File" first.',
        );
        return false;
      }
      showFlamePanel(lastRun.model, lastRun.summary, lastRun.sourcePath);
      return true;
    }),
  );
  return heat;
}

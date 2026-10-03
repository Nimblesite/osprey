import * as path from "path";
import * as fs from "fs";
import { workspace, ExtensionContext } from "vscode";
import { CloseAction, ErrorAction, LanguageClientOptions } from "vscode-languageclient/node";

// shipwrightPlatform maps the Node platform/arch to the Shipwright platform id
// (e.g. darwin-arm64, win32-x64) used in the bundled binary path. Both inputs
// are parameters rather than direct `process` reads so every arm of the mapping
// is reachable from a unit test: a VSIX is staged for platforms CI never runs
// on, and a mapping that is only ever exercised for the host's own triple is
// the half of this function most likely to be wrong.
export function shipwrightPlatform(
  platform: NodeJS.Platform = process.platform,
  arch: string = process.arch,
): string {
  const cpu = arch === "arm64" ? "arm64" : "x64";
  const os =
    platform === "win32" ? "win32" : platform === "darwin" ? "darwin" : "linux";
  return `${os}-${cpu}`;
}

// resolveBundledCompiler returns the absolute path to the version-matched
// osprey binary bundled in this VSIX for the current platform, or undefined
// when running unbundled (e.g. a local dev install). The release pipeline
// stages it at bin/<platform>/osprey[.exe]. [EDITOR-VERSIONING] Exported so
// both the bundled-present and unbundled branches can be unit tested.
export function resolveBundledCompiler(
  context: ExtensionContext,
): string | undefined {
  const exe = process.platform === "win32" ? ".exe" : "";
  const bundled = context.asAbsolutePath(
    path.join("bin", shipwrightPlatform(), `osprey${exe}`),
  );
  return fs.existsSync(bundled) ? bundled : undefined;
}

// looksLikePath reports whether a configured compiler value is a filesystem
// path (absolute or relative) rather than a bare command name resolved on PATH.
// Only path-like values are existence-checked; a bare `osprey` is left for the
// OS to resolve at spawn time.
export function looksLikePath(value: string): boolean {
  return value.includes("/") || value.includes("\\");
}

// resolveServerCommand picks the osprey binary that backs the language server:
// an explicit user setting, then the version-matched bundled compiler, then a
// plain `osprey` on PATH. The server is launched as `<command> lsp` over stdio.
// A configured path that points at a MISSING file would make the language
// client fail to spawn (ENOENT) and silently kill every feature — hover,
// diagnostics, go-to-definition. Rather than die, fall back to the bundled/PATH
// compiler and warn. `warn` is injectable so the fallback branch is unit
// testable; it defaults to a no-op. Exported so each branch is unit tested
// independently of a single live activation. [EDITOR-VSCODE],
// [EDITOR-VERSIONING]
export function resolveServerCommand(
  context: ExtensionContext,
  warn: (message: string) => void = () => undefined,
): string {
  const config = workspace.getConfiguration("osprey");
  const userPath =
    config.get<string>("server.compilerPath") ||
    config.get<string>("server.path");
  if (userPath) {
    if (looksLikePath(userPath) && !fs.existsSync(userPath)) {
      const fallback = resolveBundledCompiler(context) ?? "osprey";
      warn(
        `osprey.server.compilerPath "${userPath}" does not exist; ` +
          `falling back to "${fallback}". Run \`make build\` to produce it.`,
      );
      return fallback;
    }
    return userPath;
  }
  return resolveBundledCompiler(context) ?? "osprey";
}

// makeClientFailureHandling builds the language client's failure callbacks: the
// one-shot initialization-failed handler and the runtime error/closed handlers
// that keep the server alive (Continue) or restart it (Restart). These fire only
// on real LSP transport failures, which an integration test cannot reliably
// induce — so they are extracted here and the side effects (`log`, `showError`)
// are injected, letting each callback be unit-tested directly. Behaviour is
// identical to the previous inline handlers.
export function makeClientFailureHandling(
  log: (message: string) => void,
  showError: (message: string) => void,
): Pick<LanguageClientOptions, "initializationFailedHandler" | "errorHandler"> {
  return {
    initializationFailedHandler: (error) => {
      log(`Initialization failed: ${error}`);
      showError(`Osprey language server initialization failed: ${error}`);
      return false;
    },
    errorHandler: {
      error: (error, message, count) => {
        log(
          `Language server error: ${error}, message: ${message}, count: ${count}`,
        );
        return { action: ErrorAction.Continue };
      },
      closed: () => {
        log("Language server connection closed; restarting");
        return { action: CloseAction.Restart };
      },
    },
  };
}

// A minimal stand-in for the active editor the debug provider reads — just the
// document fields the synthesis needs.
export interface ActiveEditorLike {
  document: { languageId: string; fileName: string };
}

// Osprey ships two surface flavors that share one compiler, language server,
// and debug pipeline: the brace flavor (.osp, languageId "osprey") and the ML
// layout flavor (.ospml, languageId "osprey-ml"). The CLI selects the flavor
// from the file extension, so every editor-side filter must accept BOTH — never
// hard-filter to ".osp"/"osprey" alone or ML files silently lose their UX.
const OSPREY_LANGUAGE_IDS = ["osprey", "osprey-ml"];
const OSPREY_FILE_EXTENSIONS = [".osp", ".ospml"];

export function isOspreyFile(fileName: string): boolean {
  return OSPREY_FILE_EXTENSIONS.some((ext) => fileName.endsWith(ext));
}

function isOspreyLanguageId(languageId: string): boolean {
  return OSPREY_LANGUAGE_IDS.includes(languageId);
}

// ospreyLanguageForFile maps an Osprey source file to its VS Code language id —
// the ML layout flavor (.ospml) to "osprey-ml", the brace flavor (.osp) to
// "osprey" — or undefined for a non-Osprey file. ".ospml" is checked first
// because it also ends with "osp". Exported so the mapping is unit-testable.
export function ospreyLanguageForFile(fileName: string): string | undefined {
  if (fileName.endsWith(".ospml")) {
    return "osprey-ml";
  }
  if (fileName.endsWith(".osp")) {
    return "osprey";
  }
  return undefined;
}

function isOspreyDocument(document: ActiveEditorLike["document"]): boolean {
  return (
    isOspreyLanguageId(document.languageId) || isOspreyFile(document.fileName)
  );
}

// applyDefaultOspreyDebugConfig fills an otherwise-empty launch config from the
// active osprey editor, so pressing Run with no `.vscode/launch.json` still
// works ([EDITOR-VSCODE]). It mutates and returns `config`: synthesis happens
// only when type/request/name are all absent AND an osprey document is focused;
// any already-populated config is returned untouched. Pure (no VS Code globals)
// so the debug provider's branches are unit-testable without a debug session.
export function applyDefaultOspreyDebugConfig(
  config: any,
  activeEditor: ActiveEditorLike | undefined,
): any {
  if (!config.type && !config.request && !config.name) {
    if (activeEditor && isOspreyDocument(activeEditor.document)) {
      config.type = "osprey";
      config.name = "Debug Osprey File";
      config.request = "launch";
      config.program = activeEditor.document.fileName;
      config.cwd = path.dirname(activeEditor.document.fileName);
    }
  }
  return config;
}

export function defaultOspreyDebugConfigForEditor(
  activeEditor: ActiveEditorLike | undefined,
): any {
  return applyDefaultOspreyDebugConfig({}, activeEditor);
}

export function defaultDebugOutputPath(program: string): string {
  const exe = process.platform === "win32" ? ".exe" : "";
  return path.join(
    path.dirname(program),
    ".osprey-debug",
    `${path.basename(program, path.extname(program))}${exe}`,
  );
}

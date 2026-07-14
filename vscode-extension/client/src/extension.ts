import * as path from "path";
import {
  workspace,
  ExtensionContext,
  window,
  commands,
  debug,
  DebugAdapterExecutable,
  languages,
} from "vscode";
import { execFile, execFileSync } from "child_process";
import * as fs from "fs";
import {
  CloseAction,
  ErrorAction,
  Executable,
  LanguageClient,
  LanguageClientOptions,
  RevealOutputChannelOn,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";
import { registerOspreyDebugPanel } from "./debug-panel";
import { registerOspreyTestExplorer } from "./test-explorer";

// @nimblesite/shipwright-vscode is ESM-only; this extension is CommonJS, so it
// is loaded via dynamic import() (never a static require) inside activate().

let client: LanguageClient;

// shipwrightPlatform maps the Node platform/arch to the Shipwright platform id
// (e.g. darwin-arm64, win32-x64) used in the bundled binary path. Exported for
// unit testing of the platform-string mapping.
export function shipwrightPlatform(): string {
  const arch = process.arch === "arm64" ? "arm64" : "x64";
  const os =
    process.platform === "win32"
      ? "win32"
      : process.platform === "darwin"
        ? "darwin"
        : "linux";
  return `${os}-${arch}`;
}

// resolveBundledCompiler returns the absolute path to the version-matched
// osprey binary bundled in this VSIX for the current platform, or undefined
// when running unbundled (e.g. a local dev install). The release pipeline
// stages it at bin/<platform>/osprey[.exe]. [SWR-VERSION-MANIFEST] Exported so
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
// independently of a single live activation.
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

export interface LldbDapResolutionHost {
  env?: NodeJS.ProcessEnv;
  existsSync?: (filePath: string) => boolean;
  execFileSync?: (
    command: string,
    args: readonly string[],
    options: { encoding: BufferEncoding },
  ) => string | Buffer;
  getSetting?: () => string | undefined;
  platform?: NodeJS.Platform;
}

function findExecutableOnPath(
  command: string,
  env: NodeJS.ProcessEnv,
  existsSync: (filePath: string) => boolean,
): string | undefined {
  for (const dir of (env.PATH ?? "").split(path.delimiter)) {
    if (!dir) {
      continue;
    }
    const candidate = path.join(dir, command);
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  return undefined;
}

export function resolveLldbDapCommand(
  config: any = {},
  host: LldbDapResolutionHost = {},
): string {
  const platform = host.platform ?? process.platform;
  const lldbDapName = platform === "win32" ? "lldb-dap.exe" : "lldb-dap";
  return resolveLldbDapExecutable(config, host) ?? lldbDapName;
}

export function resolveLldbDapExecutable(
  config: any = {},
  host: LldbDapResolutionHost = {},
): string | undefined {
  const setting = host.getSetting
    ? host.getSetting()
    : workspace.getConfiguration("osprey").get<string>("debug.lldbDapPath");
  const configured = config.lldbDapPath || setting;
  const platform = host.platform ?? process.platform;
  const existsSync = host.existsSync ?? fs.existsSync;
  const env = host.env ?? process.env;
  const lldbDapName = platform === "win32" ? "lldb-dap.exe" : "lldb-dap";
  const legacyName = platform === "win32" ? "lldb-vscode.exe" : "lldb-vscode";
  if (configured) {
    if (looksLikePath(configured)) {
      return existsSync(configured) ? configured : undefined;
    }
    return findExecutableOnPath(configured, env, existsSync);
  }

  const onPath =
    findExecutableOnPath(lldbDapName, env, existsSync) ??
    findExecutableOnPath(legacyName, env, existsSync);
  if (onPath) {
    return onPath;
  }

  if (platform === "darwin") {
    try {
      const xcrun = host.execFileSync ?? execFileSync;
      const resolved = String(
        xcrun("xcrun", ["-f", "lldb-dap"], { encoding: "utf8" }),
      ).trim();
      if (resolved && existsSync(resolved)) {
        return resolved;
      }
    } catch {
      // Fall through to common install locations.
    }
  }

  const commonCandidates =
    platform === "win32"
      ? [
          "C:\\Program Files\\LLVM\\bin\\lldb-dap.exe",
          "C:\\Program Files\\LLVM\\bin\\lldb-vscode.exe",
        ]
      : [
          "/opt/homebrew/opt/llvm/bin/lldb-dap",
          "/usr/local/opt/llvm/bin/lldb-dap",
          "/usr/bin/lldb-dap",
          "/opt/homebrew/opt/llvm/bin/lldb-vscode",
          "/usr/local/opt/llvm/bin/lldb-vscode",
          "/usr/bin/lldb-vscode",
        ];
  return commonCandidates.find(existsSync);
}

export function missingLldbDapMessage(config: any = {}): string {
  const configured = config.lldbDapPath
    ? ` Configured lldbDapPath: ${config.lldbDapPath}.`
    : "";
  return (
    "lldb-dap was not found. Install LLVM/LLDB or set osprey.debug.lldbDapPath " +
    "to an existing lldb-dap executable. Checked launch config, VS Code setting, PATH, " +
    `xcrun, and common LLVM install paths.${configured}`
  );
}

function compileDebugProgram(
  compilerCommand: string,
  sourceProgram: string,
  debugOutput: string,
  cwd: string,
  log: (message: string) => void,
): Promise<void> {
  fs.mkdirSync(path.dirname(debugOutput), { recursive: true });
  return new Promise((resolve, reject) => {
    execFile(
      compilerCommand,
      [sourceProgram, "--debug", "--compile", "-o", debugOutput],
      { cwd },
      (error: any, stdout: any, stderr: any) => {
        if (stdout) {
          log(stdout);
        }
        if (stderr) {
          log(stderr);
        }
        if (error) {
          reject(
            new Error(
              `Osprey debug build failed with exit code ${error.code || "unknown"}`,
            ),
          );
          return;
        }
        resolve();
      },
    );
  });
}

export function activate(context: ExtensionContext) {
  console.log("Osprey extension is now active!");

  // Create output channel for diagnostics
  const outputChannel = window.createOutputChannel("Osprey Debug");
  outputChannel.appendLine("=== Osprey Extension Activation ===");
  outputChannel.show();

  // Native Test Explorer integration ([TESTING-VSCODE]). Registered before the
  // server-enabled gate: test discovery/running only needs the compiler CLI, so
  // it must keep working even with the LSP disabled.
  registerOspreyTestExplorer(context, () => resolveServerCommand(context));

  // Check if Osprey server is enabled
  const config = workspace.getConfiguration("osprey");
  if (!config.get("server.enabled", true)) {
    outputChannel.appendLine("Language server is disabled in configuration");
    return;
  }

  // Shipwright: verify the bundled osprey compiler matches the version this
  // extension expects before we launch it for diagnostics. On mismatch the
  // host surfaces a prompt-reinstall message (hosts.vscode.onMismatch).
  // [SWR-VERSION-HANDSHAKE] Best-effort: never block activation on it.
  const manifestPath = context.asAbsolutePath("shipwright.json");
  if (fs.existsSync(manifestPath)) {
    // Adapter normalizing VS Code's Thenable-returning API to the Promise-typed
    // shape the library expects (VscodeApiLike).
    const vscodeApi = {
      workspace: {
        getConfiguration: (s?: string) => workspace.getConfiguration(s),
      },
      window: {
        showErrorMessage: (
          m: string,
          o: { modal: boolean },
          ...items: string[]
        ) => Promise.resolve(window.showErrorMessage(m, o, ...items)),
        showWarningMessage: (
          m: string,
          o: { modal: boolean },
          ...items: string[]
        ) => Promise.resolve(window.showWarningMessage(m, o, ...items)),
      },
    };
    void (async () => {
      try {
        const sw = await import("@nimblesite/shipwright-vscode");
        const r = await sw.activateShipwright(context, {
          vscode: vscodeApi,
          manifestPath,
          showMessages: true,
        });
        outputChannel.appendLine(
          `Shipwright activation: ok=${r.ok}, diagnostics=${r.diagnostics.length}`,
        );
      } catch (e) {
        outputChannel.appendLine(`Shipwright activation error: ${e}`);
      }
    })();
  }

  // The language server is the Rust `osprey lsp` subcommand (the osprey-lsp
  // crate, built on the published lspkit crates), spoken over stdio. Resolve
  // the binary: explicit user setting first, then the version-matched bundled
  // compiler, then `osprey` on PATH.
  const ospreyCommand = resolveServerCommand(context, (m) => {
    outputChannel.appendLine(m);
    window.showWarningMessage(m);
  });
  outputChannel.appendLine(`Language server command: ${ospreyCommand} lsp`);

  const serverExecutable: Executable = {
    command: ospreyCommand,
    args: ["lsp"],
    transport: TransportKind.stdio,
  };
  const serverOptions: ServerOptions = {
    run: serverExecutable,
    debug: serverExecutable,
  };

  // Client options. The server analyzes document text (not the filesystem), so
  // unsaved `untitled:` buffers are supported alongside on-disk files.
  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "osprey" },
      { scheme: "untitled", language: "osprey" },
      { scheme: "file", language: "osprey-ml" },
      { scheme: "untitled", language: "osprey-ml" },
    ],
    synchronize: {
      fileEvents: workspace.createFileSystemWatcher("**/*.osp{,ml}"),
    },
    outputChannelName: "Osprey Language Server",
    revealOutputChannelOn: RevealOutputChannelOn.Error,
    ...makeClientFailureHandling(
      (message) => outputChannel.appendLine(message),
      (message) => {
        window.showErrorMessage(message);
      },
    ),
  };

  // Create and start the language client
  client = new LanguageClient(
    "ospreyLanguageServer",
    "Osprey Language Server",
    serverOptions,
    clientOptions,
  );

  outputChannel.appendLine("Starting language client...");

  // Start the client and server
  client
    .start()
    .then(() => {
      outputChannel.appendLine(
        "SUCCESS: Osprey language server started successfully",
      );
      console.log("Osprey language server started successfully");
    })
    .catch((error: any) => {
      const errorMsg = `Failed to start Osprey language server: ${error.message || error}`;
      outputChannel.appendLine(`ERROR: ${errorMsg}`);
      outputChannel.appendLine(
        `Error stack: ${error.stack || "No stack trace"}`,
      );
      console.error("Failed to start Osprey language server:", error);
      window.showErrorMessage(errorMsg);
    });

  // Add status bar item
  const statusBar = window.createStatusBarItem();
  statusBar.text = "$(check) Osprey";
  statusBar.tooltip = "Osprey Language Server is running";
  statusBar.show();
  context.subscriptions.push(statusBar);

  // Register debug adapter
  const provider = debug.registerDebugAdapterDescriptorFactory("osprey", {
    createDebugAdapterDescriptor(session: any) {
      const command = resolveLldbDapExecutable(session?.configuration);
      if (!command) {
        const message = missingLldbDapMessage(session?.configuration);
        outputChannel.appendLine(message);
        void window.showErrorMessage(message);
        return undefined;
      }
      return new DebugAdapterExecutable(command);
    },
  });

  context.subscriptions.push(provider);

  // Register the Osprey Debug panel (call stack, locals, program details, and
  // the reserved CPU/memory profiling surfaces). It tracks the live session and
  // refreshes on every stop.
  registerOspreyDebugPanel(context);

  // Register debug configuration provider
  context.subscriptions.push(
    debug.registerDebugConfigurationProvider("osprey", {
      async resolveDebugConfiguration(folder: any, config: any, token: any) {
        // If no config is provided, synthesize one from the active osprey editor.
        config = applyDefaultOspreyDebugConfig(config, window.activeTextEditor);

        if (!config.program) {
          return window
            .showInformationMessage("Cannot find a program to run")
            .then((_) => {
              return undefined;
            });
        }

        const sourceProgram = config.program;
        const cwd = config.cwd || path.dirname(sourceProgram);
        const debugOutput =
          config.debugOutput || defaultDebugOutputPath(sourceProgram);
        const document = workspace.textDocuments.find(
          (d) => d.fileName === sourceProgram,
        );
        if (document && document.isDirty) {
          const saved = await document.save();
          if (!saved) {
            window.showErrorMessage("Save the Osprey file before debugging.");
            return undefined;
          }
        }

        outputChannel.appendLine(
          `Debug build: ${sourceProgram} -> ${debugOutput}`,
        );
        try {
          await compileDebugProgram(
            resolveServerCommand(context),
            sourceProgram,
            debugOutput,
            cwd,
            (message) => outputChannel.appendLine(message),
          );
        } catch (error: any) {
          const msg = error?.message || String(error);
          outputChannel.appendLine(msg);
          window.showErrorMessage(msg);
          return undefined;
        }

        return {
          ...config,
          type: "osprey",
          request: "launch",
          program: debugOutput,
          sourceProgram,
          cwd,
        };
      },
    }),
  );

  // Auto-detect and force language association for .osp and .ospml files. The
  // ML layout flavor (.ospml) binds to "osprey-ml"; the brace flavor (.osp) to
  // "osprey". ".ospml" must be tested before ".osp" because the former also
  // ends with "osp".
  workspace.onDidOpenTextDocument((document) => {
    outputChannel.appendLine(`📁 Document opened: ${document.fileName}`);
    const target = ospreyLanguageForFile(document.fileName);
    if (target && document.languageId !== target) {
      outputChannel.appendLine(
        `🔧 Forcing language association for ${document.fileName} (was: ${document.languageId})`,
      );
      // Use the proper API to set language
      languages.setTextDocumentLanguage(document, target).then(
        () => {
          outputChannel.appendLine(
            `✅ Successfully set language to ${target} for ${document.fileName}`,
          );
        },
        (error: any) => {
          outputChannel.appendLine(`❌ Failed to set language: ${error}`);
        },
      );
    }
  });

  // Check already open documents
  workspace.textDocuments.forEach((document) => {
    const target = ospreyLanguageForFile(document.fileName);
    if (target && document.languageId !== target) {
      outputChannel.appendLine(
        `🔧 Forcing language association for already open file: ${document.fileName}`,
      );
      languages.setTextDocumentLanguage(document, target);
    }
  });

  // Register commands
  context.subscriptions.push(
    commands.registerCommand("osprey.compile", () => {
      compileCurrentFile(resolveServerCommand(context));
    }),
    commands.registerCommand("osprey.run", () => {
      compileAndRunCurrentFile(resolveServerCommand(context));
    }),
    commands.registerCommand("osprey.debug", () => {
      void debugCurrentFile();
    }),
    commands.registerCommand("osprey.setLanguage", () => {
      const activeEditor = window.activeTextEditor;
      if (activeEditor) {
        const target =
          ospreyLanguageForFile(activeEditor.document.fileName) ?? "osprey";
        languages.setTextDocumentLanguage(activeEditor.document, target);
        window.showInformationMessage(
          target === "osprey-ml"
            ? "Set language to Osprey ML"
            : "Set language to Osprey",
        );
      }
    }),
    workspace.onDidChangeConfiguration((event: any) => {
      if (event.affectsConfiguration("osprey")) {
        window.showInformationMessage(
          "Osprey configuration changed. Restart required.",
        );
      }
    }),
  );
}

async function debugCurrentFile() {
  const config = defaultOspreyDebugConfigForEditor(window.activeTextEditor);
  if (!config.program) {
    window.showErrorMessage("Please open a .osp or .ospml file to debug");
    return;
  }
  await debug.startDebugging(undefined, config);
}

function compileCurrentFile(compilerCommand: string) {
  const activeEditor = window.activeTextEditor;
  if (!activeEditor) {
    window.showErrorMessage("No active Osprey file found");
    return;
  }

  const document = activeEditor.document;
  if (!isOspreyFile(document.fileName)) {
    window.showErrorMessage("Please open a .osp or .ospml file to compile");
    return;
  }

  // Save the file first
  document.save().then(() => {
    const outputChannel = window.createOutputChannel("Osprey Compiler");
    outputChannel.show();
    outputChannel.appendLine(`Compiling ${document.fileName}...`);

    // Get the directory containing the file (no workspace required)
    const fileDir = path.dirname(document.fileName);

    // Use the resolved osprey compiler (user setting → version-matched bundled
    // binary → `osprey` on PATH) — same resolution the language server uses.
    execFile(
      compilerCommand,
      [document.fileName],
      { cwd: fileDir },
      (error: any, stdout: any, stderr: any) => {
        outputChannel.appendLine(`=== COMPILATION OUTPUT ===`);

        if (stdout) {
          outputChannel.appendLine(`STDOUT:`);
          outputChannel.appendLine(stdout);
        }

        if (stderr) {
          outputChannel.appendLine(`STDERR:`);
          outputChannel.appendLine(stderr);
        }

        if (error) {
          outputChannel.appendLine(`ERROR:`);
          outputChannel.appendLine(`Exit code: ${error.code || "unknown"}`);
          outputChannel.appendLine(`Signal: ${error.signal || "none"}`);
          outputChannel.appendLine(`Error message: ${error.message}`);
          window.showErrorMessage(
            "Compilation failed. Check output for details.",
          );
        } else {
          outputChannel.appendLine("=== COMPILATION SUCCESS ===");
          window.showInformationMessage("Osprey file compiled successfully!");
        }

        outputChannel.appendLine(`=== END OUTPUT ===`);
      },
    );
  });
}

function compileAndRunCurrentFile(compilerCommand: string) {
  const activeEditor = window.activeTextEditor;
  if (!activeEditor) {
    window.showErrorMessage("No active Osprey file found");
    return;
  }

  const document = activeEditor.document;
  if (!isOspreyFile(document.fileName)) {
    window.showErrorMessage("Please open a .osp or .ospml file to run");
    return;
  }

  // Save the file first
  document.save().then(() => {
    const outputChannel = window.createOutputChannel("Osprey Runner");
    outputChannel.show();
    outputChannel.appendLine(`Compiling and running ${document.fileName}...`);

    // Get the directory containing the file (no workspace required)
    const fileDir = path.dirname(document.fileName);

    // Use the resolved osprey compiler with --run (user setting → version-matched
    // bundled binary → `osprey` on PATH) — same resolution the language server uses.
    execFile(
      compilerCommand,
      [document.fileName, "--run"],
      { cwd: fileDir },
      (error: any, stdout: any, stderr: any) => {
        outputChannel.appendLine(`=== COMPILE AND RUN OUTPUT ===`);

        if (stdout) {
          outputChannel.appendLine(`STDOUT:`);
          outputChannel.appendLine(stdout);
        }

        if (stderr) {
          outputChannel.appendLine(`STDERR:`);
          outputChannel.appendLine(stderr);
        }

        if (error) {
          outputChannel.appendLine(`ERROR:`);
          outputChannel.appendLine(`Exit code: ${error.code || "unknown"}`);
          outputChannel.appendLine(`Signal: ${error.signal || "none"}`);
          outputChannel.appendLine(`Error message: ${error.message}`);
          window.showErrorMessage(
            "Compilation or execution failed. Check output for details.",
          );
        } else {
          outputChannel.appendLine("=== SUCCESS ===");
          window.showInformationMessage(
            "Osprey program executed successfully!",
          );
        }

        outputChannel.appendLine(`=== END OUTPUT ===`);
      },
    );
  });
}

export function deactivate(): Promise<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

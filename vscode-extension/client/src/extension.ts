// Real DAP launch and LSP/DAP separation [DEBUGGER-EDITOR-LAUNCH]
// [DEBUGGER-PROTOCOLS].
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
import * as fs from "fs";
import {
  CodeActionRequest,
  Executable,
  LanguageClient,
  LanguageClientOptions,
  RevealOutputChannelOn,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";
import { registerOspreyDebugPanel } from "./debug-panel";
import { registerProfilerCommands } from "./profiler/profile-run";
import { registerTestDocsCommand } from "./test-docs-panel";
import { registerOspreyTestExplorer } from "./test-explorer";
import { registerTestDebugProfile } from "./test-debug";
import { registerTestProfileProfile } from "./test-profile";
import { registerWarningFixes, warningFixMiddleware } from "./warning-fixes";

import { resolveServerCommand, makeClientFailureHandling, defaultOspreyDebugConfigForEditor, applyDefaultOspreyDebugConfig, defaultDebugOutputPath, ospreyLanguageForFile } from "./client-config";
import { resolveLldbDapExecutable, missingLldbDapMessage, compileDebugProgram } from "./debug-config";
import { compileCurrentFile, compileAndRunCurrentFile } from "./compile-commands";
export * from "./client-config";
export * from "./debug-config";

// @nimblesite/shipwright-vscode is ESM-only; this extension is CommonJS, so it
// is loaded via dynamic import() (never a static require) inside activate().

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  console.log("Osprey extension is now active!");

  // Create output channel for diagnostics
  const outputChannel = window.createOutputChannel("Osprey Debug");
  outputChannel.appendLine("=== Osprey Extension Activation ===");

  // CPU profiler ([PROF-VSCODE-FLAME]): the Profile Current File command, the
  // interactive flame-graph webview, and inline heat decorations. Registered
  // before the server-enabled gate — it only needs the compiler CLI — and
  // first, so the Test Explorer's Profile run profile can reuse its heat
  // manager ([TESTING-PROFILE]).
  const heat = registerProfilerCommands(context, () =>
    resolveServerCommand(context),
  );

  // Native Test Explorer integration ([TESTING-VSCODE]). Also before the gate:
  // test discovery/running only needs the compiler CLI, so it must keep
  // working even with the LSP disabled.
  const controller = registerOspreyTestExplorer(context, () =>
    resolveServerCommand(context),
  );
  // The Profile run profile runs a suite under the sampling profiler and opens
  // its flame graph + heat decorations ([TESTING-PROFILE]); the documentation
  // command opens a case's `///` block ([TESTING-DOC]).
  registerTestProfileProfile(
    controller,
    () => resolveServerCommand(context),
    () => heat,
  );
  // The Debug run profile runs a suite under the Osprey debug adapter, so
  // breakpoints inside a `test(...)` body are live and the Testing view offers
  // **Debug Test** ([TESTING-DEBUG-VSCODE]).
  registerTestDebugProfile(context, controller, () =>
    resolveServerCommand(context),
  );
  registerTestDocsCommand(context);
  registerWarningFixes(context, (params) => client.sendRequest(CodeActionRequest.type, params));

  // Check if Osprey server is enabled
  const config = workspace.getConfiguration("osprey");
  if (!config.get("server.enabled", true)) {
    outputChannel.appendLine("Language server is disabled in configuration");
    return;
  }

  // Shipwright: verify the bundled osprey compiler matches the version this
  // extension expects before we launch it for diagnostics. On mismatch the
  // host surfaces a prompt-reinstall message (hosts.vscode.onMismatch).
  // [EDITOR-VERSIONING]. Best-effort: never block
  // activation on it.
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
  // compiler, then `osprey` on PATH. [EDITOR-VSCODE], [LSP-TRANSPORT]
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
    middleware: warningFixMiddleware,
    documentSelector: [
      { scheme: "file", language: "osprey" },
      { scheme: "untitled", language: "osprey" },
      { scheme: "file", language: "osprey-ml" },
      { scheme: "untitled", language: "osprey-ml" },
    ],
    synchronize: {
      fileEvents: [
        workspace.createFileSystemWatcher("**/*.osp{,ml}"),
        workspace.createFileSystemWatcher("**/osprey.toml"),
      ],
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
      async resolveDebugConfiguration(_folder: any, config: any, _token: any) {
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
            config.compilerPath || resolveServerCommand(context),
            sourceProgram,
            debugOutput,
            cwd,
            (message) => outputChannel.appendLine(message),
            config.preserveArtifacts === true,
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

export function deactivate(): Promise<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}

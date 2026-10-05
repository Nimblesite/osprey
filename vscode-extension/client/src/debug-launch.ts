// Native launch preparation shared by F5 and the Test Explorer. [DEBUGGER-EDITOR-LAUNCH]
import * as path from "path";
import { debug, window, workspace, type DebugConfiguration, type ExtensionContext, type OutputChannel } from "vscode";
import { applyDefaultOspreyDebugConfig, defaultDebugOutputPath } from "./client-config";
import { compileDebugProgram, missingLldbDapMessage, resolveLldbDapExecutable } from "./debug-config";
import { resolveDebugConsole } from "./debug-console";

export function registerDebugLaunch(context: ExtensionContext, compiler: () => string, log: OutputChannel): void {
  context.subscriptions.push(debug.registerDebugConfigurationProvider("osprey", {
    resolveDebugConfiguration: (_folder, config) => resolveLaunch(config, compiler, log),
  }));
}

async function resolveLaunch(config: DebugConfiguration, compiler: () => string, log: OutputChannel): Promise<DebugConfiguration | undefined> {
  config = applyDefaultOspreyDebugConfig(config, window.activeTextEditor);
  if (!config.program) {
    void window.showInformationMessage("Cannot find a program to run");
    return undefined;
  }
  try {
    await saveSource(config.program);
    return await compileLaunch(config, compiler, log);
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    log.appendLine(message);
    void window.showErrorMessage(message);
    return undefined;
  }
}

async function saveSource(program: string): Promise<void> {
  const document = workspace.textDocuments.find((item) => item.fileName === program);
  if (document?.isDirty && !(await document.save())) {
    throw new Error("Save the Osprey file before debugging.");
  }
}

async function compileLaunch(config: DebugConfiguration, compiler: () => string, log: OutputChannel): Promise<DebugConfiguration> {
  const sourceProgram = config.program;
  const cwd = config.cwd || path.dirname(sourceProgram);
  const output = config.debugOutput || defaultDebugOutputPath(sourceProgram);
  const lldbDapPath = resolveLldbDapExecutable(config);
  if (!lldbDapPath) throw new Error(missingLldbDapMessage(config));
  const console = await resolveDebugConsole({ console: config.console, runInTerminal: config.runInTerminal, launchCommands: config.launchCommands }, lldbDapPath);
  log.appendLine(`Debug build: ${sourceProgram} -> ${output}`);
  await compileDebugProgram(config.compilerPath || compiler(), sourceProgram, output, cwd,
    (message) => log.appendLine(message), config.preserveArtifacts === true);
  return { ...config, type: "osprey", request: "launch", program: output, sourceProgram, cwd, lldbDapPath, console };
}

import * as path from "path";
import { execFile } from "child_process";
import { window, OutputChannel } from "vscode";
import { isOspreyFile } from "./client-config";

// The two "compile the active file" commands (compile, compile-and-run) differ
// only in the compiler args and the strings they log; everything else — the
// active-editor / .osp guards, save, output channel and the STDOUT/STDERR/ERROR
// reporting — is one shared shape captured by `runCompileTask`.
interface CompileTask {
  fileVerb: string;
  channelName: string;
  startLine: (fileName: string) => string;
  args: (fileName: string) => string[];
  outputHeader: string;
  failureMessage: string;
  successHeader: string;
  successMessage: string;
}

function reportCompileOutput(
  channel: OutputChannel,
  task: CompileTask,
  error: any,
  stdout: any,
  stderr: any,
) {
  channel.appendLine(task.outputHeader);
  if (stdout) {
    channel.appendLine(`STDOUT:`);
    channel.appendLine(stdout);
  }
  if (stderr) {
    channel.appendLine(`STDERR:`);
    channel.appendLine(stderr);
  }
  if (error) {
    channel.appendLine(`ERROR:`);
    channel.appendLine(`Exit code: ${error.code || "unknown"}`);
    channel.appendLine(`Signal: ${error.signal || "none"}`);
    channel.appendLine(`Error message: ${error.message}`);
    window.showErrorMessage(task.failureMessage);
  } else {
    channel.appendLine(task.successHeader);
    window.showInformationMessage(task.successMessage);
  }
  channel.appendLine(`=== END OUTPUT ===`);
}

function runCompileTask(compilerCommand: string, task: CompileTask) {
  const activeEditor = window.activeTextEditor;
  if (!activeEditor) {
    window.showErrorMessage("No active Osprey file found");
    return;
  }
  const document = activeEditor.document;
  if (!isOspreyFile(document.fileName)) {
    window.showErrorMessage(
      `Please open a .osp or .ospml file to ${task.fileVerb}`,
    );
    return;
  }
  // Save the file first, then compile with the resolved osprey compiler (user
  // setting → version-matched bundled binary → `osprey` on PATH — same
  // resolution the language server uses).
  document.save().then(() => {
    const outputChannel = window.createOutputChannel(task.channelName);
    outputChannel.show();
    outputChannel.appendLine(task.startLine(document.fileName));
    const fileDir = path.dirname(document.fileName);
    const child = execFile(
      compilerCommand,
      task.args(document.fileName),
      { cwd: fileDir },
      (error: any, stdout: any, stderr: any) =>
        reportCompileOutput(outputChannel, task, error, stdout, stderr),
    );
    // Close stdin at once, exactly as the test explorer's spawn does. execFile
    // OPENS a stdin pipe and never ends it, so a program that reads stdin
    // (`input()`) parks in `read` forever: the run never finishes, and the
    // channel stays empty because stdout is block-buffered and never flushed.
    // An output-channel run has no keyboard attached to it, so EOF — not an
    // eternal wait — is the honest thing for the child to see.
    child.stdin?.end();
  });
}

export function compileCurrentFile(compilerCommand: string) {
  runCompileTask(compilerCommand, {
    fileVerb: "compile",
    channelName: "Osprey Compiler",
    startLine: (fileName) => `Compiling ${fileName}...`,
    args: (fileName) => [fileName],
    outputHeader: `=== COMPILATION OUTPUT ===`,
    failureMessage: "Compilation failed. Check output for details.",
    successHeader: "=== COMPILATION SUCCESS ===",
    successMessage: "Osprey file compiled successfully!",
  });
}

export function compileAndRunCurrentFile(compilerCommand: string) {
  runCompileTask(compilerCommand, {
    fileVerb: "run",
    channelName: "Osprey Runner",
    startLine: (fileName) => `Compiling and running ${fileName}...`,
    args: (fileName) => [fileName, "--run"],
    outputHeader: `=== COMPILE AND RUN OUTPUT ===`,
    failureMessage:
      "Compilation or execution failed. Check output for details.",
    successHeader: "=== SUCCESS ===",
    successMessage: "Osprey program executed successfully!",
  });
}

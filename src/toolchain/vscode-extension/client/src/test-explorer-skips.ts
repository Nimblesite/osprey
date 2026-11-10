// Skipped-test warnings ([TESTING-VSCODE]): a skipped leaf publishes a
// diagnostic on its test, cleared when it runs again or its file goes away.

import * as vscode from "vscode";
import { skipReportFor, type LeafOutcome, type SkipReport } from "./test-explorer-parse";

const SKIP_DIAGNOSTIC_CODE = "test-skipped";

let skipCollection: vscode.DiagnosticCollection | undefined;

const skipWarningsByFile = new Map<string, Map<string, vscode.Diagnostic>>();

function skipDiagnosticCollection(): vscode.DiagnosticCollection {
  skipCollection ??=
    vscode.languages.createDiagnosticCollection("osprey-test-skips");
  return skipCollection;
}

// A TestItem's range is a zero-width CURSOR at the `test(` call — correct for
// the gutter marker, useless as a diagnostic: an empty span renders no
// squiggle, so the warning would exist in the Problems panel and be invisible
// in the editor. Widen it to the rest of the line, matching what the language
// server underlines for the same case ([TESTING-SKIP-WARNING]).
const LINE_END = Number.MAX_SAFE_INTEGER;

function skipWarningRange(leaf: vscode.TestItem): vscode.Range {
  const start = leaf.range?.start ?? new vscode.Position(0, 0);
  const open = vscode.workspace.textDocuments.find(
    (doc) => doc.uri.toString() === leaf.uri?.toString(),
  );
  // VS Code clamps an over-long end column to the real line end, so an
  // unopened file still underlines exactly its `test(` line.
  const end =
    open === undefined
      ? LINE_END
      : Math.max(open.lineAt(start.line).text.length, start.character + 1);
  return new vscode.Range(start.line, start.character, start.line, end);
}

// A reasoned skip is a debt the reader can weigh, so it is a Warning. A skip
// that named no reason is a defect nobody can weigh, so it is an Error
// ([TESTING-SKIP-REASON]). Both carry the same code: one rule, two strengths.
function skipWarningDiagnostic(
  leaf: vscode.TestItem,
  report: SkipReport,
): vscode.Diagnostic {
  const diagnostic = new vscode.Diagnostic(
    skipWarningRange(leaf),
    report.message,
    report.unexplained
      ? vscode.DiagnosticSeverity.Error
      : vscode.DiagnosticSeverity.Warning,
  );
  diagnostic.source = "osprey tests";
  diagnostic.code = SKIP_DIAGNOSTIC_CODE;
  return diagnostic;
}

export function publishSkipState(leaf: vscode.TestItem, outcome: LeafOutcome): void {
  const uri = leaf.uri;
  if (uri === undefined) {
    return;
  }
  const key = uri.toString();
  const entries =
    skipWarningsByFile.get(key) ?? new Map<string, vscode.Diagnostic>();
  skipWarningsByFile.set(key, entries);
  const report = skipReportFor(leaf.label, outcome);
  if (report === undefined) {
    entries.delete(leaf.id);
  } else {
    entries.set(leaf.id, skipWarningDiagnostic(leaf, report));
  }
  skipDiagnosticCollection().set(uri, [...entries.values()]);
}

export function dropSkipWarnings(uri: vscode.Uri): void {
  skipWarningsByFile.delete(uri.toString());
  skipCollection?.delete(uri);
}

/** Drops every skipped-test warning and the collection that holds them. */
export function disposeSkipWarnings(): void {
  skipCollection?.dispose();
  skipCollection = undefined;
  skipWarningsByFile.clear();
}

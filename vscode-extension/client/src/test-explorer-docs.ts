// Pure rendering of a test case's documentation ([TESTING-DOC]). `///` blocks
// (Default) and `(** … *)` blocks (ML) written above a `test("name", …)` case
// travel through `--list-tests` as `summary` + `doc`; this module turns them
// into the strings the Test Explorer shows: the inline description beside the
// case name, the Markdown detail panel, and the documentation header prepended
// to a failure message. No `vscode` import — everything here unit-tests
// directly (mirroring test-explorer-parse.ts's pure/wiring split).

import type { DiscoveredTest } from "./test-explorer-parse";

/** The documentation known for one leaf, keyed by its TestItem id. */
export type TestDocs = ReadonlyMap<string, TestDoc>;

/** One test case's documentation, as discovery reported it. */
export interface TestDoc {
  /** The case's name, for headings. */
  readonly name: string;
  /** First paragraph, or "" when undocumented. */
  readonly summary: string;
  /** Full rendered Markdown, or "" when undocumented. */
  readonly markdown: string;
  /** 1-based declaration line, for the "defined at" footer. */
  readonly line: number;
}

/** Fold one discovered case into its doc record; absent fields become "". */
export function testDocOf(test: DiscoveredTest): TestDoc {
  return {
    name: test.name,
    summary: test.summary ?? "",
    markdown: test.doc ?? "",
    line: test.line,
  };
}

/** Whether a case carries any documentation at all. */
export function isDocumented(doc: TestDoc | undefined): boolean {
  return doc !== undefined && (doc.summary !== "" || doc.markdown !== "");
}

/** Collapse every run of whitespace to one space — a tree row is one line. */
function oneLine(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/**
 * The greyed text VS Code renders after the case name in the Test Explorer
 * tree — and, because `vscode.TestItem` carries no tooltip of its own, the
 * ONLY documentation its hover can show. So it carries the WHOLE doc comment,
 * not just the summary: the row truncates to the panel width either way, while
 * the hover gains every paragraph and section the author wrote. Describing
 * with the summary alone showed one line of a four-paragraph block and dropped
 * the rest ([TESTING-DOC]). Undocumented cases get `undefined` so the tree
 * stays clean, and the doc collapses to one line — the tree renders one row.
 */
export function testDescription(test: DiscoveredTest): string | undefined {
  const text = oneLine(test.doc ?? "") || oneLine(test.summary ?? "");
  return text === "" ? undefined : text;
}

/**
 * The Markdown detail shown for one case: its name as a heading, the rendered
 * doc comment, and a footer naming the file and line it is declared on. An
 * undocumented case still produces a block — it states that no `///`
 * documentation was written, which is more useful than an empty panel.
 */
export function testDocMarkdown(doc: TestDoc, filePath: string): string {
  const body =
    doc.markdown === ""
      ? "_No documentation. Write a `///` block directly above the `test(...)` call to document this case._"
      : doc.markdown;
  return [
    `### ${doc.name}`,
    "",
    body,
    "",
    "---",
    "",
    `Declared at \`${filePath}:${doc.line}\``,
  ].join("\n");
}

/**
 * A failed case's message, with its documentation ahead of the failure so the
 * reader sees what the case was meant to prove before what went wrong. The
 * "Context For AI" block keeps the machine-readable fields, now including the
 * case's own documentation ([TESTING-DOC]).
 */
export function failureMarkdown(
  failure: string,
  doc: TestDoc | undefined,
  filePath: string,
  location: string,
): string {
  const documented = isDocumented(doc);
  const heading =
    documented && doc !== undefined ? [doc.markdown, "", "---", ""] : [];
  return [
    ...heading,
    failure,
    "",
    "## Context For AI",
    "",
    `- File: ${filePath}`,
    `- Test: ${doc?.name ?? "unknown"}`,
    "- Status: failed",
    `- Location: ${location}`,
    `- Failure: ${failure}`,
    `- Documentation: ${documented && doc !== undefined ? doc.markdown : "none"}`,
  ].join("\n");
}

/**
 * The header a profiling run appends to the run output before the TAP stream,
 * naming the suite and where its profile artifacts landed
 * ([TESTING-PROFILE]).
 */
export function profileRunHeader(filePath: string, dir: string): string {
  return `# profiling ${filePath}\n# profile artifacts: ${dir}\n`;
}

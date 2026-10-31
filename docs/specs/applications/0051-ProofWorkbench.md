# Proof Workbench

> **STOP — NOT YET IN FORCE.** This document was rewritten on 2026-10-31 for the `.lean`-only direction of [PROOF-SOURCE](../language/0049-DependentTypesAndProofs.md#proof-source-proof-source) while the `feat/type-disciplines` PR is still open. **Any agent working on that PR ignores this rewrite until the PR has merged.** Do not delete, rename, rewrite or stop maintaining any code, test, fixture or screen because of this text before then; the previous revision in git history is the contract for that PR. After the merge, [plan 0044](../../plans/0044-lean-source-compatibility.md) is the order of work.

**Status:** rewritten for `.lean`; the application at `apps/workbench/` still carries the deleted formats and is brought into line by [plan 0044](../../plans/0044-lean-source-compatibility.md), then [plan 0042](../../plans/0042-proof-workbench.md). Learn mode: [spec 0052](0052-WorkbenchLessons.md). The workbench depends on the proof kernel and tooling of [spec 0049](../language/0049-DependentTypesAndProofs.md) for its verdicts and on [`@osprey/native`](../../../packages/native/README.md) for everything it draws; it adds nothing to either that another editor could not use.

## Aim [WORKBENCH-AIM]

The workbench opens, edits, checks, reads and saves **`.lean` files**. The document is a Lean 4 source file exactly as the pinned toolchain accepts it ([PROOF-SOURCE-LEAN]); a file written in the workbench checks unchanged under `osprey prove check` and under Lean, and a file written anywhere else opens in the workbench. There is no other document format, no generated file and no translation step.

The workbench is one Osprey application over `@osprey/native`: one store, one `update`, one set of regions, running unchanged inside VS Code, in a browser and as a native desktop and tablet application. The hosts differ only in what draws them.

## Deleted formats [WORKBENCH-DELETED]

Before 2026-10-31 the workbench had its own English-sentence proof document (`Theorem and_swap. For all p q, …`, `Assume h : …`, `By cases on h`, `From h`) read by `formula`, `statement`, `step` and `goals` modules, elaborated by a hand-written prelude and `verdict` module, and written out by an `emit` module as a `// osprey: types=lean4` module with a tactic block and `sorry` holes. Both that document and that output are **deleted and illegal** ([PROOF-SOURCE-DELETED]). The modules, their tests, goldens and fixtures, the phrase table, the goal engine and the impredicative prelude are contamination and are removed by plan 0044. Nothing in this document, in lessons or in any host may read, write, display or accept them.

## Layout [WORKBENCH-LAYOUT]

The application lives at [`apps/workbench/`](../../../apps/workbench/README.md), ML flavor, namespace `workbench`, source roots `["src", "../../packages/native/src"]`.

| Path | Module | Owns |
| --- | --- | --- |
| `src/notation.ospml` | `Notation` | The Lean abbreviation table (`\forall` → `∀`) and the symbol palette ([WORKBENCH-NOTATION]) |
| `src/document.ospml` | `Document` | The `.lean` text, cursor, per-declaration ranges found by Lean's server, save and open ([WORKBENCH-DOCUMENT]) |
| `src/lean.ospml` | `Lean` | The toolchain adapter: check, export, goals and diagnostics through the capabilities of [WORKBENCH-CHECK] |
| `src/verdict.ospml` | `Verdicts` | Osprey's kernel verdict over the file's export ([WORKBENCH-VERDICT]) |
| `src/imported.ospml`, `src/proofterm.ospml` | `ImportedProofs`, `ProofTerms` | Checking a bundled export and presenting a checked statement as a `Math` tree ([WORKBENCH-LIBRARY]) |
| `src/theme.ospml` | `Palettes` | The `paper` and `chalk` themes ([WORKBENCH-DESIGN-COLOUR]) |
| `src/lesson.ospml`, `src/judge.ospml`, `src/tutor.ospml`, `lessons/` | `Lessons`, `Judge`, `Tutor` | Learn mode ([spec 0052](0052-WorkbenchLessons.md)) |
| `src/library.ospml` | `Library` | Search over the checked environment by name and shape ([WORKBENCH-LIBRARY]) |
| `src/app.ospml` | `App` | The store, `update`, the regions and the theme ([WORKBENCH-STORE], [WORKBENCH-REGIONS]) |
| `src/main.ospml` | entry | `Runtime::start` and `Runtime::dispatch` for native hosts; `Webview::respond` for VS Code and the browser |
| `tests/` | | The fixture-driven suite of [WORKBENCH-VERIFICATION] |

Every file under 500 lines, every function under 20, a documentation comment on every export.

## Notation [WORKBENCH-NOTATION]

The editor types Unicode as Lean's VS Code extension does: `\forall` `\all`, `\exists` `\ex`, `\to` `\r` `\->`, `\iff` `\lr`, `\and` `\wedge`, `\or` `\vee`, `\not` `\neg`, `\N` `\Z` `\R`, `\le` `\ge` `\ne`, `\in` `\notin` `\sub`, `\comp`, `\times`, `\inv`, `\_0`…`\_9`, `\^0`…`\^9`, `\alpha`…`\omega`. An abbreviation is replaced the moment a non-identifier character follows it; a trailing backslash shows the matching entries as a palette; the palette region lists every row. `Notation::expand : string -> string` is total and idempotent and the table is one Osprey list of `(abbreviation, symbol)` pairs. This is an input convenience, not a format: the buffer holds Lean source.

## Document [WORKBENCH-DOCUMENT]

A document is the text of one `.lean` file plus a cursor. `Document::open` takes the file's bytes; `Document::save` returns the text and nothing else. The workbench never rewrites, reformats or annotates the file: bytes in, the user's edits, bytes out. Declaration boundaries, `by` blocks and error ranges come from Lean's server ([WORKBENCH-CHECK]), never from a workbench parser. A new document starts as an empty file or the host's template (`import Mathlib` or a lesson's preamble).

## Checking [WORKBENCH-CHECK]

The workbench asks for four things and computes none of them itself:

| Service | Stage 1 (toolchain present) | Stage 2+ (Osprey front end) |
| --- | --- | --- |
| Diagnostics and declaration ranges | Lean's language server over the file | Osprey's server, same protocol |
| Goal state at the cursor | `$/lean/plainGoal` from Lean's server | same |
| Export of the checked file | pinned `lean4export` on the built module | Osprey's exporter |
| Osprey kernel verdict | `Verdicts::decide` on that export, in-process | same |

At stage 1 these run through a `Lean` host capability (`LeanCheck file`, `LeanGoals file position`), completed asynchronously with a revision ticket; a completion for an older revision is dropped. A host without the toolchain opens and saves files and shows every service as **Unavailable: Lean toolchain not found**; it never fills a badge. Checking never blocks typing.

## Verdict [WORKBENCH-VERDICT]

Per declaration, derived from the current revision only:

| Verdict | When | Shown |
| --- | --- | --- |
| `Checked` | Lean reports no error in the declaration and Osprey's kernel accepts its exported term | a filled badge beside the declaration |
| `Rejected message range` | Lean reports an error, or the kernel refuses the term (then the kernel's message, and the range is the declaration) | the range underlined, the message beside it |
| `Unchecked reason` | `sorry` or an unsolved goal in the declaration; or no toolchain; or the revision is still checking | an outlined badge and the reason |

A stale verdict is withdrawn on every edit to the declaration. The saved file never carries a verdict.

## Typeset mathematics [WORKBENCH-MATH]

`@osprey/native` has one content component, `Math`, carrying a typed layout tree (`Sym`, `Num`, `Seq`, `Sup`, `Sub`, `Frac`, `Root`, `Binder`, `Op`, `Paren`, `Big`); application code never writes TeX, MathML or HTML. The goal pane and the reading view build the tree from Lean's pretty-printed goals and statements through `ProofTerms`, and from a checked kernel term where one exists. Hosts lay the tree out with STIX Two Math or Latin Modern Math (system face as fallback) with journal spacing; the browser and VS Code hosts render MathML; every node is readable to assistive technology. The editor itself shows Lean source in a monospace face with Lean's semantic token colours.

## Mathematical input [WORKBENCH-BOARD]

The Workbench uses typed mathematics, with no drawing surface, handwriting workflow, drawing state or saved ink codec. Workspace format 2 stores only the document title and mathematical source; version 1 and files containing drawing data are explicitly refused without changing the current document or verdicts. Generic drawing and `Recognise` capabilities in `@osprey/native` remain independent SDK APIs.

## Library [WORKBENCH-LIBRARY]

`Library::search : (Library, Query) -> List<Hit>` answers by name (`and_comm`) and by statement shape (`?a ∧ ?b → ?b ∧ ?a`), matched by structural unification over checked kernel terms. Hover on a hit shows its statement as `Math` and its docstring; a hit inserts `exact <name>` or `apply <name>` at the cursor. The library is the checked environment of the pinned Mathlib once it exists; until then it is the bundled exports listed in `assets/proof-libraries.json`, each checked in-process by `ImportedProofs` from an empty environment, with corrupted proofs refused and per-theorem axioms shown separately from the export ledger. A pending or refused read clears the prior result. `ProofTerms` presents a checked statement with standard notation (`=`, `+`, `×`, numerals, projections) only when the exact declarations behind that notation are present in the checked environment, compared structurally; otherwise it shows the explicit constant.

## Store [WORKBENCH-STORE]

| Slice | Held | Contents |
| --- | --- | --- |
| `Text` | host | the `.lean` text and cursor |
| `Diagnostics` | session | Lean's diagnostics, declaration ranges and goal at the cursor, with their revision |
| `Verdicts` | session | verdict per declaration with its revision |
| `Library` | host | the environment and the current query |
| `Layout` | session | edit/read mode, open panes, splitter widths and theme |

`update` is pure; every host event is one message: `Edited text`, `Moved cursor`, `Chose symbol`, `Searched query`, `Inserted hit`, `Toggled pane`, `Resized width`, `Themed theme`, `Opened bytes`, `Saved`, plus the `Lean` completions and lifecycle phases.

## Regions [WORKBENCH-REGIONS]

| Region | Shows | Reads |
| --- | --- | --- |
| `root` | the editor with the goal pane, and navigation to reading, notation, examples and lessons | `Layout` |
| `document` | the `.lean` text in the `Editor` component with Lean's diagnostics as per-line marks and a verdict badge per declaration | `Text`, `Diagnostics`, `Verdicts` |
| `goal` | hypotheses and target at the cursor as `Math`; Lean's error for the current range | `Diagnostics`, `Text` |
| `read` | the file as a page ([WORKBENCH-PAPER]) | `Text`, `Diagnostics`, `Verdicts` |
| `library` | the search field, hits as rows, hover statement | `Library` |

A region that did not change is not sent; a keystroke refreshes `document` and `goal` and nothing else until a completion arrives. Learn mode adds the `catalogue`, `lesson` and `challenge:<lesson>/<id>` regions of [spec 0052](0052-WorkbenchLessons.md).

## Reads like a paper [WORKBENCH-PAPER]

The reading view lays the file out as a page: each declaration's statement as display mathematics under its name, its proof as the tactic lines with the goal after each in the margin, the verdict beside the heading, docstrings as prose. `Exported` writes the page as HTML with MathML through the same `Math` tree; `Saved` writes the `.lean` file.

## Hosts [WORKBENCH-HOSTS]

| Host | How |
| --- | --- |
| VS Code | [`@osprey/vscode`](../../../packages/vscode/README.md): the application compiled to WASI, the existing webview renderer; the `Lean` capability runs the pinned toolchain the extension finds or installs |
| Browser | the same renderer as a standalone page under `hosts/web/`; the `Lean` capability is a local service or Unavailable |
| macOS and iPadOS | the SwiftUI host under `hosts/apple/`, `--target=macos`, `ios`, `ios-sim`; the toolchain bundled with the macOS app, Unavailable on iPadOS until stage 2 |

One application, three hosts, proved by the same fixture-driven `update` tests; a host adds adapters and a renderer, never a decision.

### VSIX adapter [WORKBENCH-VSIX]

`Osprey: Open Workbench` (`osprey.workbench.open`) opens the application without requiring a project folder and reuses its panel. The VSIX bundles `apps/workbench/` compiled as `workbench.wasm` with the lesson catalogue, lesson `.lean` files, media and the bundled exports and licences staged by `scripts/workbench-proof-assets.py`; packaging rejects missing, invalid, stale or extra files. The package manager and workbench share the session, WASI runner, panel lifecycle, renderer and event transport; platform adapters only create the panel, map approved assets, read bounded files, run the `Lean` capability and store progress under the extension's own directory. No second proof or lesson implementation in TypeScript. VS Code's editor appearance selects the starting palette (`Dark`/`HighContrast` → `dark`, `Light`/`HighContrastLight` → `light`).

Acceptance uses the compiled application in the real webview: open and reuse the panel, open a `.lean` file, see Lean's goals and diagnostics, a `Checked` badge from the kernel, the actual rejection of a corrupted proof, lesson navigation, accessible MathML and progress persistence. The bundled-library gate of plan 0041 (the complete `Nat.add_comm` closure and the pinned OpenAI corpus checked in-process, corrupted proofs refused, axioms per root) stands; a CLI recording does not satisfy the host gate. The captures in [`docs/designs/proof-workbench/screens/`](../../designs/proof-workbench/screens/) are the visual oracle for every screen that survives the format change; screens that showed the deleted document or its generated pane are re-captured by plan 0042 before they bind.

## Beautiful, or not shipped [WORKBENCH-DESIGN]

The workbench has its own **Paper & Slate** identity. The Lean source editor and its goal pane are the working surface; Read is a continuous page. The implementation handoff is [the design system and workflow gallery](../../designs/proof-workbench/README.md); its screenshots are design references, not evidence.

### Colour [WORKBENCH-DESIGN-COLOUR]

Application-owned semantic tokens, sent through `@osprey/native`; a host does not substitute its own palette.

| Role | Paper | Slate / chalk | Meaning |
| --- | --- | --- | --- |
| `canvas` | `#FAFAF7` | `#202723` | Main working surface |
| `inset` | `#F0F2ED` | `#191F1C` | Quiet secondary surface |
| `surface` | `#FFFFFF` | `#29322D` | Open controls and contextual panes |
| `primary` | `#242923` | `#ECEDE5` | Mathematical content and main text |
| `secondary` | `#56615B` | `#B7C0B7` | Supporting text and labels |
| `accent` | `#36635C` | `#A6C5BB` | Focus, selected tool, deliberate action |
| `outline` | `#727C74` | `#7D8C80` | Required control boundaries |
| `divider` | `#D7DDD5` | `#465348` | Decorative separation only |
| `selection` | `#E4ECE5` | `#34473E` | Current line; paired with position or outline |
| `good` | `#315D3B` | `#B3CE94` | Explicit successful result, always labelled |
| `caution` | `#795719` | `#E6C077` | Pending or limited operation, always explained |
| `danger` | `#9A3935` | `#F0A69B` | Rejected declaration or Lean error |
| `dangerSurface` | `#F6E8E3` | `#453330` | Restrained diagnostic emphasis |
| `board` | `#202723` | `#202723` | Dark mathematical surface token |
| `chalk` | `#ECEDE5` | `#ECEDE5` | High-contrast foreground token |

Minimum measured contrast across `canvas`, `inset` and `surface`: primary text 13.15:1 Paper / 11.21:1 Slate; secondary 5.72:1 / 7.07:1; accent 6.02:1 / 7.13:1; semantic text at least 5.84:1 / 6.69:1; control outline 3.84:1 / 3.74:1. The suite tests the exact token pairs it renders. Decorative dividers are never the sole boundary of a control.

### Typography [WORKBENCH-DESIGN-TYPE]

STIX Two Text for prose and the reading view, STIX Two Math for formula layout, Geist for controls, the host monospace for the Lean editor. A host lacking STIX uses Latin Modern Math; never simulate maths with spaced monospace.

| Role | Desktop size / leading | Treatment |
| --- | --- | --- |
| Wordmark | 26 / 32 | STIX; italic "Proof", upright "Workbench" |
| Document title | 36–40 / 44 | STIX regular |
| Display formula | 36–44 / natural math metrics | STIX Two Math |
| Reading-view proof line | 22–24 / 34–38 | STIX |
| Hypothesis / current goal | 24 / 32 and 36 / 44 | Variables italic; operators upright |
| Controls and diagnostics | 14 / 20 | Geist regular or medium |
| Secondary label | 12 / 18 | Geist |
| Lean source | 14 / 23 | Host monospace; never the mathematics face |

### Surfaces, feedback and motion [WORKBENCH-DESIGN-SURFACES]

At desktop widths the editor leads with the goal pane alongside; on a tablet supplementary regions move below or into an explicitly opened sheet. 8-point rhythm with 4-point subdivisions; 24–32-point margins; 4–6-point corners and 1-point outlines; 44-point touch targets; 2-point accent focus outline with 3-point offset; colour never carries state alone.

| State | Presentation | Never imply |
| --- | --- | --- |
| Editing | Stable context, clear caret, restrained current-line tint | Acceptance of a line still being typed |
| Goal shown | One quiet transition as Lean's goal changes | A verdict from the goal pane |
| Lean error | Underline on Lean's range plus Lean's message in plain layout | That the rest of the file is clean without evidence |
| Checking | Stable heading; small labelled indicator; current revision | That an old check applies to edited text |
| Checked | Filled badge from the current kernel result only | A verdict for a declaration containing `sorry` |
| Unchecked | Outlined label and the reason | A success or a failure |

Motion: hover 120–150 ms colour only; pane reveal 220–260 ms with at most 12 points of travel; goal change 280–350 ms; edit/read switch 300–380 ms preserving content, focus and position; ease-out `(0.22, 1, 0.36, 1)`; text input immediate; reduced motion removes translation. The release bar is a well-set journal page with a usable `.lean` workflow: a real `and_swap` capture beside a printed page, light and dark, keyboard and notation input, and the toolchain-unavailable path.

## Nothing special for the workbench [WORKBENCH-NO-SPECIAL]

Every service the workbench uses is a capability of [PROOF-TOOLS](../language/0049-DependentTypesAndProofs.md#tooling-proof-tools) or a host capability of `@osprey/native`. The toolchain adapter, the library and the `ProofTerms` presenter are ordinary Osprey modules any editor may import.

## Verification [WORKBENCH-VERIFICATION]

`osprey apps/workbench/tests --run --quiet` and `python3 scripts/package_coverage.py --collect --project apps/workbench` are the suite, at the 90% floor in `coverage-thresholds.json`. Each case is a fixture through the real `start`/`dispatch`:

- every row of the notation table expands and is idempotent;
- open then save of every `.lean` fixture (`AndSwap.lean`, `OpenAIStacks.lean` and the rest of `benchmarks/proof/tools/`) is byte-identical;
- a `LeanCheck` completion for the current revision fills `Checked`; a stale completion is dropped; a Lean error underlines Lean's range; a corrupted export is `Rejected` by the kernel; `sorry` is `Unchecked`;
- no toolchain: every badge `Unchecked`, the reason shown, open and save still work;
- a keystroke refreshes exactly `document` and `goal`;
- the chalk and paper themes pass contrast on the exact pairs rendered;
- a source-only version 2 workspace survives open/save, while obsolete versions and drawing data are explicitly refused;
- `ProofTerms` presents `Nat.add_comm` with standard notation and refuses it for a same-named custom relation;
- no module, test, golden or lesson contains the deleted formats (a grep gate over `apps/workbench/`).

Each host adds a smoke test: VS Code through the extension's `compiled SDK renderer E2E`, the browser through a headless page, the native host through the Apple simulator, each opening `AndSwap.lean` and seeing a `Checked` badge with the toolchain present. The road test of plan 0041 is the acceptance: a mathematician new to Osprey writes and checks `and_swap` in a fresh `.lean` file in under ten minutes without documentation, recorded in `docs/reports/`.

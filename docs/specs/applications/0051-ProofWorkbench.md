# Proof Workbench

**Status:** the proof language is implemented (plan 0042 phase 1); the application is in progress. Delivery: [plan 0042](../../plans/0042-proof-workbench.md); Learn mode: [spec 0052](0052-WorkbenchLessons.md). The workbench depends on the proof kernel of [spec 0049](../language/0049-DependentTypesAndProofs.md) for its verdicts and on [`@osprey/native`](../../../packages/native/README.md) for everything it draws; it adds nothing to either that another editor could not use.

## Aim [WORKBENCH-AIM]

Mathematicians and scientists work on blackboards because no editor has been worth leaving one for. The workbench is that editor. A theorem is written as mathematics, in the words and symbols of a paper; a proof is written as the steps a person would say aloud; every step is checked as it is written; and the whole is read back as a typeset page. **The user writes no code.** The Osprey proof module of [PROOF-SURFACE-FLAVORS](../language/0049-DependentTypesAndProofs.md#proof-surface-proof-surface) is an output the workbench derives, shown beside the mathematics and saved as the file, so that `osprey prove check` on that file is the same check the workbench ran and no second tool needs the workbench.

The workbench is one Osprey application over `@osprey/native`: one store, one `update`, one set of regions, running unchanged inside VS Code, in a browser and as a native desktop and tablet application. The hosts differ only in what draws them.

The workbench opens on the mathematical document and introduces proofs through mathematics before requiring programming syntax or tactic names. Lean provides the development oracle for compatible proof semantics; Osprey's own kernel owns the verdict, following [PROOF-ORACLE](../language/0049-DependentTypesAndProofs.md#lean-as-a-development-oracle-proof-oracle). Saved documents and accepted verdicts must retain their discipline identity and lose stale acceptance when the checker, rules, assumptions or proof change. The [completion milestones](../../plans/0041-dependent-types-and-mathlib.md#completion-milestones-proof-completion) track the independent workflow; host design is evaluated through actual user tasks and rendered mathematics.

## Home and layout [WORKBENCH-LAYOUT]

The application lives at [`apps/workbench/`](../../../apps/workbench/README.md), ML flavor, namespace `workbench`, source roots `["src", "../../packages/native/src"]` as the native package's own tests do. `apps/` is the home of shipped Osprey applications; `examples/` stays for examples and `packages/` for libraries.

| Path | Module | Owns |
| --- | --- | --- |
| `src/notation.ospml` | `Notation` | The abbreviation table and symbol normalisation ([WORKBENCH-NOTATION]) |
| `src/formula.ospml` | `Formula` | The formula union, its reader from sentences and symbols, its Unicode writer and its typesetting ([WORKBENCH-FORMULA]) |
| `src/statement.ospml` | `Statement` | A theorem heading: name, variables, hypotheses, conclusion ([WORKBENCH-STATEMENT]) |
| `src/step.ospml` | `Step` | A proof step as phrased by the writer ([WORKBENCH-STEPS]) |
| `src/goals.ospml` | `Goals` | The goal engine: hypotheses and goal after each step, and why a step fails ([WORKBENCH-GOALS]) |
| `src/emit.ospml` | `Emit` | The generated `.ospml` text ([WORKBENCH-EMIT]) |
| `src/kernel.ospml` | `Closing`, `Propositions`, `Preludes` | Named terms, formulas as kernel propositions and the kernel-checked prelude ([WORKBENCH-VERDICT]) |
| `src/verdict.ospml` | `Elaborate`, `Verdicts` | Steps to one proof term and the kernel's verdict on it ([WORKBENCH-VERDICT]) |
| `src/imported.ospml`, `src/proofterm.ospml` | `ImportedProofs`, `ProofTerms` | Current exported theorem checks and shared presentation of its verified type ([WORKBENCH-VERDICT], [WORKBENCH-MATH]) |
| `src/theme.ospml` | `Palettes` | The `chalk` and `paper` themes ([WORKBENCH-DESIGN-COLOUR]) |
| `src/lesson.ospml`, `src/judge.ospml`, `src/tutor.ospml`, `lessons/` | `Lessons`, `Judge`, `Tutor` | Learn mode ([spec 0052](0052-WorkbenchLessons.md)) |
| `src/board.ospml` | `Boards` | Legacy saved board data; no drawing or handwriting UI ([WORKBENCH-BOARD]) |
| `src/library.ospml` | `Library` | Search over known theorems by name and shape ([WORKBENCH-LIBRARY]) |
| `src/app.ospml` | `App` | The store, `update`, the regions and the theme ([WORKBENCH-STORE], [WORKBENCH-REGIONS]) |
| `src/main.ospml` | entry | `Runtime::start` and `Runtime::dispatch` exported for native hosts; `Webview::respond` for the VS Code and browser hosts |
| `tests/` | | The fixture-driven suite of [WORKBENCH-VERIFICATION] |

Every file stays under 500 lines and every function under 20, as everywhere in this repository. Every export carries a documentation comment and the analyzer reports none missing.

## Notation [WORKBENCH-NOTATION]

Mathematics is typed as it is written on paper. Three ways produce the same symbol and the buffer always holds the Unicode form:

1. **Direct Unicode:** `∀ ∃ → ↔ ∧ ∨ ¬ ⊢ ℕ ℤ ℝ ≤ ≥ ≠ ∈ ∉ ⊆ ∘ × ⁻¹ ₀…₉ ⁰…⁹ α…ω`.
2. **The abbreviation table Lean users already know:** `\forall` `\all`, `\exists` `\ex`, `\to` `\r` `\->`, `\iff` `\lr`, `\and` `\wedge`, `\or` `\vee`, `\not` `\neg`, `\N` `\Z` `\R`, `\le` `\ge` `\ne`, `\in` `\notin` `\sub`, `\comp`, `\times`, `\inv`, `\_0`…`\_9`, `\^0`…`\^9`, `\alpha`…`\omega`. An abbreviation is replaced the moment a non-identifier character follows it, exactly as in the Lean VS Code extension, and a trailing backslash shows the matching table entries as a palette.
3. **The palette:** a region listing every symbol of the table with its abbreviation, inserting at the cursor on tap.

`Notation::expand : string -> string` is total and idempotent; the suite checks every row of the table both ways. The table is one Osprey list of `(abbreviation, symbol)` pairs, the only place a symbol is spelled.

## Formulas [WORKBENCH-FORMULA]

The first fragment is propositional logic over named propositions, because it is the fragment of the road test of plan 0041 (M3: `And`, `Iff`, `Eq`, `and_swap`) and the one a newcomer proves first:

```
type Formula =
    | Var of string
    | Top
    | Bot
    | Not of Formula
    | And of Formula * Formula
    | Or of Formula * Formula
    | Imp of Formula * Formula
    | Iff of Formula * Formula
    | Forall of List<string> * Formula
```

`Formulas::read : string -> Reading` (`Read Formula` or `Faulted Fault`; a `Result`'s error channel carries only a message, so the typed fault needs its own union) accepts symbols and sentences in one grammar, so `p ∧ q → q ∧ p` and `if p and q then q and p` are the same value. Precedence, loosest first: `↔`, `→` (right associative), `∨`, `∧`, `¬`, atoms; parentheses override. Sentence forms, case-insensitive:

| Written | Formula |
| --- | --- |
| `A and B` | `And` |
| `A or B` | `Or` |
| `not A`, `it is not the case that A` | `Not` |
| `if A then B`, `A implies B` | `Imp` |
| `A if and only if B`, `A iff B` | `Iff` |
| `for all p q, A`, `for every p, A` | `Forall` |
| `true`, `false` | `Top`, `Bot` |

A `Fault` names the column and the shortest thing that would have made the text well formed ("expected a proposition after `and`"). `Formula::show` writes the Unicode form with minimal parentheses, and `read (show f) = f` for every formula the suite generates. `Formula::typeset : Formula -> Math` produces the typesetting tree of [WORKBENCH-MATH], never a string.

Later fragments (equality, natural numbers, quantifiers over types, structures) extend the union and the two tables; nothing else in the workbench knows which fragment it holds.

## Statements [WORKBENCH-STATEMENT]

A document is a sequence of declarations. A declaration begins with a heading line and ends at the next heading or the end of the buffer:

```
Theorem and_swap. For all p q, p ∧ q → q ∧ p.
Proof.
  Assume h : p ∧ q.
  By cases on h, as hp and hq.
  It suffices to show q and p.
  From hq.
  From hp.
```

`Statement::read` recognises `Theorem <name>.`, `Lemma <name>.`, `Proposition <name>.` and `Example.`; a missing name becomes `t_<n>` in document order and is shown dimmed. The sentence after the heading is the statement; `For all p q,` binds propositional variables, and every unbound atom is a fault with a fix ("declare `r` with For all"). `Proof.` opens the steps and is optional when a step follows directly.

```
type Statement = { name : string, variables : List<string>, claim : Formula }
```

## Steps [WORKBENCH-STEPS]

A step is one sentence ending in a full stop. The phrase table is the whole tactic language a user ever sees; each row names the tactic it becomes in [WORKBENCH-EMIT]:

| Written | Step | Tactic |
| --- | --- | --- |
| `Assume h : A.`, `Suppose h : A.`, `Let h be A.` | `Assume (h, A)` | `intro h` |
| `By cases on h, as a and b.`, `Split h into a and b.` | `Cases (h, [a, b])` | `cases h with a b` |
| `It suffices to show A and B.`, `We show both.` | `Split` | `constructor` |
| `Show the left side.`, `Left.` | `Left` | `left` |
| `Show the right side.`, `Right.` | `Right` | `right` |
| `From h.`, `By h.`, `This is h.` | `Exact h` | `exact h` |
| `Apply h.`, `Use h.` | `Apply h` | `apply h` |
| `Contradiction from h and k.` | `Absurd (h, k)` | `exact absurd h k` |
| `Done.`, `QED.` | `Done` | nothing |

`Step::read : string -> Step` is total: an unrecognised sentence is `Unknown text`, kept in the document, underlined, and never sent to a checker. Names are identifiers; `A` and `B` are formulas read by `Formula::read` and must match the goal (see [WORKBENCH-GOALS]).

## The goal engine [WORKBENCH-GOALS]

The goal pane must answer on every keystroke without a process round trip, so the workbench computes goal states itself for the fragment it understands:

```
type Goal = { hypotheses : List<(string, Formula)>, target : Formula }
type Progress = { done : List<Goal>, open : List<Goal> }
```

`Goals::start : Statement -> Progress` has one open goal: the variables as atoms and the claim as target. `Goals::apply : (Progress, Step) -> Outcome` (`Advanced Progress` or `Stuck Mismatch`, for the same reason) follows the tactic semantics:

| Step | Precondition on the first open goal | Effect |
| --- | --- | --- |
| `Assume (h, A)` | target is `Imp (A, B)` or `Forall` | adds `h : A`, target `B` |
| `Cases (h, [a, b])` | `h : And (A, B)` | replaces `h` by `a : A`, `b : B` |
| `Cases (h, [a, b])` | `h : Or (A, B)` | two goals, `a : A` in the first, `b : B` in the second |
| `Split` | target is `And (A, B)` or `Iff (A, B)` | two goals, `A` then `B` (`A → B` then `B → A` for `Iff`) |
| `Left`, `Right` | target is `Or (A, B)` | target `A` or `B` |
| `Exact h` | `h : A` and target is `A` | goal closed |
| `Apply h` | `h : A` and target is `A` | goal closed (as Lean's `apply`) |
| `Apply h` | `h : Imp (A, B)` and target is `B` | target `A` |
| `Absurd (h, k)` | `h : A`, `k : Not A` | goal closed |
| `Done` | no open goal | nothing |

A failing step returns `Mismatch { expected, found }`: two formulas, typeset side by side under the step ("the goal is `q ∧ p`, but `From hp` proves `p`"), never a parser message. The engine is a display aid and a draft check, not the verdict: the kernel of spec 0049 decides ([WORKBENCH-VERDICT]), and the suite asserts that every step the engine accepts the kernel also accepts on the fixture corpus, so the two never disagree silently.

Speed: `Goals::apply` over a declaration of 50 steps completes in under one millisecond in the suite's timing case, so a 60 Hz host never waits on it.

## Generated code [WORKBENCH-EMIT]

`Emit::document : List<(Statement, List<Step>)> -> string` writes the ML proof module and is the only writer of code in the application. Its output for the example above is exactly:

```
// osprey: types=lean4
namespace proofs

theorem and_swap (p q : Prop) (h : p ∧ q) : q ∧ p := by
    cases h with hp hq
    constructor
    exact hq
    exact hp
```

Rules: a leading `Assume` on a `Forall` or `Imp` becomes a binder in the signature rather than an `intro`, so the generated file reads as a person would write it; `Unknown` steps are emitted as `-- <text>` comments followed by `sorry`, so the file always parses and the hole is visible; four-space layout; one blank line between declarations; the marker and namespace lines fixed. The generated pane shows this text with the line of the current step highlighted, and saving writes this text and nothing else. The suite holds one golden `.ospml` per fixture document and `osprey prove check` passes on every golden once the discipline ships.

## Verdict [WORKBENCH-VERDICT]

The verdict is the kernel's and nobody else's. The workbench is the interface to Osprey's proof kernel ([PROOF-KERNEL-THEORY](../language/0049-DependentTypesAndProofs.md)), and that kernel is an Osprey library, so the workbench calls it in-process on every change to a declaration. It needs no process, no `Run` capability and no ticket, and it answers the same way on every host, a tablet included:

1. **Prelude.** The connectives are their impredicative encodings in `Prop` (`And p q := ∀ r, (p → q → r) → r`, `Or`, `False := ∀ r, r`, `True`, `Not p := p → False`, `Iff`), and their rules (`and_intro`, `and_cases`, `or_inl`, `or_inr`, `or_cases`, `iff_intro`, `absurd`) are theorems with explicit proof terms. Every prelude constant is admitted by the kernel's own `Infer::sort` and `Infer::check` before it enters the environment (`Preludes::build`); nothing is an axiom.
2. **Elaboration.** `Elaborate::prove` turns the steps into one proof term, following the tactic semantics of [WORKBENCH-GOALS] (`Assume` is a λ, `By cases` applies `and_cases` or `or_cases`, `It suffices to show` applies `and_intro` or `iff_intro`, `Left`/`Right` apply `or_inl`/`or_inr`, `From h` is the variable, `Apply h` is an application, `Contradiction` applies `absurd`). A step that does not fit its goal still becomes a term, so that the kernel refuses it, not the workbench.
3. **Check.** `Verdicts::decide` sort-checks the statement as `∀ (variables : Prop), claim` and type-checks the term against it with `Infer::check`.

| Verdict | When | Shown |
| --- | --- | --- |
| `Checked` | the kernel accepts the term | a filled badge beside the heading |
| `Rejected (step, message)` | the kernel refuses the term (`step` is `-1` for the statement) | the step underlined, the kernel's message and the goal engine's readable mismatch beside it |
| `Unchecked (reason)` | an unknown step, an open goal or steps after the proof closes: no complete term exists to send | an outlined badge and the reason |

The kernel's messages name de Bruijn indices, so the underline position comes from the goal engine: the step it first refuses. The badge is never filled by the goal engine. `osprey prove check` on the emitted file ([WORKBENCH-EMIT]) is the second, out-of-process check of the same declaration once the `lean4` discipline of plan 0041 parses it; until then the in-process kernel is the only checker, and the workbench says nothing it did not check.

## Typeset mathematics [WORKBENCH-MATH]

Mathematics is typeset, not monospaced. `@osprey/native` gains one content component, `Math`, carrying a typed layout tree the hosts draw; application code never writes TeX, MathML or HTML:

```
type Math =
    | Sym of string            // an identifier or operator glyph
    | Num of string
    | Seq of List<Math>       // Row in the first draft; `Row` already names a framework row
    | Sup of Math * Math
    | Sub of Math * Math
    | Frac of Math * Math
    | Root of Math
    | Binder of string * List<Math> * Math   // ∀ x y, body
    | Op of string * Math * Math             // relation or operator with its spacing class
    | Paren of Math
    | Big of string * Math * Math * Math     // ∑ lower upper body
```

Hosts lay the tree out with a mathematics face (STIX Two Math or Latin Modern Math, whichever the host has, with the system face as fallback), thin spaces around relations, medium around binary operators, none inside function application, true fraction bars and radicals, sub- and superscripts at 70% and shifted, binders set as in a journal. The browser and VS Code hosts render the tree as MathML; the native host lays it out itself. Every `Math` node is also readable: the host exposes `Formula::show` text to assistive technology.

## Legacy board compatibility [WORKBENCH-BOARD]

The user withdrew the freehand board and handwriting workflow on 30 October 2026. The supported interface leads with the mathematical document, typed formulas and proof steps, with optional source inspection. Drawing, lasso, recognition, board zoom and board-focus controls are not Workbench delivery requirements. Their absence needs actual browser and native application acceptance; source removal alone is insufficient.

Existing saved workspaces retain their board strokes, notes and framing when opened and saved again. Keep legacy board records and host-held slices for this compatibility boundary; retiring the interface must not discard a user's saved work. The workspace round-trip tests must use nonempty legacy data and compare the complete recovered content. Generic drawing and `Recognise` capabilities in `@osprey/native` remain independent SDK APIs.

The surrounding application supports Paper and Slate. A host may supply startup context `theme: "dark"` or `theme: "light"`; the shared app selects Slate or Paper respectively and defaults to Paper without a supplied theme. Changing appearance preserves the document. Both palettes must pass `Contrast::ratio` at 4.5:1 for body text.

## Library [WORKBENCH-LIBRARY]

`Library::search : (Library, Query) -> List<Hit>` answers two kinds of query: a name (`and_comm`) and a shape (`?a ∧ ?b → ?b ∧ ?a`, metavariables prefixed `?`), matched against the statements of the pinned environment by structural unification over `Formula`. Hover on an identifier in the goal pane or the generated pane shows its statement typeset and its documentation comment; a hit inserts `Apply <name>.` at the cursor. The library is a host-held slice loaded from the environment file written by `osprey prove import` ([PROOF-EXPORT](../language/0049-DependentTypesAndProofs.md#lean-export-bridge-proof-export)); until that exists it holds the fixture environment of the suite.

## Store [WORKBENCH-STORE]

One store, one slice per concern, each its own record type so that each is its own signal:

| Slice | Held | Contents |
| --- | --- | --- |
| `Text` | host | the document text and cursor |
| `Parsed` | host | declarations, steps and faults derived from `Text` |
| `Progress` | host | the goal engine's `Progress` per declaration |
| `Generated` | host | the emitted `.ospml` and its line map to steps |
| `Verdicts` | session | verdict per declaration with its ticket |
| `Board` | host | legacy saved strokes, notes and framing; no exposed drawing control |
| `Library` | host | the environment and the current query |
| `Layout` | session | document/read mode, open panes, splitter widths and theme; legacy layout fields remain compatible |

The `generated` pane starts closed. A user who never opens it writes, reads, checks and exports a proof entirely as mathematics; the code exists for the checker and for the second tool, not for them.

`update` is pure; every host event becomes one of these messages: `Edited text`, `Moved cursor`, `Chose symbol`, `Searched query`, `Inserted hit`, `Toggled pane`, `Resized width`, `Themed theme`, `Saved`, plus the framework's completions and lifecycle phases. Parsing, the goal engine and emission run inside `update` on every `Edited`; they are fast enough by [WORKBENCH-GOALS].

## Regions [WORKBENCH-REGIONS]

Keyed regions ensure a keystroke re-sends only what it changed:

| Region | Shows | Reads |
| --- | --- | --- |
| `root` | mathematical document with the goal pane and explicit navigation to reading, notation, examples and optional source | `Layout` |
| `document` | the text as an editor with faults and verdict badges per line, typeset mathematics inline when the line is not being edited | `Text`, `Parsed`, `Verdicts` |
| `goal` | hypotheses and target at the cursor as `Math`, a mismatch when the step fails | `Progress`, `Text` |
| `generated` | the emitted file, current line highlighted, read only | `Generated`, `Text` |
| `library` | the search field, hits as rows, hover statement | `Library` |

The document editor is the framework's `Editor` component, a multiline field with two things the plain `Field` lacks, gained for this application and any other: per-line marks (an underline, a tone, a trailing badge and a note) and a cursor position delivered through its own `place` binding, so that `Moved cursor` is a message like any other. `Field` is unchanged for its existing users. Learn mode adds the `catalogue`, `lesson` and `challenge:<lesson>/<id>` regions and the `Lessons` and `Tutor` slices of [spec 0052](0052-WorkbenchLessons.md#store-and-messages-lessons-store), switched by a Work/Learn segment in `root`. A region that did not change is not sent, and the suite's "a keystroke in step three refreshes `document`, `goal` and `generated` and nothing else" case proves it.

## Reads like a paper [WORKBENCH-PAPER]

A rendered view lays the document out as a page: each statement as display mathematics under its name, the proof as numbered sentences with the goal after each in the margin, the verdict beside the heading. The page is the export: `Saved` writes the `.ospml`, and `Exported` writes the page as HTML with MathML through the same `Math` tree, so a checked file is the paper.

## Hosts [WORKBENCH-HOSTS]

| Host | How | Order |
| --- | --- | --- |
| VS Code | [`@osprey/vscode`](../../../packages/vscode/README.md): the application compiled to WASI, the existing webview renderer under `vscode-extension/client/src/osprey-ui/` drawing the regions | first, because the renderer exists |
| Browser | the same renderer as a standalone page under `hosts/web/`, the application as wasm32 with the browser's handwriting and file APIs as capability adapters | second |
| macOS and iPadOS | a SwiftUI host in this repository on the model of the [macOS storage example](../../../examples/macos/README.md), `--target=macos`, `ios`, `ios-sim`; PencilKit for the board | third |

One application, three hosts, proved by the same fixture-driven `update` tests; a host adds adapters and a renderer, never a decision.

### VSIX adapter [WORKBENCH-VSIX]

`Osprey: Open Workbench` (`osprey.workbench.open`) opens the shared application without requiring a project folder. The VSIX bundles `apps/workbench/` compiled as `workbench.wasm` with its lesson catalogue and assets. It uses the same `@osprey/native` store, update, regions, mathematics layout trees, marked editor and board as the browser and native applications. Platform trimming may omit an unavailable capability; it must not fork application modules, replace the maths interface, or introduce a second proof or lesson implementation in TypeScript.

VS Code supplies its active editor appearance in the startup context: `Dark` and `HighContrast` map to `dark`; `Light` and `HighContrastLight` map to `light`. This selects the starting application palette; changing the appearance control remains an explicit session choice. This startup contract does not claim live synchronization after an editor theme change. Reference-image fixtures select Light explicitly; acceptance also exercises dark startup and the manual toggle.

The package manager and workbench share the existing session, WASI runner, panel lifecycle, renderer and webview event transport. Browser and VS Code hosts import the same host-neutral rendering modules. Only platform adapters differ: VS Code creates the panel, maps approved bundled assets to webview URIs, reads bounded lesson files, and stores progress under the extension's own storage directory. Host-held slices accompany every application frame; stroke data and lesson text remain outside the opaque application store. Unknown operations and refused files complete with explicit errors. No workspace file, process or network authority is implied by opening the workbench.

Packaging must reject absent or invalid WASM and missing lesson assets; CI and release bundles carry the same verified files. Acceptance uses the actual compiled application in the VS Code webview: opening and reusing the panel, lesson navigation, accessible MathML, typed mathematical editing, board strokes and progress persistence. Existing package-manager host tests must remain passing after sharing the panel code. A successful renderer fixture alone is insufficient to mark the workbench VSIX complete.

Proof acceptance also requires a common Lean theorem and its complete dependency closure to pass the shared Osprey kernel under [PROOF-RUNNABLE-LIBRARY]. Show the result for the current document and the actual rejection of a corrupted proof in the same workbench flow. The baseline library target is `Nat.add_comm` after the recorded And/Eq/Iff controls, followed by the pinned recent OpenAI proof corpus in plan 0041. The corpus view must distinguish checked supporting lemmas from unported headline results and show per-theorem axiom dependencies separately from the complete export ledger. The frozen public checkpoint covers twelve OpenAI supporting lemmas and 167 core selections under all three targets. Eight additional statement selections now have independent three-target and corruption evidence: five original product proofs plus three compatible Nat proof bodies of the exact original statements. The three original Nat certificates remain refused at the nested `Lean.Syntax` boundary. The earlier expanded core replay passed 525 checks and 1,050 mutations across all three targets with fresh Lean verification and 103 unchanged recorded inputs; subsequent checker changes require a fresh replay. Compatible-source/export pins, fresh source builds and structural original-statement comparison remain mandatory; host and visual acceptance are separate gates. Neither a closed draft goal nor an imported screen establishes this result. Navier–Stokes remains a future high-water mark, with its own statement, library and axiom requirements.

`src/imported.ospml` supplies the shared imported-proof boundary: it checks the complete bounded export from an empty environment, returns the selected theorem's actual statement and per-root axiom dependencies, and refuses corrupted proofs, missing declarations and declarations that are not theorems. Its eight regressions pass under GC, ARC and wasm32. `src/workspace.ospml` now connects this API to the shared library selection, source editing and import flow; actual host acceptance, corrupt-source rejection and capture verification remain delivery gates. The previous frozen kernel checkpoint passed all 325 tests under GC, ARC and wasm32, including the guarded numeric rules required by the real Unicode constructor proofs; those two recorded proofs also pass fresh Lean replay and all three Osprey targets with both corruption controls. The 351-case intermediate matrix retained two genuine nested/mutual acceptance failures. The grouped-inductive implementation now passes all 375 kernel cases on GC, ARC and wasm32 with the freshly built `c66c6788…` compiler, zero skips, no input drift and zero live ARC objects or KiB (`target/proof/openai/syntax-frontier-review/grouped-final-receipt.json`). These include genuine mutual, nested and indexed recursor computations and rejection of malformed type formers and captured nested parameters. The registered public corpus and three original Nat certificates require separate fresh replay before promotion. A later name-identity audit reproduced false acceptance because namespace components and string/numeric leaves shared a flattened spelling. Promotion and packaging are held while the importer and independent corpus validator preserve those identities and their rejection and coexistence regressions pass on every managed target. A pure API test or CLI recording does not satisfy the host gate.

Bundled proof discovery uses the same Osprey library flow on every host. The canonical `assets/proof-libraries.json` lists thirteen groups; one shared stager copies their pinned exports and original licenses. The host reads the bounded manifest and selected source, and the application recomputes its kernel result for that current source. A pending or refused read clears the prior result; late responses cannot restore an earlier selection. Packaging checks exact export/manifest/license bytes and refuses extra files. This packaging check does not replace the independent source and fresh Lean proof CI gates.

Repeated catalogue queries may reuse a kernel result only within one application frame and only for byte-identical current source. The memo belongs to the shared Osprey computation and is discarded before session serialization; a new frame or changed source checks again. Saved state never supplies a trusted verdict. Bundled imports above the inline-source limit write their exact bytes to the named host slice and make those bytes available during the same frame; a missing or changed retained slice revokes checking.

Imported mathematics uses the full inspection surface and retains readable type size. A formula wider than its bounded native pane remains reachable by horizontal scrolling; a content-hugging frame must not suppress this viewport. Starting the conjunction example closes library inspection so its proof is visible, while retaining the imported source and selection for later inspection. Pending asynchronous host work must not present a previous result as the current checked result. These behaviors require actual-host regressions as well as shared application tests; five-colour readiness retains its existing test deadlines.

An imported result retains the checked `Term` alongside its raw statement and actual assumptions. `ProofTerms` presents that type through the existing shared `Math` tree; hosts receive no separate theorem formatter. Dependent binders retain their domain, arrows retain de Bruijn scope, and fresh display names avoid both global constants and the displayed sort labels `Prop`, `Type` and `Sort`, including inside formal fallbacks. Standard notation additionally requires identity of the declarations in the checked environment: both equality declarations for `=`, and the complete 23-declaration natural-addition closure for `+`, including definition bodies, recursor rules and inductive metadata. Universe parameter names are compared after consistent alpha-renaming. These permissions are derived from each newly checked environment and never restored from saved acceptance flags. Exact saturated `Eq.{1}` and natural-number `HAdd.hAdd` spines can then select notation. Addition still requires all three natural type arguments and the exact natural addition instance; another declaration, head, universe, arity, type or instance retains explicit formal notation. A valid custom relation named `Eq` or a left-projection instance named `instAddNat` must remain checked as its own declared theorem while retaining its explicit operator name; it must never acquire standard equality or addition notation. Grouping preserves nested equality and addition. The raw statement remains available when “Imported source” is expanded. This presentation never changes the kernel verdict or axiom ledger. The selected-result regression uses the byte-identical canonical `Nat.add_comm` fixture and compares its exact mathematics tree, rather than finding an unrelated sample formula elsewhere in the panel.

Natural multiplication, overloaded numerals and second projection also require their complete checked declaration identities: respectively the 29-declaration multiplication closure, seven-declaration natural-numeral closure and `Prod`, `Prod.mk`, `Prod.snd`. Exact universe levels, arities, natural type arguments and dictionaries remain mandatory. The imported `run_work` statement is presented as a second projection equal to `3 × width + 1`, preserving the fully qualified function name, binder scope and multiplication grouping. This is an abstract work counter, not a timing measurement. Presentation preserves the original statement and its `propext` dependency. The new regression fixture contains 51 declarations, extracted with their unchanged table entries from the pinned word-seed export; it adds no theorem or axiom. Final acceptance requires current GC, ARC, wasm32 and actual-host checks, including rejection of malformed declaration metadata; a presentation test alone cannot establish soundness.

The captures in [`docs/designs/proof-workbench/screens/`](../../designs/proof-workbench/screens/) are the visual acceptance oracle. Match each supported screen at its reference viewport and interaction state: geometry, typography, mathematical spacing, colour, borders, icons and pane placement. The initial surface matches `blank-board.png`; proof, source, mismatch, reading, slate and responsive states use their named captures. The VSIX may omit controls for capabilities unavailable in that host; the remaining interface must match the reference exactly. Do not replace a missing capability with an inert control or approximate the layout with a generic developer dashboard. Capture the real compiled app and compare it beside the corresponding reference before marking visual delivery complete.

## Beautiful, or not shipped [WORKBENCH-DESIGN]

The workbench has its own **Paper & Slate** identity, inspired by Osprey’s precision, semantic roles and spacing discipline. It does not copy the website’s midnight/cyan palette or developer-oriented composition. The mathematical document is the primary working surface; a person writes formulas and proof steps without learning Osprey. Read presents the argument as a continuous page. Generated code is optional, closed initially, and never a prerequisite for editing mathematics.

The implementation handoff is [the design system and workflow gallery](../../designs/proof-workbench/README.md). Its screenshots and playable study are design references, not evidence that the production workbench or proof kernel is implemented. All renderer and store changes still follow plan 0042 and its fixture tests.

### Colour [WORKBENCH-DESIGN-COLOUR]

These are application-owned semantic tokens, sent through `@osprey/native`; a host does not substitute its own palette. `Slate` below is the `chalk` theme’s appearance. Legacy board tokens remain for saved-data and SDK compatibility. Uncoloured mathematical notation remains the primary reading material; colour carries focus or an explicit state.

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
| `selection` | `#E4ECE5` | `#34473E` | Current proof step; paired with position or outline |
| `good` | `#315D3B` | `#B3CE94` | Explicit successful result, always labelled |
| `caution` | `#795719` | `#E6C077` | Pending or limited operation, always explained |
| `danger` | `#9A3935` | `#F0A69B` | Rejected operation or mismatched step |
| `dangerSurface` | `#F6E8E3` | `#453330` | Restrained diagnostic emphasis |
| `board` | `#202723` | `#202723` | Legacy board surface token |
| `chalk` | `#ECEDE5` | `#ECEDE5` | Legacy stroke token |

Minimum measured contrast across `canvas`, `inset` and `surface`: primary text 13.15:1 Paper / 11.21:1 Slate; secondary text 5.72:1 / 7.07:1; accent 6.02:1 / 7.13:1; semantic text at least 5.84:1 / 6.69:1; control outline 3.84:1 / 3.74:1. Chalk against board is 12.95:1. The application suite must test the exact token pairs it renders, including selection and diagnostic surfaces. Decorative dividers must never be the sole boundary of an interactive control.

### Typography [WORKBENCH-DESIGN-TYPE]

Use STIX Two Text for mathematical prose and STIX Two Math for formula layout. Use Geist for quiet controls, retaining a small family resemblance to Osprey. A host lacking STIX uses Latin Modern Math for mathematics as specified in [WORKBENCH-MATH]; do not simulate maths with spaced monospace text. Bundle and retain the selected font licences with the host.

| Role | Desktop size / leading | Treatment |
| --- | --- | --- |
| Wordmark | 26 / 32 | STIX; italic “Proof”, upright “Workbench” |
| Document title | 36–40 / 44 | STIX regular |
| Display formula | 36–44 / natural math metrics | STIX Two Math; preserve layout tree |
| Proof sentence | 22–24 / 34–38 | STIX |
| Hypothesis / current goal | 24 / 32 and 36 / 44 | Variables italic; operators upright |
| Controls and diagnostic explanation | 14 / 20 | Geist regular or medium |
| Secondary label | 12 / 18 | Geist; avoid dense all-caps copy |
| Optional source | 14 / 23 | Host monospace; never the mathematics face |

Let the mathematics engine apply spacing classes: relation spacing around `→` and `=`, binary-operator spacing around `∧` and `+`, no extra spaces inside function application, real scalable delimiters and radicals. Do not insert spaces to fake mathematical layout. Subscripts and superscripts use the math font’s layout metrics; do not shrink an entire formula to make it fit a sidebar. Long expressions can scroll within their own region.

### Surfaces and controls [WORKBENCH-DESIGN-SURFACES]

At desktop widths, the mathematical document leads, with goals alongside and source opened explicitly. Collapsing or reopening a pane preserves the document, caret and scroll position. Read presents the argument as a continuous page. On a tablet, supplementary regions move below or into an explicitly opened sheet; never squeeze a desktop sidebar into an unreadable sliver. The retired board layout is historical reference only.

Use an 8-point spacing rhythm with 4-point subdivisions: 24–32-point working margins, 16–24 between groups, 8–12 within controls. Controls use 4–6-point corners and 1-point outlines; only status labels use a pill. A mathematical argument is a continuous page, not a stack of cards. Use alignment and whitespace before adding a surface or border. Touch controls have at least a 44-point hit area. Focus has a 2-point accent outline and 3-point offset. A selected tool or step also has shape or position evidence; colour alone never carries state.

Typed draft mathematics remains unverified until the kernel returns its result. No handwriting recognition is presented or inferred. Saving preserves the document and compatible legacy working data; the save/restore contract requires its own implementation tests. Save/import feedback remains visible at every width; errors persist until dismissed. Notation inserts at the caret or replaces the selection and restores editing focus.

### Proof feedback and source [WORKBENCH-DESIGN-FEEDBACK]

| State | Presentation | Never imply |
| --- | --- | --- |
| Editing | Stable typeset context, clear caret, restrained current-line tint | That a sentence was accepted while still being composed |
| Goal closed | One quiet goal transition; explicit remaining goal or “No open draft goals” | A kernel verdict from the draft engine |
| Mismatch | Local underline plus readable “Goal” and “This step” formulas side by side; plain-language reason | That every syntax error proves a proposition false |
| Unsupported input | Neutral or caution-labelled explanation; preserve the sentence and avoid inferred propositions | That unrecognised wording is mathematically false |
| Checking | Stable heading; small labelled progress indication; current revision ticket | That an old check applies to edited text |
| Checked | Labelled filled badge from the current kernel result only | That a demo, recognition result, or scratch drawing was checked |
| Rejected | Local mark with the checker’s actual reason and line | That the document is clean elsewhere without evidence |
| Unchecked | Neutral outlined label and explicit reason | A success or a failure |

Draft completion uses neutral text while unchecked; reserve success checkmarks for an actual checked result.

Keep the source pane closed initially. On small screens, opening source must bring the panel into view and move focus into it; closing returns focus to its trigger. Opening it shows read-only generated source, line numbers and a restrained highlight tied to the selected proof step. Distinguish keywords, names, types, operators, comments and holes; `sorry` remains visibly a hole, never a successful completion. Paper source roles are plum `#794A64` keywords, mineral `#426B6B` types/operators, umber `#704F25` names and `secondary` comments; Slate uses `#D5B0C7`, `#A6C5BB`, `#DEC28F` and `#A6B5AA`. Body text, the caret and diagnostic marks retain their semantic roles. Export offers the generated file without making it the user’s main workflow.

### Motion [WORKBENCH-DESIGN-MOTION]

| Event | Timing | Behaviour |
| --- | --- | --- |
| Hover / selected control | 120–150 ms | Colour and outline only; no size change |
| Pane reveal | 220–260 ms | Opacity plus at most 12 points of travel |
| Goal closes | 280–350 ms | Previous goal yields to the next; at most 8 points of travel |
| New proof step | 180–240 ms | Small opacity transition; keep caret and scroll anchored |
| Proof / Read | 300–380 ms | Preserve content, focus and position; do not flash an empty surface |

Use an ease-out curve such as `(0.22, 1, 0.36, 1)`. Text input remains immediate, independent of animation duration. Do not animate every keystroke, replay all prior proof steps, or flicker a verdict while checking. Reduced motion removes translation and decorative effects; state changes remain immediate and understandable. Guided design-tour captions are demonstration controls, not production application chrome.

The release bar is a well-set journal page with a usable typed proof workflow. Compare a real `and_swap` capture beside a printed page, check mathematical spacing, inspect light and dark appearances, test keyboard, caret and notation input, preserve working across pane switches, and include the checker-unavailable path. A generated picture or a design prototype does not satisfy these release checks.

## Nothing special for the workbench [WORKBENCH-NO-SPECIAL]

Every service the workbench uses is a capability of the checker protocol of [spec 0050](../language/0050-TypeDisciplines.md#checker-protocol-types-discipline-protocol), the language server of [spec 0020](../tooling/0020-LanguageServerAndEditors.md) or a host capability of `@osprey/native`. The goal engine, the phrase table and the emitter are ordinary Osprey modules any editor may import. The discipline gains nothing that only the workbench can see.

## Verification [WORKBENCH-VERIFICATION]

`osprey apps/workbench/tests --run --quiet` and `python3 scripts/package_coverage.py --collect --project apps/workbench` are the suite; the project is registered in `coverage-thresholds.json` at the 90% floor. The cases, each a fixture through the real `start`/`dispatch`:

- every row of the notation table expands and is idempotent;
- `read (show f) = f` over a generated formula corpus; every sentence form of the formula table;
- every row of the step table reads, and `Unknown` for ten near-misses;
- every row of the goal table applies, and every precondition failure yields a `Mismatch` naming both formulas;
- the `and_swap` fixture emits the golden byte for byte; a document with an `Unknown` step emits `sorry` and still parses under the ML frontend;
- a keystroke refreshes exactly `document`, `goal` and `generated`;
- a stale checker completion is dropped; a rejection underlines the right line;
- the kernel accepts every prelude constant, checks the `and_swap` fixture and rejects its swapped twin at the fourth step;
- actual application hosts expose no retired freehand/recognition controls; a nonempty legacy board survives workspace open/save byte-for-byte at the serialized data boundary;
- the chalk and default themes pass contrast;
- the goal engine's 50-step timing case under one millisecond;
- the goal engine and the kernel agree on every fixture step: a step the engine accepts in a complete proof, the kernel accepts;
- once the discipline ships: `osprey prove check` passes on every golden.

Each host adds a smoke test: the VS Code host through the extension's existing `compiled SDK renderer E2E`, the browser host through a headless page, the native host through the Apple simulator. The road test of plan 0041 is the acceptance: a mathematician or scientist new to Osprey writes and checks `and_swap` in the workbench in under ten minutes without documentation, recorded in `docs/reports/`.

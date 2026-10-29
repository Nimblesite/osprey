# Proof Workbench

**Status:** the proof language is implemented (plan 0042 phase 1); the application is in progress. Delivery: [plan 0042](../../plans/0042-proof-workbench.md); Learn mode: [spec 0052](0052-WorkbenchLessons.md). The workbench depends on the proof kernel of [spec 0049](../language/0049-DependentTypesAndProofs.md) for its verdicts and on [`@osprey/native`](../../../packages/native/README.md) for everything it draws; it adds nothing to either that another editor could not use.

## Aim [WORKBENCH-AIM]

Mathematicians and scientists work on blackboards because no editor has been worth leaving one for. The workbench is that editor. A theorem is written as mathematics, in the words and symbols of a paper; a proof is written as the steps a person would say aloud; every step is checked as it is written; and the whole is read back as a typeset page. **The user writes no code.** The Osprey proof module of [PROOF-SURFACE-FLAVORS](../language/0049-DependentTypesAndProofs.md#proof-surface-proof-surface) is an output the workbench derives, shown beside the mathematics and saved as the file, so that `osprey prove check` on that file is the same check the workbench ran and no second tool needs the workbench.

The workbench is one Osprey application over `@osprey/native`: one store, one `update`, one set of regions, running unchanged inside VS Code, in a browser and as a native desktop and tablet application. The hosts differ only in what draws them.

The standard is Lean's editor experience with the parts a scientist does not want removed: no `import` lines, no tactic names to learn before the first proof, no monospaced mathematics, and the board itself as the place the work starts: the workbench opens on a large writable board that feeds the same proof.

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
| `src/board.ospml` | `Board` | Strokes, recognition and the chalk theme ([WORKBENCH-BOARD]) |
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

## The board [WORKBENCH-BOARD]

The board is the home surface, because the first thing a scientist does with a problem is draw it. The workbench opens on a large writable board filling the window; the proof and the goal column sit beside it, each collapsible, and the typed paper layout is the alternate the user switches to. `@osprey/native` gains one control, `Board`, with a stroke binding:

```
type Stroke = { points : List<(int, int)>, width : int, at : int }
type Board = { strokes : List<Stroke>, size : (int, int) }
```

The host captures pointer, pencil and finger strokes at device resolution, sends each finished stroke once through the binding, and draws the strokes it is given: chalk on slate, a textured off-white line with pressure-independent grain, slight edge scatter, and a dust fade on erase; the slate is the theme's `board` token, near-black in both appearances. The board is a host-held slice ([UI-STORE]), so a long session never travels through an event. Undo removes the last stroke; clear is a two-step action.

The board holds each stroke in its own host-held slice (`stroke:<n>`); the store holds only the count, so a stroke crosses the protocol once.

**Recognition.** A lasso over strokes sends the names of their slices through a `Recognise` request, and the host reads the points it already holds: the host's handwriting service where it has one (Apple PencilKit, the browser's handwriting API where present), answered as text. The text enters the same `Statement::read` and `Step::read` as typed input, so a sketched `p ∧ q → q ∧ p` becomes a checked line. Recognition is a capability like any other: typed request, ticket, bounded result, and a host without it answers `Unsupported`, which the board shows as a disabled lasso.

**Chalk theme.** The default theme is `chalk`, matching the board-first layout: the same tokens with the slate as canvas, the chalk off-white as text, a hand-set mathematics face and the stroke grain on rules and underlines. The `paper` theme (ink on off-white) goes with the paper layout. Both themes pass `Contrast::ratio` at 4.5:1 for body text.

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
| `Board` | host | strokes |
| `Library` | host | the environment and the current query |
| `Layout` | session | which surface leads (`Board`, the default, or `Paper`), which panes are open, splitter widths, the theme |

The `generated` pane starts closed. A user who never opens it writes, reads, checks and exports a proof entirely as mathematics; the code exists for the checker and for the second tool, not for them.

`update` is pure; every host event becomes one of these messages: `Edited text`, `Moved cursor`, `Chose symbol`, `Stroked stroke`, `Lassoed strokes`, `Searched query`, `Inserted hit`, `Toggled pane`, `Resized width`, `Themed theme`, `Saved`, plus the framework's completions and lifecycle phases. Parsing, the goal engine and emission run inside `update` on every `Edited`; they are fast enough by [WORKBENCH-GOALS].

## Regions [WORKBENCH-REGIONS]

Six keyed regions, so a keystroke re-sends only what it changed:

| Region | Shows | Reads |
| --- | --- | --- |
| `root` | board first: the `board` filling the window with the `document` and `goal` columns trailing, each collapsible; in the paper layout the `document` leads with the `goal` column trailing and the `board` behind a disclosure | `Layout` |
| `document` | the text as an editor with faults and verdict badges per line, typeset mathematics inline when the line is not being edited | `Text`, `Parsed`, `Verdicts` |
| `goal` | hypotheses and target at the cursor as `Math`, a mismatch when the step fails | `Progress`, `Text` |
| `generated` | the emitted file, current line highlighted, read only | `Generated`, `Text` |
| `board` | the `Board` control with undo, clear and lasso | `Board` |
| `library` | the search field, hits as rows, hover statement | `Library` |

The document editor is the framework's `Editor` component, a multiline field with two things the plain `Field` lacks, gained for this application and any other: per-line marks (an underline, a tone, a trailing badge and a note) and a cursor position delivered through its own `place` binding, so that `Moved cursor` is a message like any other. `Field` is unchanged for its existing users. Learn mode adds the `catalogue`, `lesson` and `challenge:<id>` regions and the `Lessons` and `Tutor` slices of [spec 0052](0052-WorkbenchLessons.md#store-and-messages-lessons-store), switched by a Work/Learn segment in `root`. A region that did not change is not sent, and the suite's "a keystroke in step three refreshes `document`, `goal` and `generated` and nothing else" case proves it.

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

The package manager and workbench share the existing session, WASI runner, panel lifecycle, renderer and webview event transport. Browser and VS Code hosts import the same host-neutral rendering modules. Only platform adapters differ: VS Code creates the panel, maps approved bundled assets to webview URIs, reads bounded lesson files, and stores progress under the extension's own storage directory. Host-held slices accompany every application frame; stroke data and lesson text remain outside the opaque application store. Unknown operations and refused files complete with explicit errors. No workspace file, process or network authority is implied by opening the workbench.

Packaging must reject absent or invalid WASM and missing lesson assets; CI and release bundles carry the same verified files. Acceptance uses the actual compiled application in the VS Code webview: opening and reusing the panel, lesson navigation, accessible MathML, typed mathematical editing, board strokes and progress persistence. Existing package-manager host tests must remain passing after sharing the panel code. A successful renderer fixture alone is insufficient to mark the workbench VSIX complete.

Proof acceptance also requires a common Lean theorem and its complete dependency closure to pass the shared Osprey kernel under [PROOF-RUNNABLE-LIBRARY]. Show the result for the current document and the actual rejection of a corrupted proof in the same workbench flow. The baseline library target is `Nat.add_comm` after the recorded And/Eq/Iff controls, followed by the pinned recent OpenAI proof corpus in plan 0041. The corpus view must distinguish checked supporting lemmas from unported headline results and show per-theorem axiom dependencies separately from the complete export ledger. The frozen public checkpoint covers twelve OpenAI supporting lemmas and 167 core selections under all three targets. Eight additional statement selections now have independent three-target and corruption evidence: five original product proofs plus three compatible Nat proof bodies of the exact original statements. The three original Nat certificates remain refused at the nested `Lean.Syntax` boundary. The expanded public replay now passes 525 checks and 1,050 mutations across all three targets with fresh Lean verification and 103 unchanged recorded inputs. Compatible-source/export pins, fresh source builds and structural original-statement comparison remain mandatory; host and visual acceptance are separate gates. Neither a closed draft goal nor an imported screen establishes this result. Navier–Stokes remains a future high-water mark, with its own statement, library and axiom requirements.

`src/imported.ospml` supplies the shared imported-proof boundary: it checks the complete bounded export from an empty environment, returns the selected theorem's actual statement and per-root axiom dependencies, and refuses corrupted proofs, missing declarations and declarations that are not theorems. Its eight regressions pass under GC, ARC and wasm32. `src/workspace.ospml` now connects this API to the shared library selection, source editing and import flow; actual host acceptance, corrupt-source rejection and capture verification remain delivery gates. The separately frozen kernel passes all 325 tests under GC, ARC and wasm32, including the guarded numeric rules required by the real Unicode constructor proofs; those two recorded proofs also pass fresh Lean replay and all three Osprey targets with both corruption controls. A pure API test or CLI recording does not satisfy the host gate.

Bundled proof discovery uses the same Osprey library flow on every host. The canonical `assets/proof-libraries.json` lists twelve groups; one shared stager copies their pinned exports and original licenses. The host reads the bounded manifest and selected source, and the application recomputes its kernel result for that current source. A pending or refused read clears the prior result; late responses cannot restore an earlier selection. Packaging checks exact export/manifest/license bytes and refuses extra files. This packaging check does not replace the independent source and fresh Lean proof CI gates.

An imported result retains the checked `Term` alongside its raw statement and actual assumptions. `ProofTerms` presents that type through the existing shared `Math` tree; hosts receive no separate theorem formatter. Dependent binders retain their domain, arrows retain de Bruijn scope, and fresh display names avoid both global constants and the displayed sort labels `Prop`, `Type` and `Sort`, including inside formal fallbacks. Exact saturated `Eq.{1}` and natural-number `HAdd.hAdd` spines select equality and addition notation. Addition requires all three natural type arguments and the exact natural addition instance; another head, universe, arity, type or instance retains explicit formal notation. Grouping preserves nested equality and addition. The raw statement remains available when “Imported source” is expanded. This presentation never changes the kernel verdict or axiom ledger. The selected-result regression uses the byte-identical canonical `Nat.add_comm` fixture and compares its exact mathematics tree, rather than finding an unrelated sample formula elsewhere in the panel.

The captures in [`docs/designs/proof-workbench/screens/`](../../designs/proof-workbench/screens/) are the visual acceptance oracle. Match each supported screen at its reference viewport and interaction state: geometry, typography, mathematical spacing, colour, borders, icons and pane placement. The initial surface matches `blank-board.png`; proof, source, mismatch, reading, slate and responsive states use their named captures. The VSIX may omit controls for capabilities unavailable in that host; the remaining interface must match the reference exactly. Do not replace a missing capability with an inert control or approximate the layout with a generic developer dashboard. Capture the real compiled app and compare it beside the corresponding reference before marking visual delivery complete.

## Beautiful, or not shipped [WORKBENCH-DESIGN]

The workbench has its own **Paper & Slate** identity, inspired by Osprey’s precision, semantic roles and spacing discipline. It does not copy the website’s midnight/cyan palette or developer-oriented composition. The board is the primary working surface; a person can keep sketches, calculations and the full written solution here without learning Osprey. Paper is the alternate reading and writing surface. Generated code is optional, closed initially, and never a prerequisite for editing mathematics.

The implementation handoff is [the design system and workflow gallery](../../designs/proof-workbench/README.md). Its screenshots and playable study are design references, not evidence that the production workbench or proof kernel is implemented. All renderer and store changes still follow plan 0042 and its fixture tests.

### Colour [WORKBENCH-DESIGN-COLOUR]

These are application-owned semantic tokens, sent through `@osprey/native`; a host does not substitute its own palette. `Slate` below is the `chalk` theme’s appearance. Freehand chalk remains off-white on the same slate in both appearances. Uncoloured mathematical notation remains the primary reading material; colour carries focus or an explicit state.

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
| `board` | `#202723` | `#202723` | Freehand surface |
| `chalk` | `#ECEDE5` | `#ECEDE5` | Freehand strokes |

Minimum measured contrast across `canvas`, `inset` and `surface`: primary text 13.15:1 Paper / 11.21:1 Slate; secondary text 5.72:1 / 7.07:1; accent 6.02:1 / 7.13:1; semantic text at least 5.84:1 / 6.69:1; control outline 3.84:1 / 3.74:1. Chalk against board is 12.95:1. The application suite must test the exact token pairs it renders, including selection and diagnostic surfaces. Decorative dividers must never be the sole boundary of an interactive control.

### Typography [WORKBENCH-DESIGN-TYPE]

Use STIX Two Text for mathematical prose and STIX Two Math for formula layout. Use Geist for quiet controls, retaining a small family resemblance to Osprey. A host lacking STIX uses Latin Modern Math for mathematics as specified in [WORKBENCH-MATH]; do not simulate maths with spaced monospace text. Bundle and retain the selected font licences with the host.

| Role | Desktop size / leading | Treatment |
| --- | --- | --- |
| Wordmark | 26 / 32 | STIX; italic “Proof”, upright “Workbench” |
| Document title | 36–40 / 44 | STIX regular; 28–30 in the board side pane |
| Display formula | 36–44 / natural math metrics | STIX Two Math; preserve layout tree |
| Proof sentence | 22–24 / 34–38 | STIX; 19–20 in the board side pane |
| Hypothesis / current goal | 24 / 32 and 36 / 44 | Variables italic; operators upright |
| Controls and diagnostic explanation | 14 / 20 | Geist regular or medium |
| Secondary label | 12 / 18 | Geist; avoid dense all-caps copy |
| Optional source | 14 / 23 | Host monospace; never the mathematics face |

Let the mathematics engine apply spacing classes: relation spacing around `→` and `=`, binary-operator spacing around `∧` and `+`, no extra spaces inside function application, real scalable delimiters and radicals. Do not insert spaces to fake mathematical layout. Subscripts and superscripts use the math font’s layout metrics; do not shrink an entire formula to make it fit a sidebar. Long expressions can scroll within their own region.

### Surfaces and controls [WORKBENCH-DESIGN-SURFACES]

At desktop widths, allocate roughly two thirds of the window to the board and one third to the proof and goal panes. Each secondary pane can collapse without losing the cursor, strokes or scroll position. Focus expands the existing board; it never creates a second board. Paper puts the document first with the goal in the margin and the same board behind its disclosure. Read presents the document as a continuous page. On a tablet, keep the board first and place supplementary regions below or in an explicitly opened sheet; never squeeze a desktop sidebar into an unreadable sliver.

Use an 8-point spacing rhythm with 4-point subdivisions: 24–32-point working margins, 16–24 between groups, 8–12 within controls. Controls use 4–6-point corners and 1-point outlines; only status labels use a pill. A mathematical argument is a continuous page, not a stack of cards. Use alignment and whitespace before adding a surface or border. Touch controls have at least a 44-point hit area. Focus has a 2-point accent outline and 3-point offset. A selected tool or step also has shape or position evidence; colour alone never carries state.

Scratch work is explicitly unverified. Recognition yields a reviewable transcription, preserves the original strokes, and requires an explicit insertion into the document. Unavailable recognition must explain its limit. Clear requires a second action in a keyboard-contained dialog; Escape cancels and returns focus to its trigger. Undo removes the most recent stroke. Board status and zoom controls occupy a separate strip, outside the drawing surface. Saving must preserve the actual working, not just the rendered theorem; the production save format and restore contract require their own implementation tests before shipping. Preserve board framing on reopen. Save/import feedback must remain visible at every width; errors persist until dismissed. Notation inserts at the caret or replaces the selection and restores editing focus.

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
| Board / Paper / Read | 300–380 ms | Preserve content, focus and position; do not flash an empty surface |
| Erase | 100–150 ms | Subtle local dust fade; no full-board effect |

Use an ease-out curve such as `(0.22, 1, 0.36, 1)`. Writing tracks the pointer immediately, independent of animation duration. Do not animate every keystroke, replay all prior proof steps, or flicker a verdict while checking. Reduced motion removes translation and decorative effects; state changes remain immediate and understandable. Guided design-tour captions are demonstration controls, not production application chrome.

The release bar is a well-set journal page and a usable working board. Compare a real `and_swap` capture beside a printed page, check mathematical spacing, inspect light and dark appearances, test keyboard and pen input, preserve working across pane switches, and include the checker-unavailable path. A generated picture or a design prototype does not satisfy these release checks.

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
- a stale recognition completion is dropped; a rejection underlines the right line;
- the kernel accepts every prelude constant, checks the `and_swap` fixture and rejects its swapped twin at the fourth step;
- a stroke lands in the `Board` slice; lasso with `Unsupported` disables the lasso; a recognised line reads as a step;
- the chalk and default themes pass contrast;
- the goal engine's 50-step timing case under one millisecond;
- the goal engine and the kernel agree on every fixture step: a step the engine accepts in a complete proof, the kernel accepts;
- once the discipline ships: `osprey prove check` passes on every golden.

Each host adds a smoke test: the VS Code host through the extension's existing `compiled SDK renderer E2E`, the browser host through a headless page, the native host through the Apple simulator. The road test of plan 0041 is the acceptance: a mathematician or scientist new to Osprey writes and checks `and_swap` in the workbench in under ten minutes without documentation, recorded in `docs/reports/`.

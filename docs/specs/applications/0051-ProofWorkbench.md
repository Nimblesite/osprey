# Proof Workbench

**Status:** specification written; no implementation started. Delivery: [plan 0042](../../plans/0042-proof-workbench.md). The workbench depends on the `lean4` discipline of [spec 0049](../language/0049-DependentTypesAndProofs.md) for its verdicts and on [`@osprey/native`](../../../packages/native/README.md) for everything it draws; it adds nothing to either that another editor could not use.

## Aim [WORKBENCH-AIM]

Mathematicians and scientists work on blackboards because no editor has been worth leaving one for. The workbench is that editor. A theorem is written as mathematics, in the words and symbols of a paper; a proof is written as the steps a person would say aloud; every step is checked as it is written; and the whole is read back as a typeset page. **The user writes no code.** The Osprey proof module of [PROOF-SURFACE-FLAVORS](../language/0049-DependentTypesAndProofs.md#proof-surface-proof-surface) is an output the workbench derives, shown beside the mathematics and saved as the file, so that `osprey prove check` on that file is the same check the workbench ran and no second tool needs the workbench.

The workbench is one Osprey application over `@osprey/native`: one store, one `update`, one set of regions, running unchanged inside VS Code, in a browser and as a native desktop and tablet application. The hosts differ only in what draws them.

The standard is Lean's editor experience with the parts a scientist does not want removed: no `import` lines, no tactic names to learn before the first proof, no monospaced mathematics, and a board to scribble on that feeds the same proof.

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
| `src/verdict.ospml` | `Verdict` | The checker run and its result ([WORKBENCH-VERDICT]) |
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

`Formula::read : string -> Result<Formula, Fault>` accepts symbols and sentences in one grammar, so `p ∧ q → q ∧ p` and `if p and q then q and p` are the same value. Precedence, loosest first: `↔`, `→` (right associative), `∨`, `∧`, `¬`, atoms; parentheses override. Sentence forms, case-insensitive:

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

`Goals::start : Statement -> Progress` has one open goal: the variables as atoms and the claim as target. `Goals::apply : (Progress, Step) -> Result<Progress, Mismatch>` follows the tactic semantics:

| Step | Precondition on the first open goal | Effect |
| --- | --- | --- |
| `Assume (h, A)` | target is `Imp (A, B)` or `Forall` | adds `h : A`, target `B` |
| `Cases (h, [a, b])` | `h : And (A, B)` | replaces `h` by `a : A`, `b : B` |
| `Cases (h, [a, b])` | `h : Or (A, B)` | two goals, `a : A` in the first, `b : B` in the second |
| `Split` | target is `And (A, B)` or `Iff (A, B)` | two goals, `A` then `B` (`A → B` then `B → A` for `Iff`) |
| `Left`, `Right` | target is `Or (A, B)` | target `A` or `B` |
| `Exact h` | `h : A` and target is `A` | goal closed |
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

The verdict is the checker's and nobody else's. On every change to a declaration the workbench requests one `Run` capability: `osprey prove check <generated file>` with a ten-second deadline and a 64 KiB output limit, ticketed by declaration name and document revision; a completion whose ticket is stale is dropped. The result is one of:

| Verdict | Shown |
| --- | --- |
| `Checked` | a filled badge beside the heading |
| `Rejected (line, message)` | the step at that line underlined, the kernel's message beside it |
| `Unchecked (reason)` | an outlined badge; reason `no checker on this host` until the discipline ships or on a host without `Run` |

The badge is never filled by the goal engine. Before the `lean4` discipline exists, every declaration is `Unchecked`, and the road test of plan 0041 is what turns the badges on; the workbench ships against that reality rather than claiming a check it did not run.

## Typeset mathematics [WORKBENCH-MATH]

Mathematics is typeset, not monospaced. `@osprey/native` gains one content component, `Math`, carrying a typed layout tree the hosts draw; application code never writes TeX, MathML or HTML:

```
type Math =
    | Sym of string            // an identifier or operator glyph
    | Num of string
    | Row of List<Math>
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

A scratch surface beside the proof, because the first thing a scientist does with a problem is draw it. `@osprey/native` gains one control, `Board`, with a stroke binding:

```
type Stroke = { points : List<(int, int)>, width : int, at : int }
type Board = { strokes : List<Stroke>, size : (int, int) }
```

The host captures pointer, pencil and finger strokes at device resolution, sends each finished stroke once through the binding, and draws the strokes it is given: chalk on slate, a textured off-white line with pressure-independent grain, slight edge scatter, and a dust fade on erase; the slate is the theme's `board` token, near-black in both appearances. The board is a host-held slice ([UI-STORE]), so a long session never travels through an event. Undo removes the last stroke; clear is a two-step action.

**Recognition.** A lasso over strokes sends them, as points, through a `Recognise` request: the host's handwriting service where it has one (Apple PencilKit, the browser's handwriting API where present), answered as text. The text enters the same `Statement::read` and `Step::read` as typed input, so a sketched `p ∧ q → q ∧ p` becomes a checked line. Recognition is a capability like any other: typed request, ticket, bounded result, and a host without it answers `Unsupported`, which the board shows as a disabled lasso.

**Chalk theme.** The typed mathematics has a second theme, `chalk`: the same tokens with the slate as canvas, the chalk off-white as text, a hand-set mathematics face and the stroke grain on rules and underlines, so a user who wants the whole workbench to be the board can have it. Both themes pass `Contrast::ratio` at 4.5:1 for body text.

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
| `Layout` | session | which panes are open, splitter widths, the theme |

The `generated` pane starts closed. A user who never opens it writes, reads, checks and exports a proof entirely as mathematics; the code exists for the checker and for the second tool, not for them.

`update` is pure; every host event becomes one of these messages: `Edited text`, `Moved cursor`, `Chose symbol`, `Stroked stroke`, `Lassoed strokes`, `Searched query`, `Inserted hit`, `Toggled pane`, `Resized width`, `Themed theme`, `Saved`, plus the framework's completions and lifecycle phases. Parsing, the goal engine and emission run inside `update` on every `Edited`; they are fast enough by [WORKBENCH-GOALS].

## Regions [WORKBENCH-REGIONS]

Six keyed regions, so a keystroke re-sends only what it changed:

| Region | Shows | Reads |
| --- | --- | --- |
| `root` | an `Aside` of the `document` with the `goal` column trailing, and the `board` below behind a disclosure | `Layout` |
| `document` | the text as an editor with faults and verdict badges per line, typeset mathematics inline when the line is not being edited | `Text`, `Parsed`, `Verdicts` |
| `goal` | hypotheses and target at the cursor as `Math`, a mismatch when the step fails | `Progress`, `Text` |
| `generated` | the emitted file, current line highlighted, read only | `Generated`, `Text` |
| `board` | the `Board` control with undo, clear and lasso | `Board` |
| `library` | the search field, hits as rows, hover statement | `Library` |

The document editor is the framework's multiline `Field` with two additions the framework gains for this application and any other: per-line marks (an underline tone and a trailing badge) and a cursor position in the text binding, so that `Moved cursor` is a message like any other. A region that did not change is not sent, and the suite's "a keystroke in step three refreshes `document`, `goal` and `generated` and nothing else" case proves it.

## Reads like a paper [WORKBENCH-PAPER]

A rendered view lays the document out as a page: each statement as display mathematics under its name, the proof as numbered sentences with the goal after each in the margin, the verdict beside the heading. The page is the export: `Saved` writes the `.ospml`, and `Exported` writes the page as HTML with MathML through the same `Math` tree, so a checked file is the paper.

## Hosts [WORKBENCH-HOSTS]

| Host | How | Order |
| --- | --- | --- |
| VS Code | [`@osprey/vscode`](../../../packages/vscode/README.md): the application compiled to WASI, the existing webview renderer under `vscode-extension/client/src/osprey-ui/` drawing the regions | first, because the renderer exists |
| Browser | the same renderer as a standalone page under `hosts/web/`, the application as wasm32 with the browser's handwriting and file APIs as capability adapters | second |
| macOS and iPadOS | a SwiftUI host in this repository on the model of the [macOS storage example](../../../examples/macos/README.md), `--target=macos`, `ios`, `ios-sim`; PencilKit for the board | third |

One application, three hosts, proved by the same fixture-driven `update` tests; a host adds adapters and a renderer, never a decision.

## Beautiful, or not shipped [WORKBENCH-DESIGN]

The [Osprey design system](../../../website/design_system.md) governs colour, type and spacing; this application adds a mathematics face and the chalk theme. The bar is a well-set page of a journal: a screenshot of `and_swap` in the workbench beside the same theorem in a printed paper is part of every release review, and a release in which the mathematics is monospaced, the spacing around `→` is wrong, or the board's chalk looks like a vector line does not ship. Motion only where it explains a change (a goal closing, a badge filling); nothing flickers while a check runs; the system Reduce Motion preference always wins.

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
- a stale verdict completion is dropped; a rejection underlines the right line;
- a stroke lands in the `Board` slice; lasso with `Unsupported` disables the lasso; a recognised line reads as a step;
- the chalk and default themes pass contrast;
- the goal engine's 50-step timing case under one millisecond;
- once the discipline ships: `osprey prove check` passes on every golden, and agrees with the engine on every fixture step.

Each host adds a smoke test: the VS Code host through the extension's existing `compiled SDK renderer E2E`, the browser host through a headless page, the native host through the Apple simulator. The road test of plan 0041 is the acceptance: a mathematician or scientist new to Osprey writes and checks `and_swap` in the workbench in under ten minutes without documentation, recorded in `docs/reports/`.

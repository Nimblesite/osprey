# Osprey Proof Workbench

Write a supported logical statement and its proof steps, inspect the changing hypotheses and goals, and see the Osprey kernel accept a complete proof or refuse an invalid one. The app typesets the mathematics beside a scratch board. Free-form writing and board strokes remain unverified; supported sentences must be recognised before they can contribute to a checked proof. Generated source is available on request.

Contract: [spec 0051](../../docs/specs/applications/0051-ProofWorkbench.md). Delivery: [plan 0042](../../docs/plans/0042-proof-workbench.md). Status: **proof-language modules implemented; application and hosts in progress.** The app elaborates supported steps into terms and calls the shared Osprey kernel directly. A Checked badge requires the kernel to accept the statement and proof. The separate generated-text-to-Lean workflow is not the source of that verdict; incomplete generated proofs retain explicit holes. See [spec 0049](../../docs/specs/language/0049-DependentTypesAndProofs.md).

```
Theorem and_swap. For all p q, p ∧ q → q ∧ p.
Proof.
  Assume h : p ∧ q.
  By cases on h, as hp and hq.
  It suffices to show q and p.
  From hq.
  From hp.
```

The goal pane after `By cases on h` shows `hp : p`, `hq : q` and the target `q ∧ p`, typeset. The generated file, closed by default, reads:

```
// osprey: types=lean4
namespace proofs

theorem and_swap (p q : Prop) (h : p ∧ q) : q ∧ p := by
    cases h with hp hq
    constructor
    exact hq
    exact hp
```

The browser and native app share the Osprey application built on `@osprey/native`: one store, one update function, one proof model and the same typed UI trees. Hosts render and execute platform capabilities. The React design study is a reference artifact, not application logic.

## Layout

| Path | Contents |
| --- | --- |
| `src/notation.ospml` | Symbols: Unicode, the `\forall` abbreviation table, the palette |
| `src/formula.ospml` | Propositional formulas read from symbols or sentences, shown, typeset |
| `src/statement.ospml`, `src/step.ospml` | Theorem headings and the sentence forms of proof steps |
| `src/goals.ospml` | Hypotheses and goal after each step; what a failing step expected and found |
| `src/emit.ospml` | Read-only generated proof text, including holes for unfinished steps |
| `src/verdict.ospml` | Elaborating supported steps and asking the shared kernel for a verdict |
| `src/board.ospml` | Chalk strokes, lasso recognition, the chalk theme |
| `src/shelf.ospml`, `src/learnview.ospml` | Lesson catalogue and guided exercises |
| `src/app.ospml`, `src/main.ospml` | The store, `update`, regions and host entry points over [`@osprey/native`](../../packages/native/README.md) |
| `tests/` | Fixture-driven suite; `tests/golden/` holds the generated files |

## Running

Run the suite from its test directory, where the lesson fixture paths resolve. From the repository root:

```sh
(cd apps/workbench/tests && osprey . --run --quiet --memory=gc)
(cd apps/workbench/tests && osprey . --run --quiet --memory=arc)
osprey apps/workbench --check
```

Hosts, in delivery order: VS Code through [`@osprey/vscode`](../../packages/vscode/README.md), the browser through `hosts/web/`, macOS and iPadOS through `hosts/apple/`. One application, the same `update` and regions on each; a host draws and completes capabilities and decides nothing.

Osprey is actively evolving. The workbench's first fragment is propositional logic; equality, natural numbers and quantifiers over types follow by extending the same tables.

# Osprey Proof Workbench

A blackboard that checks. Mathematicians and scientists write a theorem the way they would on a board, in symbols or in sentences, write the proof as the steps they would say, and watch each step checked as it is written. The proof is read back typeset, like a page of a paper, and a scratch board beside it takes chalk strokes that can become checked lines. Nobody writes code: the Osprey proof file is generated, shown on request, and saved.

Contract: [spec 0051](../../docs/specs/applications/0051-ProofWorkbench.md). Delivery: [plan 0042](../../docs/plans/0042-proof-workbench.md). Status: **specified; no implementation started.** Verdicts come from the `lean4` discipline of [spec 0049](../../docs/specs/language/0049-DependentTypesAndProofs.md); until it ships every declaration shows as unchecked, and the workbench never fills a badge on its own.

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

## Layout

| Path | Contents |
| --- | --- |
| `src/notation.ospml` | Symbols: Unicode, the `\forall` abbreviation table, the palette |
| `src/formula.ospml` | Propositional formulas read from symbols or sentences, shown, typeset |
| `src/statement.ospml`, `src/step.ospml` | Theorem headings and the sentence forms of proof steps |
| `src/goals.ospml` | Hypotheses and goal after each step; what a failing step expected and found |
| `src/emit.ospml` | The generated `.ospml` |
| `src/verdict.ospml` | Running `osprey prove check` and showing its answer |
| `src/board.ospml` | Chalk strokes, lasso recognition, the chalk theme |
| `src/library.ospml` | Search by name or statement shape |
| `src/app.ospml`, `src/main.ospml` | The store, `update`, regions and host entry points over [`@osprey/native`](../../packages/native/README.md) |
| `tests/` | Fixture-driven suite; `tests/golden/` holds the generated files |

## Running

```sh
osprey apps/workbench/tests --run --quiet          # the suite
osprey apps/workbench --check                      # types, effects, documented exports
```

Hosts, in delivery order: VS Code through [`@osprey/vscode`](../../packages/vscode/README.md), the browser through `hosts/web/`, macOS and iPadOS through `hosts/apple/`. One application, the same `update` and regions on each; a host draws and completes capabilities and decides nothing.

Osprey is actively evolving. The workbench's first fragment is propositional logic; equality, natural numbers and quantifiers over types follow by extending the same tables.

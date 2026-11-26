# Osprey Proof Workbench

Inspect checked theorems from bundled and imported proof libraries, and answer lesson challenges in mathematical notation, `p ∧ q → q ∧ p`, which the Osprey kernel judges. Formula keys help enter symbols.

The workbench never reads or writes English as mathematics or proof ([WORKBENCH-NO-ENGLISH](../../../docs/specs/applications/0051-ProofWorkbench.md#no-english-source-workbench-no-english)). A proof may be rendered as English words on screen, one way, but never parsed, saved or exported as English. The English-sentence proof document, its readers, the free-form draft editor, workspace files and the generated `// osprey: types=lean4` source are deleted. The goal engine, kernel bridge and notation reader are kept ([WORKBENCH-DELETED-HOLD](../../../docs/specs/applications/0051-ProofWorkbench.md#no-english-source-workbench-no-english)) while an Osprey-owned mathematics format is considered beside Lean.

Contract: [spec 0051](../../../docs/specs/applications/0051-ProofWorkbench.md). Delivery: [plan 0042](../../../docs/plans/0042-proof-workbench.md). Lean is the development oracle for compatible statements and certificates; Osprey's kernel makes the Workbench's proof decisions. Follow the [oracle, independent-operation and discipline-correction contract](../../../docs/messaging/osprey.md#proof-development-and-the-lean-oracle).

The browser and native app share the Osprey application built on `@osprey/native`: one store, one update function, one proof model and the same typed UI trees. Hosts render and execute platform capabilities. The React design study is a reference artifact, not application logic.

## Layout

| Path | Contents |
| --- | --- |
| `src/notation.ospml` | Symbols: Unicode, the `\forall` abbreviation table, the palette |
| `src/formula.ospml` | Propositional formulas read from notation only, shown, typeset |
| `src/statement.ospml`, `src/step.ospml` | The statement and step values the held goal engine checks; nothing reads them from English |
| `src/goals.ospml` | Hypotheses and goal after each step; what a failing step expected and found |
| `src/verdict.ospml` | Elaborating steps and asking the shared kernel for a verdict |
| `src/imported.ospml`, `src/workspace.ospml` | Checking bundled and imported proof exports; the library pane |
| `src/shelf.ospml`, `src/learnview.ospml` | Lesson catalogue and guided exercises |
| `src/app.ospml`, `src/main.ospml` | The store, `update`, regions and host entry points over [`@osprey/native`](../packages/native/README.md) |
| `tests/` | Fixture-driven suite |

## Running

Run the suite from its test directory, where the lesson fixture paths resolve. From the repository root:

```sh
(cd src/osprey/workbench/tests && osprey . --run --quiet --memory=gc)
(cd src/osprey/workbench/tests && osprey . --run --quiet --memory=arc)
osprey src/osprey/workbench --check
```

Hosts, in delivery order: VS Code through [`@osprey/vscode`](../packages/vscode/README.md), the browser through `hosts/web/`, macOS and iPadOS through `hosts/apple/`. One application, the same `update` and regions on each; a host draws and completes capabilities and decides nothing.

Osprey is actively evolving. The workbench's first fragment is propositional logic; equality, natural numbers and quantifiers over types follow by extending the same tables.

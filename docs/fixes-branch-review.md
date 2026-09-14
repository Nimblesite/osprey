# Branch review: `fixes` compared with `main`

## Verdict: **DO NOT MERGE — three regressions and two silent-failure holes**

The compiler core holds up. A program-by-program differential across the whole repo found **no stdout or exit-code regression** in any program that compiled on `main`. The float, NaN and interpolation-position fixes are real and correctly pinned.

The problems are in the new tooling around the compiler: the language server, the doctest harness, and the HTML documentation build. Two of the findings below are **silent failures**, and this repo's rules rank those worse than a crash. The language server now **erases real type errors** from one file while you are typing in another. The new doctest gate **reports green while skipping examples it never ran**. Neither can merge under `CLAUDE.md`.

## Scope and method

- Compared `origin/main` (`8792de9f`, also the merge base; `main` has nothing the branch lacks) with `fixes` (`30eee4b4`): **17 commits, 202 files, +43,793 / −27,868** (55k of that is the regenerated `parser.c`).
- Built both compilers side by side (`main` in a detached worktree) and ran **every** `.osp`/`.ospml`/`.ospo` under `tests/`, `examples/` and `Book/examples/` through both with `--run --quiet`. I compared stdout, stderr and exit code for all 429 programs.
- Ran the branch's own gates: `run_test_corpus.sh default`, `cargo test --workspace --release`, `cargo fmt --check`, `cargo clippy -D warnings` and `verify-no-dead-code.mjs`.
- Wrote targeted probe programs: interpolation with escapes and non-ASCII text, numeric polymorphism, NaN, `//!` placement, unused-binding false positives, and doctest fences. I also drove `osprey lsp` over stdio against a two-file project, on both binaries.
- **Not run:** wasm32, gc and arc corpus backends, the VSIX installed-extension suite, the Playwright docs-HTML acceptance, and deslop (not installed locally). CI must cover those.

## What is verified clean

| Gate | Result |
|---|---|
| Differential, 429 programs, `main` vs `fixes` | **0 stdout/exit regressions.** The only changes are intended: 2 NaN goldens now pass, and 3 new rejection cases reject. |
| `run_test_corpus.sh default` | 213/213 pass, 213/213 goldens, 18/18 GPU-mode pairs, 6/6 doctests |
| `cargo test --workspace --release` | every suite `ok`, exit 0 |
| `cargo fmt --check`, `cargo clippy -D warnings`, dead-code gate | clean |
| `--check` wall time on the largest programs | unchanged within noise (api_browser 6.34 s → 6.37 s; modules project 5.76 s → 5.78 s) |

Correct fixes confirmed by the differential:

- **Float `!=`** now uses the unordered predicate (`une`), so `NaN != NaN` is `true`. `main` fails the new truth-table test (exit 1) and the branch passes it.
- **`fptosi` → `llvm.fptosi.sat`**: no more poison on NaN or out-of-range values.
- **ML float literal overflow** (`1e400`-style literals) was silently accepted on `main` and becomes infinity. It is now rejected in both flavors.
- **Interpolation positions:** `staged_static_handler_resumes` blamed `2:4`, which is inside the synthetic fragment. It now blames `8:4`, the real line of the `resume`. The goldens were updated to the truthful position, not weakened.
- **`float_operand_constraint`:** `main` crashed in codegen (`invalid program: expected a number`). The branch now rejects it in the type checker.

## Merge blockers

### 1. HIGH (regression) — LSP erases a file's real errors whenever an open sibling has a syntax error

`crates/osprey-lsp/src/diagnostics.rs:211-219`:

```rust
if let Some(live) = live {
    let parsed = osprey_syntax::parse_program_for_path(..., &live);
    if !parsed.errors.is_empty() {
        return Some(Analysis::default());   // <- every diagnostic for THIS file, gone
    }
```

`publish_siblings` (`crates/osprey-lsp/src/server.rs:181`) is new on this branch. It runs on every `didOpen`, `didChange` and `didClose`. So the moment you type half an expression in `helper.ospml`, every open sibling is republished with **an empty diagnostic list**. Their genuine type errors disappear. Nothing tells the user that analysis was skipped.

Reproduced over stdio, same project, same edits:

```text
=== fixes
[open main]                     main: ['unknown identifier `missingName`']
[helper mid-edit (syntax error)] helper: ['unexpected token Eof in expression', "expected ')'"]
[helper mid-edit (syntax error)] main: []            <- real error erased
=== main
[open main]                     main: ['unknown identifier `missingName`']
[helper mid-edit (syntax error)] helper: [...]       <- main.ospml keeps its error
```

This is the exact "silently wrong output" the Broken Code Process exists for. An editor showing a clean file that does not compile is worse than an editor showing nothing new.

Also on line 212: `path_to_uri(&candidate.path).ok()?` returns `None` for the whole project when **one** path fails to convert. That silently drops the file into standalone analysis, where project imports become phantom errors.

Required before merge:

- A failing server test first: open A (with a type error) and B, break B's syntax, then assert A's error is still published. The harness in `server.rs` already has `sibling_open_change_and_close_refresh_signature_warnings` to copy.
- Never publish an empty list as a stand-in for "could not analyse". Either keep the last good diagnostics for A, or publish an explicit diagnostic naming the broken sibling. The disk-content fallback that `main` used is also acceptable.
- Don't let one unconvertible path abort the project with `?`. Skip that candidate or report it.

### 2. HIGH (new gate, silent pass) — the doctest harness skips examples and still reports green

`crates/osprey-syntax/src/docparse.rs:95-121` only recognises a fence whose info string is exactly `osprey`, followed *immediately* by an exactly-`output` fence. Anything else is silently dropped.

**(a) An `osprey-ml` fence is never checked.** The spec itself uses ```` ```osprey-ml ```` for ML snippets (`docs/specs/0026-DocumentationComments.md:37`), so ML authors will copy that label:

````ospml
(** Doubles its argument.

# Examples

```osprey-ml
print "${double 2}"
```
```output
5
```
*)
double x = x * 2
````

```text
$ osprey dt_ml_label.ospml --doctests
doctests: 0 passed, 0 failed        exit 0     <- wrong expectation "5", never executed
```

**(b) A blank line before the `output` fence silently turns the example into compile-only.** That blank line is ordinary Markdown style:

```osprey
/// ```osprey
/// print("${add(1, 2)}")
/// ```
///
/// ```output
/// 9
/// ```
fn add(a, b) = a + b ?: 0
```

```text
$ osprey dt_blank.osp --doctests
doctests: 1 passed, 0 failed        exit 0     <- actual output is 3; "9" is ignored
```

`crates/corpus_doctests.sh` makes this worse: it only visits files containing ```` ```osprey ````, and its floor is **exactly** the current count (6). A contributor who writes ML-labelled examples adds zero coverage and the gate stays green.

Required before merge:

- Failing tests for both cases, in `docparse` and in `crates/osprey-cli/tests/cases/doctests.rs`.
- An orphaned `output` fence, or an unrecognised fence label inside `# Examples`, must be a **hard error** that names the declaration. Alternatively, accept `osprey-ml` and `ospml` as aliases and allow blank lines between the fences, but the orphan case must still fail loudly.

### 3. MEDIUM (gate hole) — the compiler now embeds a `website/` file that CI treats as website-only

`crates/osprey-cli/src/docs/html/layout.rs:64` embeds `website/src/js/osprey-grammar.mjs` with `include_str!` and then rewrites it with `.replacen("export const", "const", 1)`.

- `.github/workflows/ci.yml:78-82` (and `ci-windows.yml:34`) classify `website/**` as **not code**. A PR that touches only `osprey-grammar.mjs` therefore skips `build`, `test-rust`, `windows-core` and the new docs-HTML browser acceptance. Yet it changes the bytes of the shipped `osprey` binary.
- `replacen(..., 1)` is a silent contract. If the website adds a second `export`, an `import`, or renames the first binding, the inlined classic `<script>` throws a `SyntaxError`. Because `highlight.js` and `behavior.js` are concatenated into the **same** script, search, navigation and the mobile menu all die with it. No compiler test will see it, because no compiler job runs.
- `webcompiler/Dockerfile` already had to learn about this cross-tree dependency. Every future packager will too.

Required: move the grammar under `crates/`, or add the path to the `code` filter; don't solve it with a path filter on a required job (see `CLAUDE.md`). Replace `replacen` with a checked transform that errors when the module shape is not what it expects, and add a test for that error.

## Other findings

### 4. MEDIUM — the new FLOAT-OPERANDS rejection has no source position

`crates/osprey-types/src/expr.rs:1556` pushes numeric obligations into `builtin_uses`, which carries no position. The golden proves it:

```text
examples/failscompilation/float_operand_constraint.ospo: operator `*` requires int or float; got string
```

The error has no `line:col`. In a five-line fixture that is fine; in a 1,300-line file or an assembled project it cannot be located. The LSP places it at line 1, column 0. Rejection beats the old codegen crash, but a "truthful error" has to say where. Carry the operator's position through the obligation.

### 5. MEDIUM — a stray `//!` is now a hard error, and the Default-flavor error is misleading

Any `//!` that is not the first item of a file, namespace or module no longer compiles. This is intended (`[DOC-SIGIL-INNER]`), but it breaks source compatibility, and the two flavors report it very differently:

```text
x = 1 //! trailing note              (ML)
ml_trailing.ospml:1:6: `//!` documents the enclosing file, namespace or module; write it as the first item of one, ...

let x = 1 //! trailing note          (Default)
def_trailing.osp:1:14: syntax error near "trailing"
```

The Default message points *inside the comment text* and never mentions `//!`. That is not a truthful error. Give the Default grammar path the same diagnostic the ML lowerer gives, and state the breaking change in the release notes.

### 6. LOW — tree-sitter highlighting lost `//!`

`tree-sitter-osprey/queries/highlights.scm:6-7` captures `(line_comment)` and `(doc_comment)` but not the new `(inner_doc_comment)`. On `main`, `//!` lexed as `line_comment` and was highlighted as a comment. It is now an uncaptured node and renders as plain text in every tree-sitter consumer (Neovim, Helix, Zed, and the "one canonical source" this file claims to be). Add `(inner_doc_comment) @comment.documentation`.

### 7. LOW (advisory regression) — three true-positive redundant-annotation warnings vanished

Branch vs `main` over the corpus: signature warnings 123 → 121, parameter warnings 430 → 429. The lost sites are `tests/core/gpu/mlkernels.test.osp:50` (`d: float`), `mlkernels.test.ospml:63` and `gamedev.test.ospml:137`. I removed `: float` from `softWeight` and re-ran on the branch: it compiles, and its output is **byte-identical** to the golden. By the repo's own definition, that annotation is redundant. The numeric-operand obligation makes the scheme display differently, so the oracle stopped recognising it. Either teach the oracle that a satisfied numeric obligation is not a type difference, or pin the new behaviour with a test that says why.

### 8. LOW — `publish_siblings` does a full disk load per keystroke

Each `didChange` calls `osprey_project::load(&root)`, which reads every project file from disk. It then re-runs full project analysis (inference, unused symbols, and the redundant-annotation oracle) for **every** open sibling. On `examples/projects/modules`, a single file's project analysis already takes about 3 s. I did not get a clean scaling number (the server appears to coalesce publishes), so this is a risk, not a measured regression. It needs a benchmark with N open files before it ships.

### 9. LOW — rule drift

- `crates/osprey-project/src/resolve.rs` crosses the 500-LOC ceiling on this branch (471 → 502).
- The branch's own new unused-symbol lint produces **835 new warnings across the repo's own tests and examples**: 551 pattern bindings, 175 handler parameters, 69 variables and 40 parameters. Examples are what users copy; `examples/wasm/hello.osp` now greets its reader with a warning. Clean the examples, or explain why they stay.

## Pre-existing defects found while probing (present on `main` too — not regressions, still must be reported)

Both of these are silent failures or crashes on programs the type checker accepted:

1. **Interpolating a list prints nothing, in both flavors.** `let xs = [1, 2]` then `print("[${xs}]")` (or ML `print "[${xs}]"`) prints `[]` and exits 0. A plain `print(xs)` is correctly rejected (`cannot convert value for printing: List<int>`), so interpolation is the unguarded path. It must be rejected the same way.
2. **A bare payload-less `Error` arm on a `Result` crashes the backend.** `match 10 % 3 { Success { value } => value  Error => 0 }` passes type checking, then emits invalid LLVM IR (`PHINode should have one entry for each predecessor`) and exits 101. It must either compile or be rejected with a type error.

## Final disposition

**Do not merge until blockers 1–3 have failing tests in the tree and fixes behind them.** Findings 4–5 should land in the same pass, because they concern whether a rejection is truthful. Findings 6–9 can follow, but they need an issue each and cannot be waved through as footnotes.

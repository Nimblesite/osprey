# Plan 0058: Every Documented Export Has a Checked Example

Every exported declaration's documentation shows at least one example, and every
example is compiled and run by `--doctests`. An example may be written in the
Default, ML or Python flavor, or in several, whichever flavor the documented
file uses. The published package reference on the website (starting with
`/docs/packages/json/`) shows these examples on every page.

Spec homes: [Documentation Comments](../specs/language/0026-DocumentationComments.md)
and [Documentation Tools](../specs/tooling/DocumentationTools.md). The rules
below are added to those specs when this plan is implemented.

## Where things stand

- `# Examples` sections already exist (`[DOC-SECTIONS]`) and `--doctests` runs
  them (`[DOC-DOCTEST-HARNESS]`).
- **No package has examples.** For instance, all 22 `@osprey/json` exports have
  a summary but no example, so their website pages show none.
- **Nothing requires examples.** `[DOC-EXPORT-REQUIRED]` warns only when the
  doc comment itself is missing.
- **Doctests fail in every package.** A package's `default_namespace` names its
  own namespace. The harness wraps each example in a function named
  `$documentation_example0` at every nesting level. Assembly then moves the
  outermost wrapper into that same namespace, which already has one. The
  result is `duplicate declaration ui::$documentation_example0`, even for a
  single example.
- **Python docstring examples cannot parse.** The docstring is only trimmed, so
  a fenced example keeps the docstring's indentation. It then fails with "this
  line's indentation does not match its enclosing block".
- **An example's flavor is fixed.** It always compiles in the flavor of the file
  documenting it, and the fence label is ignored. An ML package cannot show a
  Default or Python example of the same call.

## Rules

### `[DOC-DOCTEST-WRAPPER-DEPTH]` Each nesting level has its own wrapper

The wrapper the harness generates at nesting depth *n* is named
`$documentation_example<n>`. The same name never appears twice in one
namespace, wherever `default_namespace` places the outermost wrapper.

### `[DOC-PYTHON-DOCSTRING-CLEAN]` Docstrings are cleaned as PEP 257 does

A Python docstring is cleaned the way `inspect.cleandoc` cleans it:

- the first line is trimmed;
- every later line loses the indentation that all later non-blank lines share.

A fenced example keeps only its own relative indentation.

### `[DOC-EXAMPLE-FLAVORS]` A fence label picks the example's flavor

| Fence label | Flavor |
| --- | --- |
| `osprey` | Default |
| `osprey-ml` or `ospml` | ML |
| `osprey-python` | Python |

An unlabelled example compiles in the documenting file's flavor, as it does
today. Each example is checked in its own flavor, and its `output` fence applies
to it alone.

Labelled examples that follow one another without an `output` fence between them
are flavor twins of a single example. They must print the same output, and the
harness fails if they differ. The website shows twins as flavor tabs. The
exporter writes each twin in a fence labelled with its flavor.

Breaking change: an ML file whose example fence is labelled `osprey` now compiles
that example as Default. Every such fence in the repository is relabelled
`osprey-ml` in the same change.

### `[DOC-EXPORT-EXAMPLE-REQUIRED]` Exports must show an example

`--check`, `--run`, `--doctests` and the language server report an
`unexampled-export` warning for:

- an exported function, value, extern or effect operation whose doc comment has
  no `# Examples` section, and
- such a section that holds no example fence.

A type, module, namespace or signature is covered by the examples of the
members that use it, so these are not reported. The warning names the
declaration with its module path, just as `undocumented-export` does.

## Implementation

1. Write the failing E2E test first, in
   `src/toolchain/crates/osprey-cli/tests/cases/doctests.rs`. It builds a
   package whose `default_namespace` equals its namespace, with examples in a
   `.osp`, an `.ospml` and an opted-in `.py` file, at both namespace depth and
   module depth. Then implement `[DOC-DOCTEST-WRAPPER-DEPTH]` in
   `osprey-cli/src/doctests/synthesize.rs`: `runner(depth)` replaces
   `runner()`.
2. Implement `[DOC-PYTHON-DOCSTRING-CLEAN]` in
   `osprey-syntax/src/python/documents.rs`, so that both docstring paths call
   one `clean_docstring`.
3. Add examples to every `@osprey/json` export, each with an `output` fence.
   Run them through `osprey src/osprey/packages/json --doctests` under GC, ARC
   and wasm32.
4. Add the packages folder to the corpus doctest pass (`corpus_doctests.sh`),
   so that every package's examples run in CI.
5. Implement `[DOC-EXAMPLE-FLAVORS]`: per-fence flavor in `docparse.rs` and
   `DocExample`, per-example flavor in the harness, twin comparison, labelled
   fences in the exporter, and website flavor tabs. Relabel existing ML-file
   fences in the same change.
6. Implement `[DOC-EXPORT-EXAMPLE-REQUIRED]` beside `undocumented-export` in
   `osprey-types`. Add examples to every package until
   `make -C src/osprey/packages` reports no `unexampled-export` warnings. Then
   promote the warning to an error in the package build.
7. Website: the Playwright test for `/docs/packages/json/` asserts that every
   function page renders at least one example, with its output.

## Checklist

- [x] `[DOC-DOCTEST-WRAPPER-DEPTH]` test and fix: `doctests/synthesize.rs` names each runner by role and depth, and `tests/cases/doctests.rs` (`doctests_run_in_packages_whose_default_namespace_is_their_own_in_every_flavor`) runs examples at namespace and module depth in all three flavors.
- [x] `[DOC-PYTHON-DOCSTRING-CLEAN]` test and fix: `python/documents.rs` cleans every docstring as PEP 257 `inspect.cleandoc` does, and the same `doctests.rs` package runs an example fenced inside an indented docstring.
- [ ] Examples for all 22 `@osprey/json` exports, run under GC, ARC and wasm32
- [ ] The corpus doctest pass covers `src/osprey/packages/`
- [ ] `[DOC-EXAMPLE-FLAVORS]` parser, harness, exporter, website tabs, relabelling
- [ ] `[DOC-EXPORT-EXAMPLE-REQUIRED]` warning, examples in every package, warning promoted to an error
- [ ] Spec sections added to 0026 and DocumentationTools, with IDs cited in the code and tests

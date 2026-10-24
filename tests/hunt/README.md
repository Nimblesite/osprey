# Hunt tests

Small programs with a known Hunt result, run by `make _test_hunt` (part of
`make test` on a machine with KLEE installed). Each `<name>.osp` has a
`<name>.expected` listing the findings Hunt must report, one `<class>
<file>:<line>` per line, or `none`. The replay verdict is not part of the
expectation; the findings are the same on every host [SYMEX-HOSTS].

Osprey programs cannot index out of bounds or dereference null by
construction. Apart from the deliberately planted C `abort()` in `crash.osp`,
a `memory`, `abort` or `arith` finding here is a compiler,
runtime or mock defect. `json_empty.osp` was added when the corpus sweep found
`close_container` in `json_runtime.c` forming `NULL + 0` for an empty
container (a klee-only `memory` finding reported at the Osprey call).
`record_update_list.osp` pins osprey#293: a record update whose new field
value was a list literal of records stored the literal's flat layout where
the field promised a runtime list, and the next `listGet` segfaulted. Hunt
reported it as a `memory` finding at the `listGet` line, replayed and
`reproduced (SIGSEGV)`, before the fix in `aggregate::settled_field_value`.

To see actual crash detection, run:

```sh
./target/release/osprey hunt tests/hunt/crash.osp --input-bytes 1 --max-time 10
```

`crash.osp` deliberately calls C `abort()` only when the input is `"9"`. Hunt discovers that input, reports `abort crash.osp:4`, and confirms `reproduced (SIGABRT)` in a normal native run. This demonstrates detection of an intentionally planted failure, not discovery of a new compiler defect. Exit status 1 is expected.

Open the printed `report.html` path for the explanation and **For AI** context. JSON is also saved by default under `target/hunt/<program-and-run-id>/`; `--out target/hunt-demo` keeps both reports and raw KLEE artifacts in a predictable directory. Disable saved files independently with `--no-html` and `--no-json`.

`process.osp` is an unsupported-operation test, not a crash-detection demo.

## Counts and deliberate failures

Four explored paths, three normal exits, one reproduced crash and zero unfinished paths means Hunt found the failing execution. Paths are different possible executions, not repeated attempts or a detection success rate. Findings are deduplicated, so their count need not equal the number of failing paths. An unsupported operation is not a program crash; an unfinished path is not evidence of safety.

`make hunt-crashes` runs the fifteen input cases pinned in `crash-cases.json`, using real KLEE and native replay. Each isolated program must report exactly the expected class and source location, the expected stdin/file payload and an allowed native signal. Tool errors, missing findings, incomplete exploration, KLEE-only results and skipped replay fail. The two `memory` cases intentionally misuse C `free`; the others intentionally abort at application invariants. Default, ML and Python-flavor projects are covered. CI requires this gate and uploads its HTML/JSON reports.

`python3 scripts/hunt_sample.py --out target/my-hunt-sample` samples all runnable apps with a hard ten-second cutoff and writes an HTML summary plus `quick-corpus.txt`. Only complete, bounded, finding-free runs qualify. Source modules and non-entry libraries are not separate apps. Keep the generated reports local; review candidate timing on the CI host before promoting the manifest to a required gate.

# Hunt tests

Small programs with a known Hunt result, run by hand with `make _test_hunt` on
a machine with KLEE installed. Neither `make test` nor CI runs it while issues
#306 and #307 are open. Each `<name>.osp` has a
`<name>.expected` listing the findings Hunt must report, one `<class>
<file>:<line>` per line, or `none`. The replay verdict is not part of the
expectation; the findings are the same on every host [SYMEX-HOSTS].

Osprey programs cannot index out of bounds or dereference null by
construction. The cases pinned in `crash-cases.json` deliberately abort or violate C memory rules. Other `memory`, `abort` or `arith` findings in safe Osprey code indicate a compiler, runtime or mock defect. `json_empty.osp` was added when the corpus sweep found
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

`make -C tests/hunt hunt-crashes` runs the fifteen input cases pinned in `crash-cases.json`, using real KLEE and native replay. Each isolated program must have a normal symbolic path, pass a benign-input native check, and report exactly the expected class and source location, the expected stdin/file payload and an allowed native signal. Tool errors, missing findings, incomplete exploration, KLEE-only results and skipped replay fail. The two `memory` cases intentionally misuse C `free`; the others intentionally abort at application invariants. Default, ML and Python-flavor projects are covered. CI requires this gate and uploads its HTML/JSON reports.

`python3 scripts/hunt_sample.py --out target/my-hunt-sample` samples runnable apps with a hard ten-second cutoff. The report lists each program, its run status, and any issues; logs and path counts are under Details. `safe-runs.json` records only complete, bounded, finding-free runs and their limits. Run `python3 scripts/hunt_sample.py --manifest target/my-hunt-sample/safe-runs.json --out target/hunt-ci` to gate that list: any timeout, finding, incomplete analysis, or tool failure returns a nonzero exit. `tests/hunt/safe-runs.json` pins the 215-program CI corpus at two symbolic bytes per stream, an eight-second engine budget and a ten-second wall deadline; CI reruns it and uploads the reports. These bounded results do not prove safety for larger inputs. `quick-corpus.txt` remains available. Keep generated reports local.

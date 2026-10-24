# Hunt tests

Small programs with a known Hunt result, run by `make _test_hunt` (part of
`make test` on a machine with KLEE installed). Each `<name>.osp` has a
`<name>.expected` listing the findings Hunt must report, one `<class>
<file>:<line>` per line, or `none`. The replay verdict is not part of the
expectation; the findings are the same on every host [SYMEX-HOSTS].

Osprey programs cannot index out of bounds or dereference null by
construction, so a `memory`, `abort` or `arith` finding here is a compiler,
runtime or mock defect. `json_empty.osp` was added when the corpus sweep found
`close_container` in `json_runtime.c` forming `NULL + 0` for an empty
container (a klee-only `memory` finding reported at the Osprey call).
`record_update_list.osp` pins osprey#293: a record update whose new field
value was a list literal of records stored the literal's flat layout where
the field promised a runtime list, and the next `listGet` segfaulted. Hunt
reported it as a `memory` finding at the `listGet` line, replayed and
`reproduced (SIGSEGV)`, before the fix in `aggregate::settled_field_value`.

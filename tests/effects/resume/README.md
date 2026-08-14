# Explicit resume and early-exit tests

These paired Default/ML suites exercise handlers whose regions contain
`resume`. The handled computation pauses at `perform`; `resume(value)` supplies
that operation's result and runs the remaining computation. Returning from the
selected branch without `resume` stops the remaining work and makes the arm's
value the answer of the whole handler.

The suites cover:

- supplied integer, string, tagged-union and whole-`Result` values;
- `resume()` for `Unit` operations;
- post-resume code and LIFO settlement order;
- early exit that removes every later performer-side observation;
- recursive retry under the same deep handler;
- nested independently typed recovery channels;
- same-effect inner-handler shadowing and outer restoration;
- outer effects that remain available across resume;
- a handler transforming the completed continuation answer;
- 32 sequential suspensions settling exactly once;
- integer, string, boolean and `Unit` operations in one handler;
- string continuation answers, asserted to release under ARC — formerly the
  leak in [#185](https://github.com/Nimblesite/osprey/issues/185); and
- the operation-argument boundary: a 17-argument operation with every position
  checked — formerly truncated after the 16th by
  [#182](https://github.com/Nimblesite/osprey/issues/182).

The older focused files retain their complete former stdout transcripts as
internal oracles, then assert operation counts, supplied values, abort behavior,
handler reachability and settlement order.

Resume is currently deep, single-shot and native-only. A second resume of a
completed continuation aborts with a clear diagnostic. Direct handlers that do
not contain `resume` do not use this runtime and can compile to WebAssembly.
A resuming region may finish with a dynamic string under every memory backend;
the case is asserted here rather than avoided.

Run this category with:

```sh
target/release/osprey test tests/effects/resume
```

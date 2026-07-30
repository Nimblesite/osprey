# Direct recovery tests

The paired `direct_recovery.test.osp` and `direct_recovery.test.ospml` suites
cover handlers whose regions contain no `resume`. In this mode an arm supplies
the current operation result, and execution continues after `perform`.

The ten named cases check:

- a successful path never enters its recovery arm;
- a fallback value reaches ordinary work after `perform`;
- repeated recoveries share handler-owned state;
- `Unit` reports collect every validation error;
- one effect can declare integer, string and boolean operations;
- one effectful function can run under different local policies;
- an inner handler overrides one operation, falls through to an outer arm for
  another operation, and restores the outer policy afterward;
- a handler arm and the continued body can both use a different outer effect;
- handler lookup reaches through helpers and recursion; and
- a whole `Result<T, E>` operation value crosses the direct handler boundary
  intact — formerly the corruption in
  [critical issue #183](https://github.com/Nimblesite/osprey/issues/183).

That final case was written to return `Pass` automatically when the correct value
arrives and an explicit `Skip` while #183 remained reproducible, so it could never
label corrupted data as passing. **#183 is fixed, so it now returns `Pass`** in
both flavors under all three memory backends — the self-skipping shape is what
made the transition observable rather than silent.

Run only these twins with:

```sh
target/release/osprey test tests/effects/errors
```

# Handler lifetime and scope tests

These paired Default/ML suites pin the parts of the [effects contract](../../../../../docs/specs/language/0017-AlgebraicEffects.md) that decide *which* activation answers and *when* an activation ends:

- `finally` cleanup runs exactly once per activation, after the return clause, inner to outer, and when a control arm abandons the remainder (`[EFFECTS-FINALIZATION]`, `[MULTI-COST-ABORT]`);
- `mask E` skips one enclosing activation per level and keeps handlers installed inside its body (`[EFFECTS-MASK]`);
- `instance E` creates a capability that only its own handlers answer, distinct from the default identity (`[EFFECTS-INSTANCE]`).

These forms have shipped ([Plan 0016](../../../../../docs/plans/0016-algebraic-effects-and-handlers.md)), and the suites pass under the default, GC and ARC memory modes. The goldens record the behaviour the specification requires.

```sh
target/release/osprey test src/osprey/tests/effects/scope
```

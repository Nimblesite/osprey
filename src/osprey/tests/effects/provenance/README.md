# Callable provenance through recursive unions

`recursive_union_callable` is a Default/ML twin pair proving that a closure
stored in a recursive union keeps its effect provenance when it is collected
through a `fold` over a list, projected back out with `listGet`, and invoked.

Before #275 the compiled program was rejected with:

```
program entry invokes a dynamic callable whose effect provenance cannot be
proven; preserve the callable through a statically tracked value path
```

Every closure stored in the tree is a non-escaping, effect-free constructor
lambda. The twins also pin [EFFECTS-FIELD-ROWS]: a `Control` whose `press`
field writes `![]` stays callable after crossing `map`, `toList` and `mapList`,
where its own provenance is not tracked. [EFFECTS-PROVENANCE] requires analysis of recursive aggregates to reach
a finite fixed point rather than discarding the callable's provenance.

Run the twins with:

```sh
target/release/osprey test src/osprey/tests/effects/provenance
```

# Explicit resume tests

These paired Default/ML suites replace the former stdout-golden `resume_*`
examples. They assert the complete legacy event transcript plus internal
operation counts, supplied values, abort behavior, handler reachability, and
LIFO continuation settlement.

Run this category with:

```sh
target/release/osprey test tests/effects/resume
```

# Chapter 1 examples

The complete Default-flavor sources are the teaching authority. `first-flight.ospml` is an optional translated twin used only to prove the Chapter 1 flavor aside.

From `Book/`:

```sh
make check-examples
```

The target checks every source, runs each executable example, and compares stdout byte-for-byte with the matching `.expectedoutput` file.

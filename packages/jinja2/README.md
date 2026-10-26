# @osprey/jinja2

An Osprey Python-flavor port of jinja2: lexing, parsing and rendering of the Jinja2 chat-template subset.

Values use the port's typed representation. This is a template subset, not the complete Python object model or extension ecosystem.

The [Python-flavor ports contract](../../docs/specs/ecosystem/0043-PythonFlavorPorts.md) defines [PORT-SUITE] and [PORT-TEST-ORACLE]. The assertions in `tests/main.py` compare rendering with the generated CPython Jinja2 oracle in `tests/expected.py`. Macro declarations can be parsed, but rendering a macro returns an explicit error; the suite checks that limit. `src/` is Osprey source, not CPython; run it with the Osprey compiler.

```sh
target/release/osprey packages/jinja2/tests --run
target/release/osprey packages/jinja2/tests --run --memory=gc
OSPREY_ARC_DEBUG=1 target/release/osprey packages/jinja2/tests --run --memory=arc
```

The Rust integration harness discovers `tests/osprey.toml`, compares stdout with `tests/expected_output.txt` under all three memory backends, and requires ARC to exit without live objects. Presence of a suite does not establish that it passes; compiler diagnostics, assertion failures and missing golden files remain failures.

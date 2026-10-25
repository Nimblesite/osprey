# Python interoperability

Call compiled Osprey functions from an existing Python application and call Python libraries back from Osprey. The [effects example](../examples/python/interop.osp) uses the same operation with live Python and a deterministic test handler.

The substantial application is [LLM dataset audit](osprey/dataset_audit/README.md): an installed tool that produces clean fine-tuning datasets, checks evaluation leakage and context budgets, and quarantines sensitive or invalid records. Its Python-flavor effect driver calls Python data/tokenizer services, which call native Osprey policy back for every record. It has been run on all 15,011 Dolly records with the actual GPT-2 tokenizer.

Build the compiler/runtime first. Install this local package and select that compiler. This example uses a Linux `.so`; on macOS, use `.dylib` in both commands and Python code:

```sh
python3 -m pip install -e ./python
export OSPREY=/absolute/path/to/Osprey/target/debug/osprey
"$OSPREY" examples/python/interop.osp --compile --target=python --memory=none -o interop.so
```

The Python-library target requires explicit `--memory=none`. The current `osprey.build` helper omits that flag, so it and `python3 -m osprey` need updating for the GC-default compiler. Until then, compile explicitly as above and use `osprey.load` below. Loading installs the Python callbacks before running the entry. The package needs only Python's standard library; compiling an artifact also needs Osprey and clang.

```python
import osprey

library = osprey.load("./interop.so")
print(library.squareRootText(81.0))
print(library.mockedSquareRoot())
```

Python exceptions return explicit error data for Osprey to handle. `library.keep(tensor)`, `resolve(id)` and `release(id)` pass opaque Python references without serializing their payloads. The optional PyTorch part of the example calls a Python tensor method; it does not run an Osprey GPU kernel or transfer tensor storage into an Osprey buffer.

The [Python-flavor example](../examples/python/interop.py) also compiles and calls Python through typed `@extern` signatures. The bridge is shared by Default, ML and the implemented Python frontend subset. The proposed `@python("module", "callable")` marker is not delivered syntax.

Current boundaries: Linux/macOS, scalar native exports, one loading thread, retained GIL, explicit unmanaged `--memory=none` allocation and explicit object release. [Spec 0042](../docs/specs/platforms/0042-PythonInterop.md) defines the exact supported protocol and [plan 0032](../docs/plans/0032-python-interop.md) tracks typed direct calls and tensor storage.

Run integration tests with `cargo test -p osprey-cli --test python_interop`. They use the compiler Cargo builds and do not require PyTorch, NumPy or a GPU.

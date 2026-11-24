# @osprey/xdsl

Reads MLIR generic syntax into typed Osprey records. It is a Python-flavor port of the reading half of [xDSL](https://github.com/xdslproject/xdsl), the Python compiler toolkit, licensed Apache-2.0 WITH LLVM-exception; see [NOTICE.md](NOTICE.md).

The first user is the IR to Lean framework ([IR-LEAN](../../../../docs/specs/tooling/IrToLean.md)), which reads the compiler's IR after `mlir-translate --import-llvm` and `mlir-opt --mlir-print-op-generic`.

```python
from xdsl import Parser

match Parser.parse_module(unwrap_or(readFile("module.mlir"), "")):
    case Success(value=module):
        print(module.name)
    case Error(message=message):
        print(message)
```

Status: in progress. Only the generic form is read, not dialects' custom assembly; see [the plan](../../../../docs/plans/ir-to-lean.md).

## Tests and oracle

The suite in `tests/` compares the reader with the real xDSL. [oracle/xdsl.cpython](oracle/xdsl.cpython) runs xDSL at commit `efd7a0e7` on the same inputs and writes `tests/expected.py`. Run it from a virtualenv with that xDSL installed, as `python -I src/osprey/packages/testing/oracle/oracle.cpython xdsl`.

The suite checks these against xDSL:

- every token's kind, text and line, and every lexical error message;
- the verdict on 25 types and 34 attributes, with every type and attribute this reader models rendered in full;
- the shape of every operation in the compiler's generic MLIR for a list function and a branching function.

It also checks the reader's own line-numbered error messages.

## Differences from xDSL

The oracle found these, and the suite pins each one:

- **Forms the reader does not model stay as raw text.** The reader keeps a type or attribute it does not model and reports it as such. xDSL refuses some of these: an undefined alias such as `#loc`, an unknown bare type such as `ix`, and a malformed LLVM type such as `!llvm.array<x i8>` or `!llvm.struct<(i64>`. The reader keeps them, because `mlir-opt` output refers to location aliases the reader does not resolve.
- **Typed strings are read.** MLIR's typed string `"…" : i8`, which `mlir-opt` prints for LLVM globals, is read as a string. xDSL refuses it.
- **Hex float patterns are refused.** A hexadecimal `f64` bit pattern such as `0x7FF0000000000000 : f64` is refused as an integer too large for 64 bits. xDSL reads it as a float.
- **One xDSL message is fixed.** At this commit xDSL prints `Expected suffix identifier after {first_char}` with the template unfilled. The reader names the character.

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

Status: in progress. Only the generic form is read, not dialects' custom assembly. The test suite and its xDSL oracle are not written yet; see [the plan](../../../../docs/plans/ir-to-lean.md).

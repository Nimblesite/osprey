# @osprey/xdsl: licensing and upstream acknowledgement

The Osprey implementation is provided under the normal [MIT license](LICENSE), Copyright (c) 2026 NIMBLESITE PTY LTD, Australia. The package license expression is `MIT AND Apache-2.0 WITH LLVM-exception`. This does not claim ownership of any third-party material identified below.

## Retained upstream material

The four modules in `src/` are a port of the MLIR reader in [xDSL](https://github.com/xdslproject/xdsl), compared against commit `efd7a0e7c656ea81ef64037aa98f6d468043c335`:

| File | Ported from |
| --- | --- |
| `src/lexer.py` | `xdsl/utils/mlir_lexer.py` |
| `src/ir.py` | `xdsl/ir/core.py`, `xdsl/dialects/builtin.py` |
| `src/attrs.py` | `xdsl/parser/attribute_parser.py`, `xdsl/dialects/llvm.py` |
| `src/parser.py` | `xdsl/parser/core.py` |

The port follows xDSL's structure, token kinds, grammar comments and error messages, so those files are treated as derived from xDSL and carry its license, Apache-2.0 WITH LLVM-exception, in [LICENSES/xDSL-Apache-2.0-WITH-LLVM-exception.txt](LICENSES/xDSL-Apache-2.0-WITH-LLVM-exception.txt). Copyright in the original xDSL code belongs to the xDSL project and its contributors. The Osprey changes to those files are also offered under MIT.

## Distribution

The publisher carries `LICENSE`, this acknowledgement and the `LICENSES/` file in the digest-pinned release. The upstream project name provides acknowledgement and implies no endorsement.

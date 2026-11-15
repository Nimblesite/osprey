# One project, four syntax flavors

A **syntax flavor** is a way to write Osprey. This project imports between all four: the entry and `Pricing` are TypeScript flavor, and `Pricing` imports `Units` (Default, [`units.osp`](src/units.osp)), `Rounding` (ML, [`rounding.ospml`](src/rounding.ospml)) and `Tax` (Python flavor, [`tax.py`](src/tax.py)). Every file declares the same namespace, `shop`, so a TypeScript relative import such as `import { cents } from "./units";` names module `Units` of that namespace, whatever flavor the module is written in.

```sh
osprey src/osprey/examples/projects/four_flavors --run
```

prints `price=770`: seven dollars in cents, with ten percent tax, rounded down to a multiple of five. The entry installs an `Arith` handler for the remainder and integer division the other modules perform. `make test` runs it under GC and ARC through `project_e2e.rs` ([FLAVOR-TS-INTEROP](../../../../../docs/specs/language/0054-TypeScriptFlavorSyntax.md)).

# Math to code [MATH-CODE]

**Status:** fragment 1 (`Float` and `Bool` definitions) is implemented in `src/toolchain/crates/osprey-math`. Every other fragment below is a delivery target of [plan math-to-code](../../plans/math-to-code.md), and a claim about it is not evidence.

## Aim [MATH-CODE-AIM]

Implement production formulas in versioned native Osprey modules. The proof tooling now makes it straightforward to prove properties of supported Osprey code with Lean. It reads the production function and presents that definition to the theorem. Keeping the algorithm in Osprey gives developers one implementation to maintain and makes changes to that implementation visible to the proof. For a theorem about a production function, the proof build must derive its executable definition from that function’s parsed Osprey body, including the callees on which the claim depends. State the domain, primitive semantics and arithmetic assumptions. Check the certificate with the Osprey kernel and exercise the proof with a source edit that violates its conclusion. The [synthesis specification](../synth/Synthesis.md) identifies the verified functions and the outstanding obligations.

Record a less readable Osprey expression in the [ML notation specification](../language/0024-MLFlavorSyntax.md#mathematical-notation-flavor-ml-math-notation) and [implementation plan](../../plans/math-to-code.md#native-osprey-formulas-math-code-native-notation) with the two source forms, the semantic difference if any, and a check for the proposed improvement.

## Lean proof sources and import tooling [MATH-CODE-LEAN]

Theorem statements and proofs use the pinned Lean 4 format specified by [PROOF-SOURCE](../language/0049-DependentTypesAndProofs.md). Osprey parses production algorithms using its ordinary language frontend. The remaining sections describe the existing Lean import facility used by packages with Lean inputs and by the Proof Workbench. An imported definition must be accepted by the pinned Lean, and the importer must reject unsupported constructs explicitly.

### Deleted format [MATH-CODE-DELETED]

The definition document proposed earlier (`Function … over reals with …`, `Suppose`, `Constant`, `Arithmetic`, `Example` sentences, its term language and its notation rows) was never implemented and is deleted. No reader, writer, test, golden or lesson may be written for it.

## Sources [MATH-CODE-SOURCES]

A `.lean` file under a project's `source_roots` is a math source. Discovery is the project loader's ([MODULES-PROJECT](../language/0025-ModulesAndNamespaces.md)), so the command line, the language server and the test runner see the same sources. There is no manifest key and no marker.

A Lean development that is not program mathematics (a Lake project, a proof corpus) stays outside `source_roots`.

## The fragment [MATH-CODE-FRAGMENT]

The reader accepts the constructs below and refuses everything else by name ([MATH-CODE-REFUSALS]). A construct outside the fragment is never skipped, approximated or guessed.

| Command | Reading |
| --- | --- |
| `namespace N` … `end N` | at most one block, holding every definition; `N` has one or two components |
| `def name (a b : T) (c : T) : T := term` | one generated function; no parameters gives a constant |
| `private def …` | generated, not exported |
| `open Float (exp log)` | the listed `Float` functions may be written unqualified from there on, as Mathlib writes them under `open Real`; the list is required, so no other name changes meaning |
| `/-- text -/` before a `def` | the documentation of the generated function |
| `#eval`, `#check`, `#guard`, `#print` | checked by Lean; generate nothing |
| `--`, `/- … -/`, `/-! … -/` | comments |

`T` is `Float` or `Bool`.

A command of the fragment is recognised by its word. Any other command, and anything else that starts a line in column 0, ends the command before it and is refused by the word the source uses.

**A math source holds no theorem and no proof.** A statement about its definitions lives in a proof file outside the source roots that imports the math source, and the proof side checks it ([PROOF-SOURCE](../language/0049-DependentTypesAndProofs.md)). The toolchain crates therefore name no proof vocabulary ([TYPES-DISCIPLINE-REGISTRY](../language/0050-TypeDisciplines.md)).

| Term | Notes |
| --- | --- |
| `2`, `2.5`, `1.5e-3` | a `Float` literal; decimal digits, optional fraction and exponent |
| `true`, `false` | |
| a parameter, a `let` name, a definition of the same file | definitions are read in order; a use before its definition is refused |
| `f a b` | arguments on the head's line |
| `-a`, `a + b`, `a - b`, `a * b`, `a / b`, `a ^ b` | Lean's precedence and associativity |
| `a < b`, `a ≤ b`, `a > b`, `a ≥ b`, `a == b`, `a != b` (`<=`, `>=`) | over `Float`; `==` and `!=` also over `Bool`, and `≠` over `Bool` only, because Lean cannot decide `=` or `≠` between `Float` values |
| `a && b`, <code>a &#124;&#124; b</code>, `!a`, `a ∧ b`, `a ∨ b`, `¬a` | over `Bool` |
| `if c then a else b` | as a definition body, a branch, a `let` value or a `let` body |
| `let x := a` then a line break or `;` then `b` | as a definition body, a branch or a `let` body |
| `Float.sqrt a`, `a.sqrt` | the functions of [MATH-CODE-MEANING]; both spellings |
| `( a )` | |

A definition may take the name of an Osprey builtin, as Mathlib names its own `sinh`, provided the source never calls that builtin: the generated call would reach the definition instead, so a source that does both is refused (`ReservedName`). A parameter or a `let` never takes a builtin's name.

The reader checks what it reads: every name resolves, every call has its declared arity and argument types, both branches of an `if` have one type, and a body has its declared result type.

## Meaning [MATH-CODE-MEANING]

Lean's `Float` and Osprey's `float` are both IEEE-754 binary64, and Lean's `Float` operations are the C library's. Each row is emitted only because both sides compute the same value; a Lean operation with no equal Osprey operation is refused, never approximated.

| Lean | Osprey | Why they agree |
| --- | --- | --- |
| `a + b`, `a - b`, `a * b` | the same operator | IEEE, total on both sides |
| `a / b` | `a / b` when `b` is a nonzero literal, otherwise `floatDiv (a, b)` | Lean's `/` is the IEEE quotient; Osprey's `/` asks `Arith.divideByZero` at a zero divisor and `floatDiv` never does ([BUILTIN-MATH-FLOATDIV](../runtime/0012-Built-InFunctions.md)) |
| `-a` | a negative literal, otherwise `-1.0 * a` | IEEE multiplication by `-1.0` flips the sign of every non-NaN value, zeros included; a NaN stays a NaN and nothing in the fragment observes its sign. Osprey's unary `-` on an untyped name selects integer negation, so it is not emitted |
| `a ^ b`, `Float.pow a b` | `pow (a, b)` | both are C `pow` |
| comparisons | the same operator | IEEE on both sides: every ordered comparison with a NaN is false, `!=` is true |
| `&&`, <code>&#124;&#124;</code>, `!`, `∧`, `∨`, `¬` | `&&`, <code>&#124;&#124;</code>, `!` | |
| `if c then a else b` | `match c` with `true` and `false` arms | |
| `Float.sqrt`, `.exp`, `.sin`, `.cos`, `.tan`, `.asin`, `.acos`, `.atan`, `.sinh`, `.cosh`, `.tanh`, `.asinh`, `.acosh`, `.atanh`, `.log2`, `.log10`, `.floor`, `.ceil`, `.abs` | the builtin of the same name | the same C library function ([BUILTIN-MATH](../runtime/0012-Built-InFunctions.md)) |
| `Float.log` | `ln` | natural logarithm on both sides |
| `Float.atan2 y x` | `atan2 (y, x)` | |

Refused because the two sides differ or Osprey has no equal: `Float.round` (C `round` takes ties away from zero; Osprey's `round` takes them to even), `Float.exp2`, `Float.cbrt`, `Float.scaleB`, `min`, `max`, and every conversion to or from an integer type.

### Literals [MATH-CODE-LITERALS]

A literal is emitted as the same decimal: digit for digit, with `.0` appended to an integer and an exponent applied by moving the point. A literal that is not finite in binary64 is refused. Lean's `OfScientific` and Osprey's literal reader are separate conversions, so their agreement is checked, never assumed: the oracle compares the bits of every literal of every tracked source ([MATH-CODE-ORACLE]).

## Generated module [MATH-CODE-EMIT]

One math source generates one ML-flavor module.

- `namespace a.M` generates `namespace a` and `module M`; `namespace M` generates `module M` in the project's default namespace; a source with no namespace generates `module <file stem>`.
- A `def` is `export`ed; a `private def` is not. The generated Osprey is as tight as the Lean source [MATH-CODE-EMIT-ELEGANT]: a parameter carries an annotation only when Osprey's checker would otherwise give it a type other than its Lean type, and a definition keeps a `name : type` header only when annotating every parameter still leaves its result open. The emitter does not predict the checker; it asks it. It emits the module, infers it, annotates the first parameter of each definition that does not infer its Lean type (a `Float` may also infer `number`, which keeps every float a caller passes), and repeats until every definition matches. The checker is the only authority because the rules are not obvious: arithmetic between open values defaults them to `int` (`b * k * k` in `1.0 + b * k * k`), and `floatDiv` is open over `int` too, so `g (w, q) = w * w + floatDiv (w, q)` left bare takes `int`. `every_generated_definition_infers_its_lean_types` in `src/toolchain/crates/osprey-math/tests/generate.rs` checks the property against the checker itself.
- Documentation is the Lean docstring; a public definition without one is documented by its name and source file.
- The bytes are a function of the source text and file name alone. Generation reads no clock, locale, environment or other file.
- Every definition of the source appears in source order, or nothing is generated.
- The module is already in the formatter's form: `osprey fmt --check` accepts it unchanged.

### Untracked by default [MATH-CODE-UNTRACKED]

The module is written to `<project>/.osprey/math/<source path>.ospml`, and only when its bytes change. The generator writes a `.gitignore` holding `*` into `.osprey/math/`, so in any repository generated modules are untracked unless someone deliberately runs `git add -f`. A generated module is never a source of truth: a build regenerates it from the `.lean` file and overwrites any edit.

## Refusals [MATH-CODE-REFUSALS]

A refusal names the 1-based line, the column in Unicode scalar values and the shortest change that would make the source readable. It is reported at the `.lean` path. A source with any refusal generates nothing, and the project does not build ([COMPILER-RED-FAILS](../../../AGENTS.md)).

The vocabulary is closed, and each code has a test with a minimal near-miss: `UnsupportedCommand`, `UnsupportedTerm`, `UnsupportedType`, `UnsupportedFunction`, `UnsupportedLiteral`, `LiteralOutOfRange`, `UnknownName`, `ReservedName`, `ShadowedName`, `DuplicateName`, `ArityMismatch`, `TypeMismatch`, `NestedConditional`, `MultilineApplication`, `NamespaceShape`, `UnexpectedToken`.

A type error inside a generated module is a generator defect, reported at the generated path, never a fault in the user's mathematics.

## What generation does not claim [MATH-CODE-CLAIMS]

A generated function is not a proof and claims no theorem. `Float` is binary64, not ℝ: a definition over `Float` is a floating-point algorithm, and its distance from the real-valued formula is not measured here. The evidence that generated code computes what Lean computes is the oracle below, on the points it records, and nothing beyond them.

## Verification [MATH-CODE-VERIFICATION]

| Check | Proves |
| --- | --- |
| Reader and emitter goldens, in `osprey-math` | every fragment row reads and emits; every refusal fires on its near-miss; output is byte-identical across runs |
| Compiler agreement | every generated golden is accepted by the real compiler |
| Lean acceptance [MATH-CODE-ORACLE] | every tracked math source is accepted by the pinned Lean; for the oracle corpus, the bits Lean computes equal the bits the generated module computes under GC and ARC, including every literal |
| Converted packages | a package whose mathematics moved to `.lean` keeps its recorded outputs byte for byte |

Lean acceptance runs where the pinned Lean is installed (`make -C src/osprey/examples/proof math`); the other rows run in `make ci` with no Lean.

## Commands [MATH-CODE-COMMANDS]

- `osprey math <file.lean>` prints the generated module, or the refusals with exit code 1. It writes nothing.
- Checking, building, running or testing a project generates its math sources first.

## Standard library [MATH-CODE-LIBRARY]

`@osprey/math` is Osprey's standard library of well-known formulas, and it holds no Osprey code: every function is a Lean definition. Where Mathlib defines or states the formula, the definition is Mathlib's formula at the pinned commit (`1ccd71f89cbbd82ae7d097723ce1722ca7b01c33`), the docstring names the Mathlib declaration and quotes its source, and the package carries Mathlib's Apache-2.0 licence and the authors' notices. The definitions are over `Float`, so three Mathlib notations that core Lean does not define for `Float` are respelled: `x⁻¹` as `1 / x`, `√x` as `sqrt x` and `π` as `pi`. A function added to the library follows the same rule: find the Mathlib declaration first, and restate it.

## Workbench [MATH-CODE-WORKBENCH]

The Proof Workbench's "mathematics in, Osprey out" activity ([spec 0051](../applications/0051-ProofWorkbench.md)) edits a `.lean` document and displays what the toolchain generated from it. The application holds no reader, generator or second format. It shows four statuses, each from its own evidence and none implying another: `Generated` (this revision generated), `Typechecked` (the compiler accepted those exact bytes), `Tested` (recorded points agree) and `EquivalenceProved` (a kernel-checked certificate; unavailable, and shown as unavailable). An edit clears every status before a new one is granted.

## Limits and follow-on fragments [MATH-CODE-LIMITS]

Not in fragment 1, each refused by name until its fragment is specified with its own meaning table, refusals and oracle: integer types (`Int64`, `Nat`, `Int`); `structure` and record values; `match`, recursion and `where`; `fun` and function-typed parameters; imports between math sources and `open`; definitions over Mathlib's `ℝ`, which need a declared binary64 approximation profile; solving, summation, differentiation and any other form whose algorithm the source does not state.

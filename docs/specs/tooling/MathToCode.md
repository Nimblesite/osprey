# Math to code [MATH-CODE]

**Status:** fragment 1 (`Float` and `Bool` definitions) is implemented in `src/toolchain/crates/osprey-math`. Every other fragment below is a delivery target of [plan math-to-code](../../plans/math-to-code.md), and a claim about it is not evidence.

## Aim [MATH-CODE-AIM]

Mathematics is written as mathematics, in a `.lean` file. The toolchain generates the Osprey module that computes it. Nobody writes or maintains mathematical code in Osprey: the Lean definition is the source, and the Osprey module is build output.

```lean
namespace physics.Motion

/-- Seconds for a body to fall `h` metres from rest. -/
def fallTime (h : Float) : Float := Float.sqrt (2 * h / 9.80665)

end physics.Motion
```

An Osprey program calls `Motion::fallTime 19.6133` with no further step: no generator to run, no file to commit, no annotation to keep in step.

## Lean is the format [MATH-CODE-LEAN]

The source format is Lean 4, exactly as the pinned toolchain reads it (`leanprover/lean4:v4.25.0`, the pin of [PROOF-SOURCE](../language/0049-DependentTypesAndProofs.md)). Every math source MUST be accepted by that Lean with no error. The generator reads a fragment of Lean; it never reads anything Lean would reject, and it gives every construct the meaning Lean gives it.

**Osprey-specific formats for mathematics are prohibited.** No notation, sentence format, definition document, keyword, marker, header, dialect or extension of Lean may be designed, specified, implemented or extended. The single exception: functionality Lean's format cannot express. An exception needs an entry in this specification that names the missing Lean capability and cites a Lean source showing the gap, and it ends the day Lean gains the capability. "Lean is verbose", "the reader does not support it yet" and "a custom notation is nicer" are not gaps: the remedy for an unread Lean construct is to extend the reader ([MATH-CODE-FRAGMENT]).

Reasons: proofs are already `.lean` and nothing else ([PROOF-SOURCE-DELETED](../language/0049-DependentTypesAndProofs.md)), so one format serves statements, proofs and programs; a theorem about a definition imports the very file the program is generated from; Lean's own checker, editor support and libraries apply to every source with no work here.

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
- A `def` is `export`ed with a signature carrying its declared types, so the API is the one Lean checked and never an inferred generalisation. A `private def` has no `export` and no signature.
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

# Formula code generation [WORKBENCH-CODEGEN]

**Status:** proposed; formula-to-code generation is not implemented. The workbench of [spec 0051](0051-ProofWorkbench.md) implements a formula reader, a goal engine, a proof kernel and an emitter that writes a Lean-flavoured proof module ([WORKBENCH-EMIT]). That emitter writes proof source from the current proof document; kernel verdicts are obtained separately. It is not a numeric algorithm generator, it parses no arithmetic, and it types no `int` or `float` expression. This specification describes a second activity the same application will offer — mathematics in, executable Osprey out — and distinguishes the two. Delivery: [plan formula-code-generation](../../plans/formula-code-generation.md). Every new feature rule here is a delivery target. Existing primitives and the framework/application ownership model are reference contracts, not evidence of numeric code generation; see [spec 0051](0051-ProofWorkbench.md) and [NATIVE-HOSTS](0047-OspreyNativeHosts.md).

## Aim [WORKBENCH-CODEGEN-AIM]

A scientist has a formula from a paper. They type it as mathematics, without learning Osprey, and they get a function they can call: readable source, highlighted, exportable, and — where the host can run a compiler — typechecked by the real Osprey compiler. The workflow needs no programming language knowledge at any step; the source pane is optional and closed initially, exactly as [WORKBENCH-STORE](0051-ProofWorkbench.md#store-workbench-store) treats generated proof text.

Safety comes from explicit domains, failure results and truthful validation; elegance from entering mathematics without programming syntax; performance from generated native Osprey, with measurements required before any speed claim.

The generated artifact is **Osprey source**, because Osprey is the target this repository can actually check, quote provenance for and keep honest. Other source languages are a stated future possibility, not a promise in this fragment: the semantic layer below is language-neutral, and a later backend is a new emitter over the same typed IR, never a second application.

## Related activities [WORKBENCH-CODEGEN-SCOPE]

Mathematics in, code out, is four different jobs. Confusing them is how a tool ends up claiming more than it checked.

| Activity | Input | Output | Where |
| --- | --- | --- | --- |
| Expression evaluation | a closed expression over known values | one number | **In scope**, as the value mode of a generated function |
| Executable function generation | a definition naming a function, its inputs and its domain | a pure Osprey function and its source | **In scope; the first fragment** |
| Solver synthesis | an equation or constraint with no isolated variable | a solved form, a root, a search procedure | **Out of scope**; refused by name until a specified algorithm exists |
| Proof extraction | a statement and steps | a checker term and a verdict | Out of scope here; it is [WORKBENCH-EMIT](0051-ProofWorkbench.md#generated-code-workbench-emit) and [WORKBENCH-VERDICT](0051-ProofWorkbench.md#verdict-workbench-verdict), unchanged |

A generated function is **not** a proof and never claims a theorem. The two panes may sit in the same window; they share no verdict, no status and no receipt.

## Why an equation is not an algorithm [WORKBENCH-CODEGEN-UNDERDETERMINATION]

`y = x² + 1` determines a set of points and nothing else. It does not determine which symbol is the output, in what order a caller supplies the inputs, whether the numbers are reals or machine integers, which branch a piecewise definition takes at a boundary, how `x²` is computed, what happens for `x` outside the mathematical domain, or how much rounding the answer carries. Every one of those is a decision, and each one is a place where a tool that guesses produces plausible wrong numbers.

The rule of this specification: **the document makes the decision, or the workbench refuses and says which decision is missing.** No default exists for any row below.

| Decision | Who makes it | Refusal when absent |
| --- | --- | --- |
| Which symbol is the output | `Function` heading and the definition sentence's left side | `ImplicitEquation` |
| Which symbols are inputs, and their order | `Function` heading | `UnknownInput` |
| Real or integer arithmetic | the heading's `over reals` / `over integers` | `NumericModeMissing` |
| Integer overflow policy | the document's `Arithmetic` sentence | `ArithmeticPolicyMissing` |
| Integer division semantics | nobody in this fragment | `IntegerDivisionUnspecified` |
| Domain conditions and what failure returns | `Suppose` sentences | none: an unstated domain is an unrestricted one, disclosed in the receipt |
| Branch order at a shared boundary | written order, first true guard wins | none: the semantics are defined, not inferred |
| Exhaustiveness | the final `otherwise` | `GuardedGap` |
| Which glyph means π or e | the document's `Constant` sentence | `UnknownName` |
| Which root, parametrisation or solver | nobody in this fragment | `RelationNotFunction`, `UnboundedSynthesis` |
| Numerical representation | an explicit exact-integer or binary64 approximation profile | `NumericModeMissing`, `LiteralOutOfRange` |

A relation is refused rather than solved. `x² + y² = 1` under `Function y over reals with x` is `ImplicitEquation`, and the workbench's fix names the branch it will not choose: "write `y(x) = sqrt(1 - x^2)` and suppose `1 - x^2 ≥ 0`, if the upper branch is what you mean".

## The definition document [WORKBENCH-CODEGEN-DOCUMENT]

A definition document is a second document kind in the same workbench store, alongside the proof document. It is a sequence of `Function` blocks; a block begins at a `Function` heading and ends at the next heading or at end of buffer.

```
Function fall_time over reals with h.
Suppose h ≥ 0.
Constant g = 9.80665.
fall_time(h) = sqrt(2 * h / g).
```

| Sentence | Reads as | Rules |
| --- | --- | --- |
| `Function <name> over reals\|integers with <in>, <in>.` | a function block | ordered inputs within the document resource bound; omit `with …` for a zero-input definition; names unique in scope |
| `Suppose <condition>.` | one domain condition | one comparison, or comparisons combined with `and`, `or`, `not`, over this block's inputs and constants |
| `Constant <name> = <numeral>.` | a named value, block-scoped | numeral rules of [WORKBENCH-CODEGEN-NUMERICS]; never inferred to mean π or e |
| `<name>(<in>, …) = <expression>.` | the whole body | left side must name this block's function with exactly its inputs, in order |
| `<name>(<in>, …) = <expression> if <condition>.` | one branch of a piecewise body | every branch but the last carries `if`; the last is `otherwise` |
| `Arithmetic wrapping\|saturating\|checked.` | the integer overflow policy for the block | required when the block is `over integers`; governs every integer operation; never inferred, and allowed even when no operation needs it |
| `Example <name>(<args>) = <numeral>.` | one recorded evaluation, for `Tested` | exact agreement; `≈ <numeral> within <numeral>` states an absolute tolerance instead |

A `Constant` or `Example` sentence belongs to the block that precedes it. `Example` sentences are test data, not code: they never appear in the generated module and never change its bytes, so they matter only to `Tested`.

Readings and faults carry a 1-based column counted in **Unicode scalar values**, with exact UTF-8 byte spans retained for caret and host mapping. This proposed reader must test those conversions rather than assume the units of the existing [formula reader](0051-ProofWorkbench.md#formulas-workbench-formula). `Definitions::read : string -> Reading` mirrors `Formulas::read`: a `Read Document` or a `Faulted Fault`, because a `Result`'s error channel holds only a message. Every refusal names the column and the shortest thing that would have made the text well formed. A block with any fault is refused whole; a document with one faulted block generates nothing, so a partial algorithm never appears beside a clean preview.

Precedence, loosest first: `+ -`, `* /`, unary `-`, `^` (right associative), application, atoms. Parentheses override. `2 h` is refused as `ImplicitMultiplication` with the fix `2 * h`: adjacency means function application everywhere else in the workbench, and letting it also mean multiplication is how `f (x)` becomes ambiguous.

## The term language [WORKBENCH-CODEGEN-TERMS]

Arithmetic terms are **not** formulas. `Formula` of [WORKBENCH-FORMULA](0051-ProofWorkbench.md#formulas-workbench-formula) is propositional and stays propositional; a numeric expression is never a proposition and is never handed to the kernel. A definition document contains no proposition at all.

```
type Expr =
    | Numeral of string
    | Input of string
    | Constant of string
    | Neg of Expr
    | Add of Expr * Expr
    | Sub of Expr * Expr
    | Mul of Expr * Expr
    | Div of Expr * Expr
    | Pow of Expr * int
    | Call of string * List<Expr>
    | Branch of List<Arm>

type Arm = { guard : Guard, body : Expr }

type Guard =
    | Compare of Comparison * Expr * Expr
    | Both of Guard * Guard
    | Either of Guard * Guard
    | Neither of Guard

type Comparison = Less | AtMost | Greater | AtLeast | Same | Apart
```

`Pow` carries an exact integer exponent, because a non-literal exponent is a `Call` to `pow` and the difference must be visible. Lowering must not coerce a large integer exponent to an inexact float and silently change parity; use an explicitly specified integer-exponent algorithm or refuse with `ExponentNotRepresentable`. The same rule applies to integer-mode exponentiation, including negative exponents that would leave its numeric domain. `Branch` holds the guarded alternatives in written order, and its last arm's guard is the always-true one a final `otherwise` produces; a guard list is never empty.

`Call` is closed: `abs`, `sqrt`, `exp`, `ln`, `sin`, `cos`, `tan`, `atan`, `atan2`, `hypot`, `fmod`, `pow`, `floor`, `ceil`, `trunc`, `round`. Each maps to the shipped builtin of the same name in [BUILT-IN-FUNCTIONS](../runtime/0012-Built-InFunctions.md); a name outside the table is `UnknownFunction`. The mathematical constant `π` is a separate literal in real mode: it emits the canonical binary64 approximation `3.141592653589793`, records that approximation, and never claims exact representation of π. It is not an undeclared variable or a user-defined input. No other builtin is reachable from a document, so the generator cannot emit a string, list, channel, file or process operation.

`Terms::show : Expr -> string` writes the Unicode form with minimal parentheses, exactly as `Formula::show` does, and `Definitions::read (show d) = d` for every document the suite generates. `Terms::typeset : Expr -> Math` builds the existing `Math` tree ([WORKBENCH-MATH](0051-ProofWorkbench.md#typeset-mathematics-workbench-math)); the preview never passes through generated text.

## Domains, totality and the error channel [WORKBENCH-CODEGEN-DOMAINS]

Osprey has no panics and no fabricated values, and neither does anything this generator writes.

- **No `Suppose` sentence, no faulting operation:** the function is total and returns its plain type.
- **Real mode, any `Suppose` sentence, or any operation that can fault:** every condition is evaluated before the body, and the function returns `Result<T, Error>`. A false condition returns `Error "<name>: <condition> required"`, where operators use canonical spellings (`>=`, `<=`, `!=`) and mathematical identifiers retain their Unicode names. Messages are canonical UTF-8 and preserve source mappings.
- **Composition:** a function that calls a `Result`-returning function returns `Result` too, propagating with explicit `match` arms. `?:` is never used: a default value would be exactly the fabricated fallback the language forbids.
- **Real division** is the only division in the fragment. `x / y` with a constant nonzero literal divisor needs no zero-divisor check but still needs a finite-result check; with a variable divisor the document must state `Suppose y ≠ 0` and the generated guard enforces it. A real division with an unguarded variable divisor is `UnguardedDivision`.
- **Integer overflow** is decided by the document's `Arithmetic` sentence: `wrapping` emits `wrapAdd`/`wrapSub`/`wrapMul`, `saturating` emits `satAdd`/`satSub`/`satMul`, and `checked` emits `checkedAdd`/`checkedSub`/`checkedMul` and returns `Result`. The policy also applies to unary negation and integer powers, including `-INT64_MIN`; every builtin must match the selected numeric domain without hidden coercions. Bare integer `+ - *` are never emitted, because they would seed an `Arith` requirement the generated module cannot discharge and must not silently handle ([ARITH-TOTAL](../language/0037-ArithmeticEffects.md#the-guarantee--arith-total)).
- **Integer division, integer `%` and `intDiv` are refused** in this fragment (`IntegerDivisionUnspecified`). Mathematical integer division is floor division and is undefined at zero; Osprey's `intDiv` truncates toward zero and performs `Arith.remainderByZero`. Choosing one is a decision, and this fragment does not have it.
- **Guard coverage** is the document's, and totality of a piecewise body comes only from its final `otherwise`.

## Guarded piecewise bodies [WORKBENCH-CODEGEN-PIECEWISE]

Branches keep the order they were written. A guard list is evaluated top to bottom; the first true guard selects its branch. Overlapping guards are therefore well defined and are not a defect, and the typeset preview numbers the branches so the order is visible rather than inferred from arithmetic.

The last branch is `otherwise`. Without it the document is `GuardedGap` and generates nothing, because the alternative — a fabricated value for the uncovered case — is a wrong answer presented as a right one. A generated piecewise body is a nested `match` over `bool`, the form the workbench's own ML modules already use.

## Names, scopes and hygiene [WORKBENCH-CODEGEN-NAMES]

- Mathematical identifiers may contain Unicode letters, including Greek variables. Preserve the exact input and source spans; use NFC for binding identity, never compatibility-fold distinct mathematical symbols silently. A normalization collision or visually ambiguous binding is `AmbiguousName`, with the conflicting names identified. Generated identifiers must satisfy the actual Osprey parser. When an alias is needed, derive it deterministically from binding identity, prevent collisions, and show the original-to-generated mapping in the source pane and receipt. The user need not rename mathematics to ASCII.
- An ambiguous use of a reserved operator or builtin name (`round`, `sqrt`, …) is `ReservedName`. Code-only binder names and aliases are hygienic and visible in the mapping; a generated helper must never capture or change the meaning of a mathematical binding.
- Constants are block-scoped and are **inlined at each use** as their value, with the name recorded in the generated line map and listed in a header comment. Inlining keeps `/` provably total against a literal divisor, keeps the module free of file-scope state, and makes the emitted bytes depend only on the block.
- The generated binder in a propagated `Result` pattern is drawn from a generator-owned pool (`cause`, `value`). A collision with a document name renames the binder deterministically (`cause_1`) and the receipt lists the renaming. Mathematical names remain unchanged in the document and preview; any code-only alias is recorded explicitly.
- One generated module holds one document: `namespace generated`, deterministic mapped function names, and `export` on every definition so the file is a library another module can import.

## Generated source [WORKBENCH-CODEGEN-EMIT]

`Generate::document : Document -> string` is the only writer of numeric code in the application, as `Emit` is the only writer of proof code. The following is an illustrative, unverified source shape, not a working API or compiler-accepted golden. Implementation must first produce and check the fixture goldens:

```
// osprey: flavor=ml
// generated by the Osprey Workbench; status: unvalidated
// arithmetic: checked
namespace generated

export square_sum n =
    match checkedMul n n
        Error reason => Error reason
        Success square => checkedAdd square n
```

Rules:

- ML flavor, four-space indentation, one blank line between definitions, the marker line and namespace fixed. The marker makes the flavor explicit in a file that may be moved anywhere ([FLAVOR-SELECT](../language/0023-LanguageFlavors.md#flavor-selection)).
- Use inference and omit every redundant annotation. Keep the smallest annotation when the chosen numeric domain cannot otherwise be inferred, such as a real identity function or an unused numeric parameter. A generic identity is not evidence of a concrete binary64 interface. The real compiler's inferred signature must match the declared inputs and output; never add a fake arithmetic operation merely to force a type ([AGENTS](../../../AGENTS.md)).
- No `?:`, no `unwrap`-like builtin, no placeholder body, no `sorry` analogue, no empty branch. Every branch is total or returns `Error`.
- A `// osprey: flavor=ml` module with no definitions is not emitted; an empty document produces no file and a neutral explanation.
- The bytes are a pure function of canonical mathematical input, explicit domains/numerical policies, target source-language profile and generator version; the receipt identifies every one. Two runs on the same input are byte-identical, and generation has no clock, no locale, no host and no random source.
- The source pane is read-only, line-numbered and highlighted with the roles of [WORKBENCH-DESIGN-FEEDBACK](0051-ProofWorkbench.md#proof-feedback-and-source-workbench-design-feedback): keywords, names, types, operators, comments. Editing it is not offered; a deliberate "Copy as ordinary document" action produces an unlinked Osprey file document whose code statuses are all cleared.
- Export writes exactly the bytes shown, through the existing `SaveFile` capability, as `<name>.ospml`, and optionally the receipt as `<name>.receipt.json`. Nothing else is written, and no workspace path is created without that explicit action.

### Line map and diagnostics [WORKBENCH-CODEGEN-LINEMAP]

Each generated line records its origin: a document line, a `Suppose` sentence, a `Constant` inlined on it, or `Generated` scaffolding (marker, namespace, comments). A compiler diagnostic on an origin line underlines the corresponding document span, in code points, in the mathematics pane. A diagnostic on a `Generated` line is not the user's fault and is reported as a generator defect with the receipt attached; it never moves the caret or marks the formula.

## Numerical semantics [WORKBENCH-CODEGEN-NUMERICS]

Mathematics has integers and reals. Osprey has `int` (64-bit two's complement) and `float` (IEEE-754 binary64), and they are not the same thing. The document states the intended domain and separately chooses a bounded integer or approximate binary64 representation; the receipt states that distinction and its consequences.

| Written | Mode | Emitted | Reading |
| --- | --- | --- | --- |
| `7`, `-3` | integers | `7`, `(-3)` | exact; outside `-2^63 … 2^63-1` is `LiteralOutOfRange` |
| `7`, `-3` | reals | `7.0`, `(-3.0)` | exact as binary64 for `|v| ≤ 2^53` |
| `0.1` | reals | `0.1` | nearest binary64; not exactly representable, disclosed |
| `1 / 3` | reals | `1.0 / 3.0` | binary64 division, rounded once |
| `x^2` | reals | `x * x` | a fixed multiplication chain, not libm `pow` |
| `x^2` | integers | `checkedMul x x` (`wrapMul`, `satMul` for the other two policies) | the chosen policy applies to the exponentiation too |
| `x^-1` | reals | `1.0 / x` | requires a `Suppose x ≠ 0` |
| `x^n`, `n` not a literal integer | reals | checked `pow x n` | the declared binary64 profile and its domain checks apply; the receipt records the backend behavior |
| `sqrt`, `ln`, … | reals | checked use of the builtin | intrinsic domain and finite-result failures return `Error`; NaN or infinity is not successful real output |
| `x = y` in a guard | reals | `x == y` | IEEE equality: NaN is unequal to itself and `0.0 == -0.0` is true; the receipt records it, because it is not mathematical equality |

Two consequences are stated rather than papered over.

**Real arithmetic returns `float`, even when the mathematics is integral.** `floor`, `ceil`, `trunc` and `round` return `float`, and Osprey rejects float-to-integer coercion, so a definition that computes an integer through reals cannot be returned as `int`. Such a definition is `NonIntegerResult` unless it is rewritten in integer mode. Publishing a lossy conversion, or an `int` built from a fabricated cast, is exactly what this clause forbids.

**Literal support follows the numerical profile.** Accept every finite decimal or scientific literal supported by the target's verified binary64 conversion, including values requiring 17 significant digits to round-trip. Preserve the original spelling and record the canonical emitted literal, represented bits, and exact/inexact or unavailable-exactness evidence. Overflow, malformed conversion and underflow that would silently turn a nonzero value into zero are explicit `LiteralOutOfRange` faults; subnormal nonzero values require tests. Correct rounding is a named backend assumption until independently established, not a mathematical proof. An exactness label requires real evidence; absence of such evidence is shown as unknown, never guessed.

**Real mode is a finite binary64 approximation profile.** The product discloses this before generation; it does not implement exact ℝ arithmetic. Generated functions reject nonfinite inputs and report domain violations or nonfinite intermediate/output results as `Result` errors. Check intrinsic operator and builtin preconditions at the point of evaluation, including inside branch guards; a numerical fault never becomes a false predicate or a successful NaN. A distinct opt-in profile permitting IEEE NaN/infinity would need its own contract and is outside this fragment. No algebraic reassociation or fast-math substitution is allowed without an explicit semantics change. Backend identity and math-library behavior belong in the receipt; transcendental results are not promised bit-identical across native and WASM targets. Numerical limits must name the target profile; a source-generation budget is not permission to change the mathematics silently.

## What typechecking does not prove [WORKBENCH-CODEGEN-EQUIVALENCE]

Osprey's type checker establishes that the generated module is well typed against the language's rules. It does not establish that the function computes the mathematician's function, that two expressions are equal, that a guard covers the intended set, or that a binary64 result matches a real number. Those are separate claims with separate evidence, and the workbench never merges them into one badge.

The only equivalence this fragment can claim with evidence is **presentation equivalence**: the same term IR produces the rendered mathematics, the shown text and the generated bytes, and re-reading the shown text yields the same IR. A **mathematical equivalence claim** — the generated code denotes the document's function — requires a checked certificate relating the two, and no such certificate exists for arithmetic over reals in this fragment. The `EquivalenceProved` status below therefore exists, is unreachable, and is displayed as "not available" rather than as an empty success.

## Statuses and invalidation [WORKBENCH-CODEGEN-STATUS]

Four statuses, each with its own evidence, each displayed separately. A later status never implies a claim beyond its own.

| Status | Evidence required | Available in fragment 1 |
| --- | --- | --- |
| `Generated` | the emitter produced bytes from the current valid IR | only after valid generation; never for editing, refused or unsupported input |
| `Typechecked` | the real Osprey compiler accepted those exact bytes, with the compiler's identity and the bytes' hash recorded | only where the host can run a compiler |
| `Tested` | the generated module ran and the document's recorded examples agreed, under the document's stated tolerance | only where the host can run a compiler |
| `EquivalenceProved` | a kernel-checked certificate of the stated mathematical relation | **never**; shown as unavailable |

Invalidation is total and immediate. Editing mathematical source, domains, constants, numeric policy or generator settings clears current generated and later statuses before any new reading is accepted. Only a successful reading and generation of the new revision restores `Generated`; invalid input shows no current code. Old output may be inspected only as explicitly stale and cannot be exported as current. A compiler/checker/dependency/backend identity change invalidates the evidence that depends on it. Example-only edits clear `Tested`; retain `Generated` or `Typechecked` only when their exact semantic and source hashes are unchanged. A pending validation is stamped with the full current receipt identity, including revision and bytes hash; a stale completion cannot restore status or overwrite text. Restoring a document never grants a status from a saved label alone.

## Provenance [WORKBENCH-CODEGEN-PROVENANCE]

Every generated file has a receipt, shown beside the source and exported only on request.

| Field | Contents |
| --- | --- |
| Document | SHA-256 of the exact document text, and its revision number |
| Term | SHA-256 of the canonical IR |
| Source | SHA-256 of the generated bytes |
| Generator | this specification's ID and the emitter's version |
| Decisions | mathematical domain, runtime representation, arithmetic policy, branch order, identifier mapping, inlined constants with original spelling |
| Numerics | literal spellings and represented bits, exactness evidence or unknown, exponent lowering, rounding assumptions, backend/math-library identity |
| Domains | the conditions as written, and the `Result` type each function returns |
| Validation | compiler identity, version, the exact command's name, the bytes' hash, and the outcome or the absence of a host capability |
| Clearing | every status cleared since the previous receipt, with the field that changed |

Regeneration produces a new receipt; the previous one is not merged into it. A receipt is never evidence of a mathematical claim, and the source pane says so in one line.

## States the interface must show [WORKBENCH-CODEGEN-STATES]

| State | Presentation | Never imply |
| --- | --- | --- |
| Clean, generated | stable preview, source pane closed, quiet "unvalidated" line | that the function is checked |
| Editing | caret visible, unicode symbols inserted at the caret or over the selection | that a stale status still holds |
| Faulted input | the sentence underlined at the column, the fix in plain words, source pane empty | that a refused sentence is false or that a partial function exists |
| Stale | statuses cleared the instant the text changed, "previous revision not current" if a receipt is shown | that an old check applies |
| Checking | labelled progress with the current revision's ticket | that an earlier result is the current one |
| Typechecked | filled labelled badge from this revision's compiler result only | that the mathematics was proved |
| Tested | separate labelled badge, tolerance and examples named | that testing approves the untested inputs |
| Unavailable capability | control disabled, one sentence naming the host's limit | success, or that the input is wrong |
| Validation refused | the compiler's actual diagnostic, mapped to the document span | that the fault is in the generated scaffolding |

## Typed mathematics, entry and preview [WORKBENCH-CODEGEN-ENTRY]

Entry is keyboard and text first, on every host. The notation table of [WORKBENCH-NOTATION](0051-ProofWorkbench.md#notation-workbench-notation) gains arithmetic rows — `+ - * / ^ · × ÷ √ π ≤ ≥ ≠ …` and abbreviations (`\times`, `\sqrt`, `\cdot`, `\pi`, `\le` …) — under one rule: **the table remains the only place a symbol is spelled**, and `Notation::expand` stays total and idempotent. `π` has the explicit approximate-constant contract in [WORKBENCH-CODEGEN-TERMS]; a palette key must never offer a glyph that the current reader cannot interpret.

Caret insertion, selection replacement and focus return are required shared-editor behaviors, not claims of already completed host support. Multi-byte symbols, combining marks and astral-plane characters must not corrupt column arithmetic; unsupported identifier forms produce a specific `InvalidIdentifier` or `AmbiguousName` fault, never silent character removal.

The preview is typeset from the same IR the emitter consumes. Structure that the preview shows — numerator and denominator, the radical's extent, branch order, exponent binding — is structure the generated code has; the preview cannot show a grouping the code does not compute. A round-trip regression covers it: reading the Unicode text back yields the identical IR, so what a person sees and what a compiler reads cannot silently diverge.

## Workflows [WORKBENCH-CODEGEN-WORKFLOWS]

1. **Mathematics only.** New definition document, sentences typed, preview typeset. Source pane closed the whole time. Export writes the `.ospml` if they ask.
2. **Mathematics, then source.** Open the source pane; read-only, highlighted, line map live; select a source line and the corresponding formula span highlights, and the reverse.
3. **Mathematics, then a check.** A host with a compiler runs the bounded validation; `Typechecked` fills only on this revision; a diagnostic lands on the formula, not the code.
4. **Refusal.** `x² + y² = 1` under `Function y over reals with x` produces no source and no badge; the fix suggests the branch explicitly.
5. **Correction.** Editing cancels pending work and clears current statuses. A successful regeneration returns to `Generated`; invalid input keeps a neutral draft/refusal state with no current output.
6. **Fork.** "Copy as ordinary document" creates an unlinked `.ospml` file document: no receipt, no statuses, no regeneration.

## Illustrative contracts [WORKBENCH-CODEGEN-CONTRACTS]

**Every source shape and code block in this proposal is illustrative.** It is not the output of an implemented emitter, it is not typechecked by the compiler, and it is not a golden. The goldens are written in [plan formula-code-generation](../../plans/formula-code-generation.md) phase 2 and pass the compiler before any of these shapes is claimed to be valid Osprey.

**C1 — real function with a declared domain.**

```
Function fall_time over reals with h.
Suppose h ≥ 0.
Constant g = 9.80665.
fall_time(h) = sqrt(2 * h / g).
```

Assumptions: `h` is binary64; `g` is the decimal as written; the `Suppose` line is the whole declared domain. Output returns `Result<float, Error>` for domain and finite-input/intermediate/output checks. Decisions recorded: divider is the inlined literal `9.80665`, so no zero-divisor check is needed, while finite-result checks still apply; `9.80665` is not exactly representable and the receipt says so; `2` is emitted as `2.0`. Not claimed: that the result is the physical fall time, or that any particular precision is achieved at any `h`.

**C2 — integer function with a chosen overflow policy.**

```
Function square_sum over integers with n.
Arithmetic checked.
square_sum(n) = n^2 + n.
```

Output keeps both multiplications and the addition inside `checked` helpers, so arithmetic failure is data rather than a wrapped value; the propagation is explicit and the function returns `Result<int, Error>`. Not claimed: an overflow-free formula — `n = 3037000500` has `n²` above `2^63−1` and the caller learns it rather than receiving a wrapped number.

**C3 — guarded piecewise, first true branch wins.**

```
Function parking_fee over integers with minutes.
Arithmetic saturating.
parking_fee(minutes) = 0 if minutes ≤ 30.
parking_fee(minutes) = 200 if minutes ≤ 120.
parking_fee(minutes) = 500 otherwise.
```

Output is a nested `match` in the written order; the final `otherwise` is what makes it total, and the type is plain `int` because no operation can fault. The `saturating` policy is recorded even though no body operation needs it.

**C4 — refused: an implicit equation.**

```
Function y over reals with x.
y^2 = 1 - x^2.
```

`ImplicitEquation`. The fix: "write `y(x) = sqrt(1 - x^2)` and suppose `1 - x^2 ≥ 0` if the upper branch is what you mean". No source, no status, source preserved.

**C5 — refused: synthesis this fragment will not fake.**

| Document | Refusal | Why no algorithm exists yet |
| --- | --- | --- |
| `harmonic(n) = sum of 1/k for k from 1 to n.` | `UnboundedSynthesis` | summation binders, their bounds and their numeric effect are unspecified |
| `velocity(t) = d position / d t.` | `UnsupportedDerivative` | differentiation is not an evaluation, and no supported numeric rule exists |
| `half(a, b) = a div b.` | `IntegerDivisionUnspecified` | floor versus truncating division, and the zero divisor, are decisions |
| `total(n) = Σ_{k=1}^{n} k, n ≥ 0, closed form.` | `UnboundedSynthesis` | "closed form" names a search, not an operation |
| `find x such that x^2 > 2.` | `UnboundedSynthesis` | a solution set is a relation, not a function, and no search procedure is specified |

## Refusals are neutral [WORKBENCH-CODEGEN-REFUSALS]

An unsupported input is not an error about the user's mathematics and is never presented as one. A refused document: preserves the text byte for byte, shows a caution or neutral explanation rather than a danger state, produces no source, no preview of a function it did not build, no status, no `true`/`⊤`, no zero, no empty function, and no partial file. The word "wrong" appears only where the compiler itself refused something.

The refusal vocabulary is closed and each code is a typed fault, not a string: `NoFunctionHeading`, `ImplicitEquation`, `RelationNotFunction`, `UnknownInput`, `UnknownName`, `UnknownConstant`, `UnknownFunction`, `NumericModeMissing`, `ArithmeticPolicyMissing`, `IntegerDivisionUnspecified`, `UnguardedDivision`, `GuardedGap`, `ImplicitMultiplication`, `InvalidIdentifier`, `AmbiguousName`, `ReservedName`, `LiteralOutOfRange`, `ExponentNotRepresentable`, `NonIntegerResult`, `UnboundedSynthesis`, `UnsupportedDerivative`. A new refusal is a specification change with its own test.

## One application, mechanical hosts [WORKBENCH-CODEGEN-APP]

Everything above lives in the shared Osprey application over `@osprey/native`, on native, in VS Code and in the browser, from one source tree, under [WORKBENCH-LAYOUT](0051-ProofWorkbench.md#home-and-layout-workbench-layout) and [WORKBENCH-STORE](0051-ProofWorkbench.md#store-workbench-store). Parsing, the term IR, every decision table, generation, the line map, statuses, receipts and invalidation are application code. A host renders regions and completes capabilities; it makes no decision and holds no second generator. There is no TypeScript, Swift, C# or Rust implementation of any rule in this document.

Candidate modules, each under 500 lines with functions under 20, joining the existing layout table:

| Module | Owns |
| --- | --- |
| `src/definition.ospml` | `Definitions::read`, `Function`/`Suppose`/`Constant`/definition sentences, faults, `show` |
| `src/terms.ospml` | `Expr`, `Guard`, `Terms::show`, `Terms::typeset`, canonical form |
| `src/lower.ospml` | the semantic decisions: numeric mode, policy, exponent lowering, guards to totality, `Result` typing |
| `src/generate.ospml` | `Generate::document` and the line map; the only writer of numeric source |
| `src/codestatus.ospml` | statuses, receipts, hashes, invalidation, ticket comparison |

New slices and regions, added the way [WORKBENCH-STORE](0051-ProofWorkbench.md#store-workbench-store) adds them: a `Definitions` slice (derived, host-held) and a `Program` slice holding the IR, the source and the receipt; regions `definition`, `preview`, `source` and `receipt`. A keystroke in one definition sentence refreshes `definition`, `preview` and `source` and nothing else, with the existing region-refresh case extended rather than duplicated.

## Verification [WORKBENCH-CODEGEN-VERIFICATION]

The suite lives in `apps/workbench/tests/`, runs under GC, ARC and wasm32 with no skips, and its own-source coverage must never fall below the gate already registered for `apps/workbench` in `coverage-thresholds.json`. Every case below is a delivery requirement; the matrix is [WORKBENCH-CODEGEN-MATRIX] in [plan formula-code-generation](../../plans/formula-code-generation.md#acceptance-matrix-workbench-codegen-matrix), which also carries the exit evidence per phase.

| Area | What must be proved |
| --- | --- |
| Reading | every sentence row of [WORKBENCH-CODEGEN-DOCUMENT] reads; every refusal code fires on a minimal near-miss; columns are code-point columns |
| Presentation equivalence | `read (show d) = d` over a generated corpus; `typeset`, `show` and `generate` agree on structure for every fixture |
| Determinism | byte-identical goldens, twice per run and once after a reparse from `show` |
| Decisions | each underdetermination row has a case that omits the decision and asserts the named refusal, not a default |
| Numerics | integer bounds, full finite binary64 literal range, subnormals, Unicode identifiers, inexact/unknown-exactness disclosure, nonfinite/domain errors, exact large-exponent parity, division guards, checked/wrapping/saturating lowering, `x^2` versus `pow` |
| Totality | no generated fixture contains `?:`, a bare integer `+ - *`, a redundant annotation, or an empty branch; every `Result` path propagates |
| Statuses | each status fills only with its evidence; every invalidation field clears it; a stale ticket cannot restore a result |
| Adversarial mutation | mutating a guard, constant, operator, mode, policy or branch order changes the bytes and clears statuses; no mutation is a silent no-op |
| Refusals | each refusal preserves the text, produces no source and no status, and never marks the input false |
| Application | the fixture suite drives the real `start`/`dispatch`: entry, preview, source pane, export, states |
| Hosts | the real shared application completes accepted and refused flows in the native app, installed VSIX and browser; each host has its own interaction and screenshot evidence |

Nothing in this feature may disable, skip, weaken or narrow an existing gate: `make ci`, the differential corpus, the `apps/workbench` suite and its coverage gate, the `packages/native` tests, VSIX packaging checks and the release audit all keep their current requirements, and this feature adds cases to them rather than replacing any.

## Performance budgets [WORKBENCH-CODEGEN-PERFORMANCE]

Proposed and measurable, not measured. Each is a benchmark under `benchmarks/` or a bounded assertion, never a story:

| Budget | Target | How it is measured |
| --- | --- | --- |
| Read + lower + generate, 40-sentence document | ≤ 8 ms p95 over 200 iterations on the CI runner | benchmark case with a fixed fixture |
| Preview refresh on one keystroke | ≤ 4 ms p95, and no re-read of unchanged blocks | benchmark, plus the region-refresh fixture |
| Generated source size | ≤ 32 KiB for a 20-function document; slice bound below the capability limit | assertion on every golden |
| Validation round trip | bounded by the `Run` deadline of 15 s, cancelled by ticket, never blocks a keystroke | host test with a slow stand-in |
| Memory | no live ARC objects after a document is replaced | the existing ARC live-object assertion |

The workbench suite has no clock, so timing cases belong to `benchmarks/`; the suite asserts the structural guarantees (no re-read, bounded bytes) that timing would otherwise hide behind.

## Limits and follow-on fragments [WORKBENCH-CODEGEN-LIMITS]

Explicitly **not** in this fragment, and not to be implied by it:

- implicit equations, solving, root selection, parametrisation, inequality systems;
- sums, products, integrals, series, limits and any bindered form with a bound expression;
- derivatives, ODEs, PDEs, numerical integration and any step-size or tolerance choice;
- vectors, matrices, complex numbers, structured values and records;
- integer division, `%`, modular arithmetic and bit operations;
- recursion, iteration, state, collections, effects and I/O in generated code;
- other source languages;
- any claim that the generated function equals the mathematician's function.

Each follow-on fragment is a specification with its own algorithm, its own numeric contract, its own refusals and its own tests. "The generator will figure it out" is not a fragment.

## Related contracts

- [spec 0051](0051-ProofWorkbench.md): the application, its store, regions, notation, mathematics tree and proof emitter.
- [spec 0052](0052-WorkbenchLessons.md): Learn mode, which may later teach this fragment.
- [spec 0049](../language/0049-DependentTypesAndProofs.md) and [spec 0050](../language/0050-TypeDisciplines.md): the proof side, with which this feature shares a window and no verdict.
- [spec 0037](../language/0037-ArithmeticEffects.md): arithmetic totality, `Arith`, the total helpers and the float contract.
- [spec 0012](../runtime/0012-Built-InFunctions.md): the numeric builtins this generator may call.
- [spec 0004](../language/0004-TypeSystem.md): `int`, `float`, inference and the ban on float-to-integer coercion.
- [spec 0023](../language/0023-LanguageFlavors.md): the ML flavor the emitter writes and the marker it carries.
- [spec 0047](0047-OspreyNativeHosts.md): the capability boundary: `Run`, `SaveFile`, slices and tickets.

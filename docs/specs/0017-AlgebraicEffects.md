# Algebraic Effects

Effects let application code ask for work without fixing its implementation.
A handler decides how that work happens. Use them to manage logging, storage,
configuration, retries and other interactions at a clear application boundary.
The compiler tracks the required operations and rejects an application that
has not provided them.

This is the single normative effects contract for both flavors, including
staging and continuations. Delivery status and independent comparisons belong
to [plan 0016](../plans/0016-algebraic-effects-and-handlers.md); the
[runnable guide](../../examples/handlers/README.md) identifies what the current
prototype can execute.

## Manage an effect

Start with an ordinary function that requests an operation. Choose its
implementation where the application runs:

```osprey
effect Console { write: fn(string) -> Unit }
fn report() = perform Console.write("Order accepted")

let terminal = handler Console { write message => print(message) }
terminal(report)

mut captured = ""
let recording = handler Console { write message => { captured = message } }
recording(report)
print(captured)
```

`report` is unchanged. One handler prints; the other records the message for a
test. An operation is a typed request, a handler is its implementation, and
calling the handler runs the work within that implementation's scope. No
continuation terminology or `in`/`do` is needed for this use.

### Callable handler values `[EFFECTS-HANDLER-VALUE]`

Default `handler E { op args => body }` and ML `handler E` with indented arms
produce callable values. `h(work)` / `h work` installs `h` while calling the
zero-argument function `work`. Construction installs nothing. Handler factories,
parameters, return values and ordinary function composition work normally:
`outer(|| => inner(work))` installs `outer` around `inner` in Default.

`[EFFECTS-HANDLER-VALUE-STATE]` A factory invocation captures its own environment.
Repeated calls of the resulting handler share its captured cells; independent
factory calls have independent cells. Each application creates a fresh handler
activation and cleanup lifetime, even when its captured state is shared.

### Handling the rest of a block `[EFFECTS-HANDLE-REST]`

```osprey
fn runReport() = {
    handle Console { write message => print(message) }
    report()
}
```

`handle` governs the following statements and final expression in its containing block. ML uses indented arms. A final handler with no following expression is an error. To select a computation explicitly, construct a callable handler and apply it. Handler forms with `in` or `do` are rejected in both flavors; there are no compatibility aliases.

## Effect declarations

```ebnf
effectDecl ::= [ "static" ] "effect" IDENT [ "<" typeParams ">" ] "{" opDecl* "}"
opDecl ::= [ "control" [ "abort" | "once" | "many" ] ] [ "replayable" ] IDENT ":" fnType
```

ML uses layout and `payload => result` operation types; the semantic fields
are identical. Modifiers are contextual. A modifier followed immediately by
`:` is an operation name, not a modifier. Static operations must be value
operations.

An undecorated **value operation**, on normal completion of its arm, returns a value to the caller, which continues once. Value mode alone does not prove termination or exclude effects requested by that arm. A **control operation** gives the handler its suspended
remainder, called a continuation. Its multiplicity defaults to `once`;
`abort` permits no resumption and `many` permits repeated resumption. Mode is
part of the operation's interface, so substituting a handler cannot change it.
Searching an arm for `resume`, including unreachable code, never selects mode.
`replayable` is independent of mode; see [Replayability](#replayability--multi-replay).

`[EFFECTS-OP-TYPING]` Operations must be declared; performs and arm parameters
must match their positional arity and types. Unknown or duplicate arms are
errors. All arms are validated, including unused ones. Arguments are evaluated
once, left to right. `perform` does not accept named arguments.

### Generic effects

`[EFFECTS-GENERIC-DECL]` Effects may declare type parameters and `in`/`out`
variance. Operation parameter positions are inputs; result positions are
outputs. Each written instantiation has the declared arity and known,
well-formed types.

`[EFFECTS-GENERIC-INSTANTIATION]` `perform Stash<int>.take()` and
`handler Stash<int> { … }` use explicit arguments. Omitting arguments requests
inference. Both spellings work at either stage. Each handler site is
instantiated independently; its arms and handled operations must agree.
`Stash<string>` cannot answer `Stash<int>`.

`[EFFECTS-GENERIC-RUNTIME]` Operation identity includes the declaration,
resolved type arguments, operation and scoped instance. Lowering must preserve
that identity and full payload/result types; a runtime name or erased ABI is
not permission to merge distinct instantiations.

<a id="handlers"></a>

## Scope, composition and instances

A perform selects the innermost **active** matching operation implementation,
including through helper calls. The location where the helper was defined does
not select a handler. Arm variable captures, however, keep their definition's
lexical environment. Nested partial handlers can answer complementary
operations; an uncovered operation searches outward.

Operation arms, return clauses and finalizers execute **outside their own
activation**. Performing the same operation in an arm therefore forwards to an
outer handler; it does not call itself. An absent outer handler is an ordinary
unhandled-effect error. Deep resumption reinstalls the suspended activation and
all intervening scopes for the resumed computation.

```ebnf
instanceExpr ::= "instance" effectRef
handlerTarget ::= effectRef | capabilityName
handlerValue ::= "handler" [ "static" ] handlerTarget "{" clauses "}"
maskExpr ::= "mask" handlerTarget "{" expr "}"
```

Here `capabilityName` resolves to an ordinary bound instance value; effect names resolve to declarations. ML replaces braces with layout. `instance` accepts a declaration/instantiation, not an existing `@capability` row reference. Block-scoped `handle` uses the same target and clauses as `handler`.

`[EFFECTS-INSTANCE]` `let db = instance Store<T>` creates a fresh typed capability
identity, without installing an implementation. `handler db { … }` / `handle db`
installs one; `perform db.get(key)` selects only implementations of that identity.
Unnamed `Store<T>` operations use the default identity for that instantiation.
Handles may be passed and captured; their identity and lifetime remain in types.
A named row entry `Store<T>@db` refers to that capability. A formal capability
parameter binds the corresponding row identity; unrelated instances never unify.
A fresh identity can escape only with its lifetime/requirements preserved, never
as a claim that its future operations have already been handled.

`[EFFECTS-MASK]` Default `mask E { body }` and ML `mask E` with an indented body
skip one enclosing activation for that resolved effect instance while evaluating
`body`. Masking a named capability uses `mask db`. It skips that activation's
arms, preserves other effects and any handlers installed inside `body`, and
requires the skipped outer scope in the row. Nested masks skip successive
activations. It neither removes the requirement nor grants an implementation.

<a id="effectful-function-types"></a>
<a id="effect-row-polymorphism-effects-row-poly"></a>

## Effectful function types `[EFFECTS-ROW-POLY]`

A function type records permitted operations as an effect row. Most application
code can leave it inferred. Public higher-order interfaces can name an unknown
remainder:

```ebnf
effectSet ::= "!" row
row ::= effectRef | rowVar | "[" [ effectRef { "," effectRef } ] [ "|" rowVar ] "]"
effectRef ::= IDENT [ "<" typeList ">" ] [ "@" capabilityName ]
```

`!Console`, `![Console, Store]` and `![]` are closed bounds. Lowercase `!e`
(or `![|e]`) is a row variable; `![Console | e]` adds known requirements to
an open remainder. An omitted annotation requests inference, not purity.
A whole-effect entry permits that declaration's operations; internal inference
retains individual operations for partial discharge.

`[EFFECTS-GENERIC-ROWS]` Explicit row arguments pin instantiation and have the
same arity/type rules as performs. Omitted generic arguments are inferred.

Rows are multisets of resolved scoped operation labels with an optional tail.
Multiplicity here counts **handler scopes**, not executions: two sequential
calls to one capability need one requirement; forwarding or masking can require
another scope. Different labels commute; occurrences of the same label retain
their inner/outer order. Sequencing computes a common admissible row, not a
concatenation of operation executions.

Equality unification matches labels and scope occurrences, solves remaining
flexible tails and performs occurs checks after substitutions. With distinct
labels and distinct flexible tails, `<E|r1> = <F|r2>` may use fresh `t` with
`r1=<F|t>` and `r2=<E|t>`. `<E|r> = <F|r>` has no finite solution; freshening
must not discard the shared-tail constraint. Closed rows must match exactly
under equality; cyclic row substitutions are rejected.

The preceding equality rules solve inference constraints, not annotation admissibility. Annotation checking uses **inclusion**, not equality: inferred requirements
must fit the declared bound; permitted unused operations are legal. Quantified
annotation tails are rigid while checking, so extra requirements cannot be
hidden by assigning them to the caller's variable. Free row variables generalize
at function/let boundaries when absent from the environment, subject to the
same ownership/value restrictions as type variables. Captured mutable state
cannot be generalized into incompatible uses.

`[EFFECTS-STATIC-DISCHARGE]` A handler removes one scoped occurrence of each
operation it supplies, at the matching instantiation and instance. Arm, return
and finalizer requirements remain in the enclosing row. A missing operation
names its effect, operation and relevant handler in the diagnostic. The selected
program entry has an empty closed row. Compiler-provided host operations retain
their declared target capabilities; this rule does not turn builtin I/O into
untracked pure computation.

Effect requirements propagate through calls, generic/higher-order arguments,
returned closures, fields and fibers. Constructing a closure does not perform
its latent operations, and being constructed inside a handler does not discharge
operations invoked after escape. Not calling a callback contributes none of its
latent requirements. Builtin behavior belongs to the resolved binding; shadowing
a builtin name cannot confer builtin effect behavior.

## Handler-owned state

`[EFFECTS-HANDLER-STATE]` Arms may capture mutable cells. Each captured cell has
one identity shared with its originating scope; promotion cannot copy its value
into an independent cell. Assignment requires handler mutation authority under
[Bindings](0003-Syntax.md#bindings), preserved through specialization. Reusable
continuations must satisfy the additional captured-state replay obligations.

<a id="resuming-handlers"></a>

## Resuming and transforming answers

`[EFFECTS-HANDLER-ARMS]` Let the handled body return `A`, the handler answer `B`,
and an operation return `R`. A value arm returns `R`; a control arm returns `B`.
A `return value => expression` clause converts normal body completion from `A`
to `B`; its default is identity, requiring `A=B`. It is not applied to a control
arm's answer. There is at most one return clause and one finalizer.

```osprey
effect Ask { control value: fn() -> int }
let answering = handler Ask {
    value => resume(21)
    return value => "answer=${value}"
}
print(answering(|| => perform Ask.value()))
```

`[EFFECTS-RESUME]` Inside a control arm, `resume(v)` supplies `R` and runs the
captured remainder deeply; it returns that branch's answer `B`. `resume()`
supplies `Unit`. A control arm can run more code after a resume returns.
Returning without resuming or transferring the continuation abandons its
remainder and answers `B`. Value and abort arms have no usable continuation.

`[EFFECTS-CONTINUATION-OWNERSHIP]` Bare `resume` denotes the continuation value in both flavors, including in tail position. It never implicitly calls or drops it. Default `resume()` and ML `resume ()` invoke with `Unit`; ML `resume value` supplies an argument. Returning bare `resume` transfers ownership into the answer and requires a compatible answer type; otherwise it is a type error.
`let k = resume`, passing it to an owning parameter or capturing it in an
escaping closure transfers ownership and removes the former owner's right to
use it. Its type records input `R`, answer `B`, residual row, multiplicity and
captured lifetimes. Latent resume effects and drop/finalizer effects are tracked separately: both explicit and implicit drop must discharge cleanup requirements at that site, or retain the required handler context through owned captures. Escaping cannot silently lose finalizer handlers. A once call consumes it; a many call borrows its reusable
snapshot for one branch. An owning alias can outlive the original arm only
when every captured resource permits that lifetime. Borrowed callbacks cannot
retain it. A scope exit drops untransferred ownership; `drop(k)` explicitly
drops an owned continuation. Transfer delays abandonment and cleanup until the
new owner consumes or drops it. Ordinary copying of a continuation's ownership
is rejected; reusable invocation does not imply unrestricted copying.

`[EFFECTS-RESUME-NESTING]` The continuation includes intervening suspended arm
frames up to its answering handler. Their post-resume work settles from the
innermost live frame outward; crossing a handler boundary can make this differ
from reversing perform-entry order. Preserve this behavior under nested partial
handlers and owned continuation transfer.

### Finalization `[EFFECTS-FINALIZATION]`

`finally => cleanup` in a handler is a `Unit` cleanup clause. It runs exactly
once when that activation's ownership ends, after normal completion, abandoned
work or cancellation. It runs outside its own activation with cleanup's effects
accounted for in the enclosing row. Cleanup cannot resume the dropped
continuation. Cancellation cannot interrupt cleanup; scheduling is specified by
[CANCEL-FINALLY](0036-StructuredConcurrency.md#finalizers--cancel-finally).

On normal body completion, the return clause transforms the answer before the activation finalizer runs. Cleanup returns `Unit` and cannot replace that answer; any explicit outward control transfer during cleanup follows ordinary outer-handler semantics.

A shared delimiting activation remains alive while any owned continuation or
active branch retains it. Each independently owned inner branch scope finalizes
once when that branch finishes or is abandoned; the shared activation finalizes
once after its last owner is released. Capturing many does not implicitly clone
an affine resource or copy a finalizer's authority to release a shared resource.
Nested finalizers execute inner to outer. A finalizer that itself transfers
control must still unwind the remaining outer finalizers; already-run cleanup
is never repeated.

## Runtime transport obligations

`[EFFECTS-OPERATION-MAILBOX]` Transport preserves every operation operand and
answer at any accepted arity, including managed values and complete `Result`
variants. Storage stays alive through its owning continuation and aliases;
release happens exactly once per owned reference, including abandonment.
An absent operand is an invalid-runtime-state error, never a fabricated zero.

`[EFFECTS-FIBER-PERFORM]` Independent performers serialize access to a shared
handler's mutable state and cannot exchange operands or answers. Nested deep
resumption retains logical ownership and must not deadlock on a physical lock.
Concurrent activations that alias one captured cell must share its logical state owner; otherwise the checker rejects concurrent access. Independent per-activation locks are insufficient. Alias/ownership checks survive static discharge.
Cross-fiber ownership and cancellation follow
[Structured Concurrency](0036-StructuredConcurrency.md); serialization alone
cannot prove reusable-continuation safety.

## Stage — [STAGE-AXIS]

`[STAGE-AXIS]` Stage selects whether a handler interpretation is discharged during compilation or dispatched at runtime. A `static effect` requires static interpretation; an unmarked declaration defaults to dynamic interpretation and also permits an explicitly selected static interpretation of its value operations. Operation mode and multiplicity remain declaration contracts independent of that selection. No axis is inferred by searching an arm for `resume`.

`dynamic` is the default. A dynamic handler may be selected at runtime. `static` requires compile-time selection and specialization of the interpretation, eliminating handler dispatch while preserving the interpreted computation. Its operands, captured values and results may still be computed at runtime; static selection does not require constant evaluation.

## Declaring a stage — [STAGE-DECL]

`[STAGE-DECL]` The `static` modifier precedes `effect` in both flavors. The static requirement is retained on every resolved operation reference; selecting a static interpretation for an unmarked effect does not change that operation's identity. Generic instantiation and stage are independent: explicit or inferred type arguments identify the same operation in either stage.

Operation modes are defined once in [Effect declarations](#effect-declarations). Static operations use value mode; dynamic operations may use value or control mode.

## Static handlers — [STAGE-HANDLE-STATIC]

`[STAGE-HANDLE-STATIC]` `handle static E` interprets `E` within its computation; `handler static E { arms }` is its callable form. Application must resolve its interpretation at compilation. Control operations cannot be installed statically. The compiler must know the selected interpretation's code and capture bindings, but need not know their runtime values. Arbitrary runtime selection of a heap handler value does not imply static discharge. Runtime and static interpretations must preserve the same operation contract and captured binding identities.

`[STAGE-STATIC-TOTAL]` A static region MUST cover every operation of the resolved effect instantiation, and all must be value operations. A mixed value/control declaration cannot be installed statically; split the interfaces when separate staging is required. Missing, duplicate and unknown arms are compile errors naming the operation.

`[STAGE-STATIC-TAIL]` A static arm supplies an operation result. Normal return continues the computation once in tail position. It MUST NOT capture, store or explicitly resume a continuation, or abandon the computation. A tail `resume(v)` is written as the value `v`; control handlers belong to the dynamic stage.

`[STAGE-STATIC-MONOTONE]` An arm selected for static interpretation may introduce only operations that are themselves statically discharged, including through helpers and callbacks. A residual runtime operation requiring effect-handler dispatch in an arm is rejected. Source-authorized memory reads/writes may remain, retaining the purity, ownership and replay obligations below. Builtin effects remain obligations without explicit `perform` syntax. Validation applies to unused arms too; an outer dynamic handler does not satisfy this condition.

A static arm may read or mutate captured state under the same source-level handler authority as a dynamic arm. Discharge preserves the shared cell, rather than copying its current value, and retains state-access obligations for residual purity, ownership and replay checks. Rewriting an arm does not authorize mutation elsewhere or make its state replayable.

`[STAGE-STATIC-FINITE]` Discharge MUST terminate. A compiler may enforce documented step and depth bounds and reject a transformation it cannot finish. Exhausting a bound is a source-located compilation error, never stack overflow or partially rewritten output.

## Rows and discharge — [STAGE-ROW]

`[STAGE-ROW]` The row representation and unification rules are [EFFECTS-ROW-POLY](#effectful-function-types-effects-row-poly). Labels retain resolved effect, operation and generic-instantiation identity. The declaration supplies mode, multiplicity and any static requirement; the handler selects a permitted stage. Duplicate scoped labels are retained.

`[STAGE-ROW-DISCHARGE]` A handler removes only the operation occurrences it covers, at the matching instantiation and scope. Effects of arms, return transformations and finalizers remain obligations. A static region MUST be explicitly selected; a simple value implementation is not silently promoted. A statically selected region covers value operations only and a `static effect` cannot be installed dynamically. Requirements propagate through calls, higher-order arguments and fibers.

## Handlers are lowering passes — [STAGE-LOWER]

`[STAGE-LOWER]` Static discharge is a semantics-preserving transformation of a resolved and validated program. It preserves operation input/result types, binding identities, observable order, evaluation count and residual effects. Arguments are evaluated once, left to right, before arm parameter bindings become visible. Arm captures refer to their definition environment; shadowing at a perform site cannot change them. Specializing a helper preserves the same captures and mutable locations as a direct invocation.

`[STAGE-LOWER-ORDER-PHASE]` The required order is:

1. Parse both flavors to canonical AST while preserving declarations, arms and performs.
2. Assemble modules and resolve declarations and lexical identities.
3. Validate all operation contracts, arms, argument/result types, generic instances and transitive effect obligations, including unused arms.
4. Specialize and discharge static regions.
5. Validate transformed types and residual effects, then check target and kernel capabilities before code generation.

Source validation evidence, mutation authority and replay-relevant operation and capture identities survive discharge. Transformed code neither gains authority nor loses valid authority merely because handler syntax was erased. Errors retain original source locations. Editor services inspect the original contracts. An empty row produced by unchecked erasure is not evidence of correctness.

`[STAGE-LOWER-ORDER]` Invocation selects the innermost active enclosing handler instance covering the resolved operation, including when a helper was defined outside that region. Static specialization reproduces this selection; the perform's source location does not select an instance. Arm captures remain lexical to their definition environment. An inner region is discharged before its enclosing region consumes the result. Independent transformations may commute only when doing so preserves observations.

`[STAGE-LOWER-DYNAMIC]` Rewriting inside a dynamic region MUST preserve that region's scope, dispatch and operation order. Specializing a helper does not erase its original declaration if other callers still require it. An uncovered call reports its missing operation, not an invented unknown-identifier error.

## Zero residue — [STAGE-RESIDUE]

`[STAGE-RESIDUE]` No static perform or runtime effect-handler dispatch or installation survives successful discharge. Ordinary residual functions, closures, calls and allocations may remain, including closure environments that preserve captured bindings. They MUST NOT perform runtime effect lookup, handler installation or continuation capture on behalf of the discharged effect. Zero dispatch residue does not promise elimination of every ordinary indirect call or allocation; relative speed and allocation claims require measurements.

## Effects as dialects — [STAGE-DIALECT]

`[STAGE-DIALECT]` Typed static operations form a compiler dialect and their handler interpretations define lowering rules. Coverage proves all source operations have interpretations; validation preserves their contracts; residual checks establish target legality.

`[STAGE-DIALECT-INDEPENDENT]` This model does not require MLIR or another particular compiler framework. An implementation must satisfy the same observable contracts whether it uses AST rewriting, an intermediate representation or another internal form.

`[STAGE-DIALECT-PORTABLE]` Changing the compiler's internal dialect representation MUST NOT change source types, lexical capture, evaluation order or diagnostics' source attribution.

## GPU legality — [STAGE-GPU-LEGAL]

`[STAGE-GPU-LEGAL]` A kernel requires an empty residual dynamic row after validated static discharge, and must satisfy the target's data-type, memory and control-flow restrictions. An empty row alone does not prove an arbitrary program executable on a GPU.

`[STAGE-GPU-KERNEL]` A `kernel` region supplies static interpretations for its device operations. It is checked using the same transformation and residual obligations as other static regions.

`[STAGE-GPU-DIAG]` A rejected region identifies the remaining effect/operation or unsupported target construct at the kernel boundary. An unresolved higher-order requirement is not treated as pure.

## WebAssembly — [STAGE-WASM]

`[STAGE-WASM]` Static discharge requires no runtime continuation and is portable when the resulting ordinary program is supported. Dynamic value handlers likewise need no captured continuation. Neither fact proves support for dynamic control or cleanup.

### Multiplicity on wasm32 — [MULTI-WASM]

`[MULTI-WASM]` Continuation semantics are independent of the target. A backend without a required capability MUST reject it before linking, naming the effect, operation and capability. Host-native pthread support does not imply support in a native mobile library.

| Operation shape | Backend obligation |
|---|---|
| Static value | Validated discharge and supported residual code |
| Dynamic value | Value dispatch without capturing a continuation |
| Control `abort` | Non-local transfer plus complete cleanup |
| Control `once` | Owned affine continuation |
| Control `many` | Reusable continuation with independent branches |

A one-shot platform stack-switch primitive does not prohibit multi-shot implementation through CPS or another portable representation. There is no permanent source-language ban on `many` for wasm32.

## Reactive signals — [STAGE-SIGNALS]

`[STAGE-SIGNALS]` A signal read may be represented by a static effect such as `Signal<T>.read`. Its interpretation supplies the value; dependency reporting retains the resolved operation identity before discharge.

`[STAGE-SIGNALS-DIRTY]` A computation's dependency set contains the signal operations it may reach. It is a sound static may-read set, not necessarily the values read on one execution. Declared effect annotations are allowed-effect bounds; an allowed but unused signal is not a type error.

`[STAGE-SIGNALS-EXACT]` Dependency reporting MUST state its precision:

- Resolved generic instances are distinct; `Signal<Count>` and `Signal<Cursor>` do not merge. Reusing one payload type does not create two independently identified signals.
- Resolved lexical names, helper calls and higher-order rows determine dependencies. Source spelling alone is insufficient.
- Runtime selection conservatively includes every possible target and reports that widening.
- An unresolved open row is an unknown dependency remainder, never an empty set.
- Invalid or unresolved source returns diagnostics and MUST NOT publish a complete-looking empty dependency set.

`[STAGE-SIGNALS-REBUILD]` A reactive consumer invalidates computations whose may-read sets contain a changed signal. Conservative widening may cause extra work; missing a possible dependency is a correctness defect. Dependency analysis alone does not implement scheduling, change propagation or a UI runtime.

## Per-region backends — [STAGE-BACKEND]

`[STAGE-BACKEND]` Each region may select an interpretation suitable for its target. The same operation contract governs host and device interpretations. Backend selection cannot bypass type, row or stage checks, and a host implementation is not proof of execution on device hardware.

## Stage polymorphism — [STAGE-POLY]

`[STAGE-POLY]` A higher-order definition can be instantiated with callbacks requiring different declared effects and stages. No duplication of its source definition is required.

`[STAGE-POLY-ERASURE]` Validate both instantiations before rewriting. Static instances lose only discharged requirements; dynamic instances retain theirs. Compiler specialization may produce distinct internal definitions.

`[STAGE-POLY-PREREQ]` Independently published higher-order contracts express unknown effects through open rows. A closed-program example that works after specialization does not prove general row polymorphism.

`[STAGE-POLY-PARAMETRIC]` An unmarked value-effect contract may have both dynamic and explicitly selected static interpretations. The callback's operation requirements retain identity through higher-order calls; selection changes discharge, not the signature. A `static effect` remains a requirement for static interpretation. No implicit stage parameter or promotion is introduced.

## Multiplicity — [MULTI-AXIS]

`[MULTI-AXIS]` A control operation declares how often its captured continuation may be resumed: `abort` means zero, `once` means at most one, and `many` permits repeated use. Multiplicity is an upper bound, not a promise to resume. In particular, `once` is affine, not linear.

A value arm returns the operation result and implicitly continues on normal return. A control arm returns the handler answer and may decline to resume. These meanings do not depend on whether `resume` text occurs in the arm. Dropping a continuation is legal only with correct ownership release and finalization.

`[MULTI-AXIS-STATIC]` Static value operations have no captured continuation and admit no multiplicity annotation. Their normal-return behavior is fixed by [STAGE-STATIC-TAIL].

## Declaring multiplicity — [MULTI-DECL]

`[MULTI-DECL]` The [operation grammar](#effect-declarations) permits multiplicity only on control operations. Independently, `replayable` may decorate either a value or control operation: the promise concerns repeating that operation when some enclosing continuation is replayed, not whether this operation captures a continuation itself. Control operations default to `once`. A value operation cannot claim `abort` or `many`; its normal return supplies one answer.

`[MULTI-DECL-ABORT-RESULT]` An `abort` operation never returns to its perform site. Its written result type describes an unreachable result; use of that result does not make the operation resumable. The operation's arguments and the handler's answer type remain checked.

## Handler obligations — [MULTI-HANDLE]

`[MULTI-HANDLE]` The checker enforces each continuation's permitted uses, accounting for branches, recursion, aliases and higher-order callbacks. Absence of loop syntax does not imply absence of repetition. Runtime consumed-continuation checks remain defense against invalid execution, not permission to accept a statically invalid use.

`[MULTI-HANDLE-ABORT]` An `abort` arm MUST NOT resume. Its answer completes the handler region after required cleanup.

`[MULTI-HANDLE-ABORT-MODE]` The operation declaration determines its arm mode as specified in [Effect declarations](#effect-declarations). Unreachable branches cannot change an arm from value interpretation to continuation control.

`[MULTI-HANDLE-ONCE]` Each `once` continuation is an affine owned value. Moving it transfers the right to resume; copying that right or consuming it twice is rejected. One resume on each alternative control-flow branch is permitted. Abandonment releases its captured resources.

`[MULTI-HANDLE-MANY]` Each resume of a `many` continuation starts from the same captured remainder and returns that branch's handler answer. It does not rerun work performed before capture. The runtime must preserve the captured environment and handler scope independently for each branch.

`[MULTI-HANDLE-MANY-LEXICAL]` Resumptions may be passed to callbacks or stored in an explicitly owned continuation value whose lifetime keeps the required frames and handler environment alive. Escape is governed by ownership and resource lifetime, not a blanket ban based on lambda syntax. Borrowed resumptions cannot escape their owner. The continuation-value surface is [EFFECTS-CONTINUATION-OWNERSHIP]; finalization is [EFFECTS-FINALIZATION].

## Replayability — [MULTI-REPLAY]

`[MULTI-REPLAY]` `replayable` promises that repeated execution with the same arguments and handler context is acceptable. It is a semantic promise about the operation, not a conclusion from its name or body size.

`[MULTI-REPLAY-CHECK]` A reusable continuation may be invoked more than once only when its replayed computation and reachable captures permit it. Non-replayable external actions such as charging a card or sending a message cannot be duplicated implicitly. A diagnostic names the responsible operation or captured resource. Operations of the handled effect are not exempt merely because they share an effect name.

`[MULTI-REPLAY-COARSE]` A compiler may use a conservative whole-region row when it cannot isolate the continuation suffix, and must explain that approximation. It MUST NOT accept an unsafe replay because an operation was erased or hidden behind a helper. Moving non-replayable work outside the captured region is a valid way to make the intended boundary explicit.

`[MULTI-REPLAY-STATE]` Branches MUST NOT implicitly share mutable cells that were assumed isolated. Immutable state may be threaded through results. A handler implementing a `replayable` operation must establish the promised semantics from its operations and captures. Immutable captured values and explicitly threaded immutable branch state are the baseline; a mutable or external resource requires a checked replay-capable abstraction with specified share/snapshot behavior. An annotation alone cannot turn an arbitrary captured cell into such a resource. Static origin alone does not prove arbitrary captured state replayable. Discharge preserves this evidence even when state access becomes ordinary residual code and the original operation no longer appears in its row.

`[MULTI-REPLAY-FIBER]` A reusable continuation MUST NOT duplicate a live fiber's execution, wait-queue membership or affine resources. Cross-fiber use requires an ownership-transfer protocol; implementations lacking it reject that use with the originating perform site. Serializing an arm is not proof that its continuation is safely cloneable.

## Cost model — [MULTI-COST]

`[MULTI-COST]` Static interpretation eliminates handler dispatch; value interpretation needs no captured continuation; abort needs transfer and cleanup; once needs an affine continuation; many needs reusable branch state. CPS, persistent frames and safe stack segments are permitted implementation choices. One-shot limits uses of a captured continuation, not the number of runtime switches. No representation implies a universal performance advantage.

`[MULTI-COST-ABORT]` Dropping a continuation runs the finalizers of abandoned regions in nesting order and releases every owned operand. The rule applies to early exit, cancellation and abort. A successful resume transfers ownership appropriately; it must not make finalizers run twice. [EFFECTS-FINALIZATION] owns lifetime and ordering; [CANCEL-FINALLY](0036-StructuredConcurrency.md#finalizers--cancel-finally) owns cancellation shielding.

## Effect trace — [MULTI-TRACE]

`[MULTI-TRACE]` A trace records the resolved perform site, actual runtime handler instance and resumption branch. The handler implementation may be selected dynamically. Reusable continuations produce a branching logical trace; physical stack frames alone need not describe it. Static-dispatched operations have no runtime trace entry. Debugger and language-server views preserve source attribution and distinguish possible static handlers from the actual runtime selection.

## Relation to stage — [MULTI-STAGE]

`[MULTI-STAGE]` Stage and multiplicity are distinct. A dynamic value interpretation meeting static coverage and purity obligations may receive a conversion suggestion, but the compiler MUST NOT silently change its stage or interpretation selection.

`[MULTI-STAGE-POLY]` A row-polymorphic helper carries the instantiated operation contracts, including multiplicity, without duplicating the helper's definition. Dynamic multiplicity survives validation; it is not erased to make a program type-check.

`[MULTI-STAGE-TURN]` A reusable continuation retains logical handler ownership across its branches. Runtime serialization must permit nested resumption and cannot hold a non-reentrant physical lock while executing a branch that calls the same handler. Structured-concurrency rules govern ownership transfer and scheduling; cloneability is checked separately.

## Compatibility — [STAGE-COMPAT]

`[STAGE-COMPAT]` Omitting `static` selects dynamic staging. It does not weaken operation typing or select arm mode. Programs relying on accidental name capture or unchecked erased contracts are rejected or corrected with source-located diagnostics.

`[MULTI-COMPAT]` Legacy mode inference from `resume` occurrence is removed. Migration makes the intended mode explicit and preserves valid behavior. A historical wrong-answer golden is replaced only with a documented contract and an independent reproducer. Multiplicity defaults cannot silently reinterpret an arm.

## Falsification gates — [STAGE-FALSIFY]

`[STAGE-FALSIFY]` Acceptance requires both-flavor tests for static/dynamic observational equivalence, shadowed captures and helpers, once-only ordered argument evaluation, wrong used/unused arm contracts, generic instances, cross-module resolution, nested regions, kernel residual effects, dependency-report precision and absence of static dispatch in IR. Shared higher-order code must work with both static and dynamic callbacks.

`[MULTI-FALSIFY]` Acceptance requires an operation-level retry that does not repeat a later external action; rejection of unsafe replay; a pure search producing all alternatives; the same higher-order combinator serving affine and reusable operations; stored-continuation lifetime checks; independent branch state; finalization under resume, abandonment and cancellation; and target-specific capability tests. Compiler success or clean IR alone is insufficient: observable output and ownership must agree with the contracts.

## References — [STAGE-RESEARCH]

- [Koka handbook: effect handlers](https://koka-lang.github.io/koka/doc/book.html#sec-handlers): operation modes, handler abstraction, masking, reusable resumptions and finalization.
- [OCaml Effect.Deep](https://ocaml.org/manual/5.3/api/Effect.Deep.html): typed handlers and affine continuations.
- [Effekt handlers](https://effekt-lang.org/docs/concepts/effect-handlers): scoped capability interpretation.
- [MLIR dialect conversion](https://mlir.llvm.org/docs/DialectConversion/): typed lowering and residual legality.
- [WebAssembly stack switching](https://github.com/WebAssembly/stack-switching): platform continuation support, distinct from compiler transformations.

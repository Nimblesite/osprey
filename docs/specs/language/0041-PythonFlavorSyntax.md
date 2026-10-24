# Python Flavor Syntax

**Status: partially implemented frontend; GPT-2 generation matches the Default generator byte for byte on the host, Metal and OpenCL; the complete flavor and the deferred training port remain delivery targets.** [Plan 0031](../../plans/0031-python-flavor.md) owns delivery and the [GPT-2 port sources](../../plans/python-flavor-port/README.md). This specification adds a surface to [Language Flavors](0023-LanguageFlavors.md), not a Python execution environment.

## One language — [FLAVOR-PYTHON-BOUNDARY]

The concrete syntax is Python-shaped; the canonical AST is Osprey. The frontend parses a Python CST, validates the supported surface, and lowers directly to `osprey_ast::Program`. Python spelling disappears before project assembly, Hindley–Milner inference, effect checking, static specialization, memory management, and LLVM generation. These phases must not inspect the source flavor.

There is no Python interpreter, object model, standard library, import execution, implicit exception handling, GIL, or Python package dependency. Existing Osprey types, arithmetic policies, closures, fibers, handlers, and target restrictions retain their meaning. Python-looking source is not a promise of Python behavioral compatibility. Unsupported constructs are diagnosed before shared checking; they never fall through to CPython.

Equivalent programs in Default, ML, and Python must have equal canonical ASTs after removing source locations and normalizing generated names. Equivalent flat-call programs must also emit identical LLVM IR under the same options. AST equality does not compare ML curried functions with flat Python functions.

## Source selection — [FLAVOR-PYTHON-SELECT]

The extension is **`.py`**, the CLI spelling is `--flavor python`, and the source marker is `# osprey: flavor=python`. Default and ML extensions remain unchanged. `.py` selects Python when a file is explicitly handed to the Osprey compiler. Osprey never executes a `.py` file to discover its flavor.

The shared selection precedence remains explicit override, leading flavor marker, extension, then Default. Python markers use `#`; the existing flavors retain their marker spelling. Only leading comments/blank lines, including a shebang and encoding cookie, may precede a marker. A marker/extension conflict is an error unless an explicit override resolves it. Selecting another flavor changes the parser, not the bytes of a file.

Project discovery adds an optional `[project].python_sources` list of literal paths relative to the manifest. Paths must resolve inside a declared source root. Only these Python files and a `.py` entry explicitly named by the manifest are discovered; ordinary `.py` helpers beneath a source root are excluded. Listed paths must exist, be unique after normalization, and end in `.py`. The entry is not added twice. Each selected file resolves its own flavor, so a project may import `.osp`, `.ospml`, and `.py` together. A conflicting project-wide flavor override must report an error rather than silently parse a mixed project under one frontend.

Editors must not globally register `*.py` as Osprey. An explicit Osprey project selection, leading marker, or user language-mode choice opts that document into an `osprey-python` language mode and Osprey LSP. Unlisted scripts such as `examples/gpt2/reference.py`, build tools, virtual environments, and vendored Python remain Python documents. No replacement extension is required to solve editor ownership.

## Concrete syntax — [FLAVOR-PYTHON-CST]

Use a pinned Python tree-sitter grammar with a separate `python/` validator and lowerer. The CST preserves indentation, punctuation, comments, decorators, string spelling, and source ranges. No textual translation into Default or ML, external parser process, or execution of source is permitted. The initial accepted grammar includes Python's generic parameter brackets, `type` aliases, `match`/`case`, comprehensions, and f-strings. A syntactically valid CST node can still be an unsupported Osprey surface form.

Indentation opens suites; tabs in indentation and inconsistent dedents are errors. Parentheses/brackets/braces permit implicit line continuation. Four spaces are the formatter's standard. `#` starts a comment. A leading triple-quoted string on a file, class, or function is documentation, lowered into Osprey's documentation model. A plain string-literal statement written directly after a module constant assignment or a `type X = ...` alias documents that declaration — the PEP 257 attribute-docstring convention — lowered into its `doc`; the string statement itself is documentation, not an executed value. Other strings are values. Python escape decoding and f-string segmentation belong to the frontend; interpolation expressions use the same Python frontend and original source locations.

## Bindings and functions — [FLAVOR-PYTHON-FUNCTIONS]

```python
def scale(value, factor):
    return value * factor

answer = scale(value=6, factor=7)
twice = lambda value: scale(value, 2)
```

`def` becomes a flat `Stmt::Function`; `lambda` becomes `Expr::Lambda`. Parameters/results are inferred unless a written constraint is necessary. Keyword arguments retain names and lower to canonical named arguments; as in every flavor, one argument list is fully positional or fully named ([function calls](0005-FunctionCalls.md)). There is no implicit receiver, currying, default argument, variadic argument, argument unpacking, or decorator execution. Explicit type application uses `f[int](value)` and lowers to the existing type-application node.

`@extern` marks a signature-only `def` with an exact `...` body and complete parameter/result types. It lowers to the existing `Stmt::Extern`, with the same C ABI and safety boundary; it does not import a Python extension module. Legal combinations such as `@export` with one declaration-kind decorator are order-independent metadata. Duplicate or conflicting declaration-kind decorators are errors.

An initial `name = value` or `name: T = value` declares a lexical binding. The frontend identifies later assignments in that lexical scope before lowering: a reassigned binding becomes the existing `Stmt::Let { mutable: true }`, and later writes become `Stmt::Assignment`; a binding with no writes remains immutable. `name: Final[T] = value` explicitly forbids reassignment, with `Final` imported from `typing` under PEP 591. No custom `mut` annotation is required or accepted. Field/index assignment and augmented assignment are rejected initially. Parameters cannot be reassigned; inner lexical scopes retain Osprey scope rules.

`return value` is allowed only at the tail of a function suite or of every branch of a tail `if`/`match`. Such suites lower to block values; `return` has no runtime representation. Bare tail `return` and an empty body written `pass` produce Osprey `Unit`. An early return, fallthrough from only some branches, or a return from a loop body is an error. A non-tail `if`/`match` must produce Unit in all branches. `None` in an annotation or value position spells Osprey `Unit`, so `-> None`, bare `return`, and `return None` have the same unit result. It introduces no nullable object or Python object identity.

## Typed Python notation — [FLAVOR-PYTHON-TYPING]

The surface follows typed Python annotation syntax: [PEP 484](https://peps.python.org/pep-0484/) function annotations and `Callable`, [PEP 526](https://peps.python.org/pep-0526/) variable/field annotations, [PEP 585](https://peps.python.org/pep-0585/) built-in collection generics, [PEP 591](https://peps.python.org/pep-0591/) `Final`, and [PEP 695](https://peps.python.org/pep-0695/) type parameter and alias spelling. Four-space suites and source presentation follow [PEP 8](https://peps.python.org/pep-0008/). Annotations keep their standard punctuation and names; an Osprey-specific substitute for standard typed Python notation is not acceptable.

`from typing import Callable, List, Dict, Any, Final` and `from collections.abc import Callable` are compile-time annotation imports, including aliases. They never load a Python module or create an Osprey project import edge. `list[T]` and `dict[str, T]` are the preferred collection spellings; imported `List[T]` and `Dict[str, T]` lower to the same canonical types. `Callable[[A, B], R]` supplies an exact flat function signature. Bare/ellipsis Callable signatures cannot erase Osprey's arity contract. `Any` denotes Osprey's existing `any`, including its mandatory pattern matching.

Annotations may be omitted in exactly the situations accepted by Osprey's shared Hindley–Milner checker, including parameters, return values, local bindings, lambdas and inferred polymorphic functions. Omission never introduces implicit `Any`, an unchecked function body or a Python-specific inference fallback. When the shared checker cannot infer a type required by the program, compilation fails; an explicit annotation must supply the missing information. A valid inferred polymorphic type is not itself an inference failure. Explicit annotations constrain that same inference and contradictory uses fail normally.

This is a specified subset of typed Python annotation notation over Osprey semantics, not blanket conformance to every typing PEP. Class inheritance, protocols/interfaces, dynamic type construction, Python nullability, unchecked gradual casts, and variadic/unconstrained callable types are not added by accepting annotations. Unsupported typing forms must name the missing representation in a diagnostic. Implementation evidence for each supported form belongs in plan 0031.

## Expressions and iteration — [FLAVOR-PYTHON-EXPRESSIONS]

| Surface | Canonical meaning |
| --- | --- |
| `True`, `False` | Osprey booleans |
| `str`, `list[T]`, `dict[str, T]` in types | `string`, `List<T>`, `Map<string, T>` |
| `Callable[[A, B], R]` imported from `typing` or `collections.abc` | Flat Osprey function type |
| `a and b`, `a or b`, `not a` | Boolean-only Osprey `&&`, `||`, `!`, with short circuiting |
| `a if condition else b` | Shared boolean `Expr::Match` |
| `if`/`elif`/`else` suites | Nested shared boolean matches |
| `match value: case ...:` | Osprey exhaustive pattern match; no `__match_args__` |
| `f"{value}"` | Osprey interpolation; no format specifiers, conversions, or debug `=` initially |
| `len(value)` | Existing constrained `length` builtin |
| `range(stop)`, `range(start, stop)` | `range(0, stop)`, `range(start, stop)` |
| `[f(x) for x in items if p(x)]` | Existing iterator map/filter/toList pipeline |
| `update(record, field=value)` | Immutable `Expr::Update` |

Builtin aliases are syntax recognized only when not shadowed by a lexical/imported declaration. `perform`, `resume`, `update`, `unwrap_or`, `spawn`, `await_fiber`, `yield_fiber`, `send`, and `recv` are reserved frontend intrinsics and cannot be rebound. Their arguments are AST syntax, never Python calls executed by the compiler. Decoration forms listed below are reserved in decorator position; other decorators are rejected.

Integers are Osprey signed 64-bit values; floats are binary64. Arithmetic faults require the same `Arith` policy as other flavors. `/`, `//`, and `%` lower to Osprey `/`, `intDiv`, and remainder, including Osprey's negative-operand behavior. There is no arbitrary precision, truthiness, operand-returning `and`/`or`, Python floor-division promise, or float32 substitution. Chained comparisons and `is` are rejected initially; write explicit comparisons with `and`. Existing builtin names are available without an import.

Ordinary `items[index]` has Osprey's existing lookup/result semantics, not Python `IndexError` or negative indexing. Slices, tuple values/destructuring, dictionary iteration, generator expressions, `sum`, `max`, and `zip` are not implicitly invented. Use existing Osprey operations and explicit Result matching. `unwrap_or(result, fallback)` is the spelling of Default `result ?: fallback`: it lowers to the same exhaustive Result match, so the fallback is an arm evaluated only on `Error`, and `unwrap_or(gpuGet(buffer, index), 0.0)` keeps its device gather form inside a kernel. Any other arity or a keyword argument is rejected. List comprehensions accept the existing range/map/filter iterator surface, not arbitrary Python iterables; nested clauses lower to nested folds with source evaluation order preserved. Comprehensions produce host lists, never implicit GPU kernels.

`for name in iterator:` lowers to `forEach(iterator, callback)` over the same iterator surface. The iteration variable is local to that callback, not leaked into the enclosing scope. `while`, loop `else`, `break`, and `continue` are rejected; state accumulation uses `fold` or recursion. These forms introduce no new loop node or generator runtime.

## Closed classes — [FLAVOR-PYTHON-CLASS]

```python
class Price[B]:
    value: B

    def discount(amount: B) -> B: ...

quote = Price(value=100, discount=lambda amount: amount)
```

A plain `class` declares one closed Osprey record `Stmt::Type`. Annotated attributes are immutable fields. A `def` whose body is exactly `...` declares a function-valued field; it is not an interface method. Its complete parameter/result types are required. `Price` above has exactly two stored fields, both supplied at construction. Calling `quote.discount(5)` calls its stored function; no implicit `self` is inserted. Ordinary field-call resolution remains shared with other flavors.

Construction uses `Price(value=..., discount=...)`, lowering to `Expr::TypeConstructor`; explicit generic construction uses `Price[int](...)`. Declaration field order determines the same layout as a Default record twin. Missing, extra, duplicate, or wrongly typed fields are shared-checker errors. Defaults, method implementations in plain classes, inheritance/base expressions, metaclasses, `__init__`, descriptors, `super`, dunder hooks, class variables, monkey patching, protocols, and interface declarations are rejected. Record operations are ordinary free functions or explicit function-valued fields. Existing module signatures are a separate Osprey capability, not class inheritance or a Python interface mechanism.

`type Name = ExistingType` declares an Osprey alias. `@union` on a class declares an Osprey sum type: each immediately nested, undecorated record class supplies one variant's fields. It is the only plain-data context permitting nested classes. Patterns use `case Variant(field=binding):`; constructor values use `Variant(field=value)`. A union is not a Python union of runtime class objects.

## Effect declarations — [FLAVOR-PYTHON-EFFECT]

```python
from typing import Callable

@static_effect
class Tensor[B]:
    def build(size: int, kernel: Callable[[int], float]) -> B: ...
    def get(buffer: B, index: int) -> float: ...
    def matMul(a: B, b: B, rows: int, inner: int, cols: int) -> B: ...

def product(a, b, rows, inner, cols):
    return perform(Tensor.matMul(a, b, rows, inner, cols))
```

`@effect` and `@static_effect` make the class an Osprey dynamic/static `Stmt::Effect`, not a record. Operation signatures have complete types and an exact `...` body; no receiver is added. Empty field lists and generic binders retain their existing meaning. `perform(E.op(args))` lowers directly to `Expr::Perform`: its argument is an operation reference with argument syntax, not an ordinary function call evaluated first.

Operations are value-mode by default. `@control` on an operation declares control mode; `@once`, `@many`, and `@replayable` record the corresponding existing multiplicity/replay metadata. Invalid combinations, static control, unsupported resumption multiplicities, and target limitations receive the same shared diagnostics as the other flavors. These spellings grant no additional continuation capability.

## Handlers — [FLAVOR-PYTHON-HANDLER]

```python
@effect
class Log:
    def write(message: str) -> Unit: ...

@handler(Log)
class Console:
    def write(message):
        print(message)

def work():
    return perform(Log.write("Model loaded"))

Console(work)
```

`@handler(E)` and `@static_handler(E[T])` declare named handler functions over a zero-argument computation: `Console(work)` runs `work` under the handler. Their class-shaped suites contain operation implementations, not instance methods. The decorator lowers the definition to the function the Default module idiom spells as `fn Console(work) = { handle Log { … }\n work() }`, with the same lexical handler node as a handler value's body, so a module may export a handler where a module `let` must be a compile-time constant; declaring it performs no work. The class name follows function naming rules and cannot redefine a builtin. There are no bases or inherited/default arms. Handler arm parameters/results are inferred against the declared effect. Required arms, duplicate arms, arity, type arguments, captures, and handler stages follow [Algebraic Effects](0017-AlgebraicEffects.md).

`handler_value(work)` has the existing zero-argument callback contract. `with handler_value:` is syntax for applying that value to a generated zero-argument callback containing the suite. The suite is a lexical scope. A terminal `with` suite may supply its surrounding function's tail result through that callback; an ordinary statement-position suite must return Unit and cannot early-return from its enclosing function. `as`, multiple context managers, arbitrary context-manager expressions, and Python `__enter__`/`__exit__` are rejected. No resource-management behavior is inferred.

`resume(value)` lowers to the existing resume expression and is permitted only in a control arm. Arms execute outside their own activation; forwarding to an outer handler, deep single-shot resumption, captures, and cleanup follow the shared contract. Static escaping callables/iterators remain subject to the shared specialization checks. A syntax flavor must not remove such a diagnostic or broaden capabilities.

`@requires(Log, Arith)` on a function spells a closed effect annotation, including `@requires()` for an explicitly empty requirement. `@requires(Log, tail="e")` spells the existing open remainder. Names and concrete type arguments lower to the existing effect-reference fields; inference remains the default. There is no new Python-specific effect inference.

## Modules and concurrency — [FLAVOR-PYTHON-INTEROP]

Leading `# osprey: namespace=gpt2` and `# osprey: module=PythonModel` directives contribute a file to a logical namespace/module. They lower to the same namespace/module declarations as Default and ML. The module directive requires a namespace directive. Directive values are identifier paths; malformed, duplicate, or late directives are errors. They do not derive identity from folders.

`from gpt2 import Backends`, `from gpt2.Backends import Tensor`, and `import gpt2.Model as Model` lower to logical Osprey imports, including aliases. `@export` marks a declaration public; unmarked declarations remain module-private. Exported constants use `__all__ = ["WIDTH", "HEADS"]`, a compile-time list of literal names, not an executed assignment. `@export` and `__all__` are equivalent visibility markers; duplicate names are errors. A listed name must be a declaration in the same file/module contribution. Wildcard/relative imports, `sys.path`, importing arbitrary Python modules, and import-time execution are rejected. `__name__` and the `if __name__ == "__main__"` idiom are replaced by Osprey's `main` entry selection.

`spawn(work)`, `await_fiber(fiber)`, `yield_fiber()`, `send(...)`, and `recv(...)` lower to existing fiber expressions with their existing arity, isolation, serialization, and target restrictions. They are not Python coroutines. `async def`, `await`, Python `yield`, shared-memory threads, `try`/`except`/`finally`, and `raise` are rejected. Expected failure uses Osprey `Result` and exhaustive matching. Reactive signals import the existing primitives unchanged.

## GPU and GPT-2 — [FLAVOR-PYTHON-GPU]

Python flavor calls the existing GPU builtins and static `Tensor` effect; it adds no automatic placement or NumPy/PyTorch compatibility. The handler decides where arithmetic runs. The model body is identical for CPU lists and GPU buffers. Kernel purity, scalar captures, binary64 arithmetic, explicit host/device transfers, device selection, and backend errors follow [GPU Computation](../runtime/0034-GPUComputation.md). A selected unavailable device must fail visibly; host fallback cannot count as a successful hardware test.

The reference remains the independent, executable [`examples/gpt2/reference.py`](../../../examples/gpt2/reference.py). Do not rewrite it, reclassify it as Osprey, or make it depend on the port. Port the reference's forward helpers and greedy sampler into `.py` flavor sources, retaining names, dimensions, formulas, causal masking, accumulation order, tie-breaking, and tokenization behavior where Osprey permits. Replace mutation with records/folds and tensor comprehensions with explicit `Tensor.build`; explain those concrete edits beside the port. The port is self-contained Python-flavor source: the `Tensor` static effect, its list and GPU static handlers, the arithmetic policy, the tokenizer, the checkpoint loader, the constants and the records are `.py` modules, and the generation project reaches no `.osp` source through an import or a source root. The flavor's algebraic effects carry the whole design: `@static_effect` declares the dialect, `@static_handler` classes answer it, `@effect`/`@handler`/`with` express the fault policy.

The Python generator prints exactly what the Default generator prints for the same stdin, byte for byte: banner, prompt and token lines, generated count, decoded text and the error and unknown-backend messages. It therefore ports the Default example's KV-cached step as well as the full pass; a port that only recomputed the full pass would still have to match. Training must subsequently port the reference's backward functions and SGD update, with the existing Osprey training example as an additional comparison. A wrapper around the existing generator/trainer does not establish a model port.

## Acceptance — [FLAVOR-PYTHON-VERIFICATION]

Each implemented section needs a source reference and named regression. Frontend tests assert CST retention, exact lowering, diagnostics, and positions. Differential triples share a Default golden and run on native default/GC/ARC with zero ARC leaks, WebAssembly, and supported mobile targets. Mixed projects test all three flavors' imports, records, unions, callbacks, fibers, static/dynamic handlers, and reactive signal rebuilds. Target rejections remain explicit.

Editor acceptance includes completion, hover, diagnostics, source identity, formatting, rename, debugger locations, and leaving ordinary Python scripts alone. CLI test/docs/format/project discovery and CI change classification must recognize only opted-in Python-flavor sources. Existing required gates and coverage thresholds remain intact.

GPT-2 acceptance compares tiny deterministic tensors first, then the published 124M checkpoint. CPU-host results with matched arithmetic are byte-exact; hardware GPU results follow the existing GPU precision contract. Compare prompt IDs, generated IDs/text, stopping/tie behavior, every gradient tensor's established checksum, initial/post-update losses, and process status. Require a strict device run with adapter identity and dispatch evidence separately from host execution. A successful CST parse, CPU run, or GPU-looking banner alone cannot establish the flavor or hardware path as shipped. Performance claims require recorded hardware, commands, warmup/loading policy, repeated timings, and a comparable CPU baseline; no superiority claim over another language follows from this specification.

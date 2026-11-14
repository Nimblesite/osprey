# TypeScript Flavor Syntax

**Status: planned; nothing is implemented.** [Plan 0061](../../plans/0061-typescript-flavor.md) owns delivery. This specification adds a surface to [Language Flavors](0023-LanguageFlavors.md), not a JavaScript runtime. Its first package port is Express, so that HTTP servers written in the TypeScript flavor run natively on Osprey.

## One language, two checkers — [FLAVOR-TS-BOUNDARY]

The concrete syntax is TypeScript 7; the canonical AST is Osprey. The frontend parses a TypeScript syntax tree, validates the supported surface and lowers directly to `osprey_ast::Program`. TypeScript spelling disappears before project assembly, Hindley–Milner inference, effect checking, static specialization, memory management and LLVM generation. These phases must not inspect the source flavor ([FLAVOR-BOUNDARY]).

There is no JavaScript engine, prototype chain, garbage-collected object graph shared with JavaScript, event loop, `Promise`, Node.js API, npm dependency or JavaScript standard library in a compiled program. Existing Osprey types, arithmetic policies, closures, fibers, handlers and target restrictions retain their meaning. TypeScript-looking source is not a promise of JavaScript runtime behavior except where this specification states one.

Equivalent programs in Default, ML, Python and TypeScript have equal canonical ASTs after removing source locations and normalizing generated names. Equivalent flat-call programs emit identical LLVM IR under the same options.

## TypeScript 7 compliance — [FLAVOR-TS-COMPLIANCE]

Every program the TypeScript flavor accepts is a valid TypeScript 7 program. Compliance has two parts:

- **[FLAVOR-TS-COMPLIANCE-SYNTAX]** The frontend accepts only source that the TypeScript 7 parser accepts, and it reports a syntax error wherever TypeScript 7 does. A syntax-valid file can still be an unsupported Osprey surface form, which is a separate diagnostic.
- **[FLAVOR-TS-COMPLIANCE-TYPES]** Every accepted program also type-checks under the TypeScript 7 compiler, `tsc`, configured by the generated `tsconfig.json` against the Osprey ambient library ([FLAVOR-TS-LIB]). An Osprey-accepted program that `tsc` rejects is a frontend defect.

The converse does not hold. Osprey rejects valid TypeScript 7 that violates Osprey's type checking, effect checking or the supported surface. Examples are `any`, `null`, mutation of a field, a non-exhaustive `switch` and mixing `int` with `number`. Rejection is a compile error at the original TypeScript position, never a fallback to JavaScript semantics.

The compiler does not run `tsc` and needs no Node.js. CI enforces [FLAVOR-TS-COMPLIANCE-TYPES] by running the pinned TypeScript 7 compiler over every TypeScript-flavor source in the repository, together with the frontend's acceptance corpus. A user project can run the same check with its generated `tsconfig.json`.

## Source selection — [FLAVOR-TS-SELECT]

The extension is **`.ts`**, the CLI spelling is `--flavor typescript` and the source marker is `// osprey: flavor=typescript`. The shared selection precedence is unchanged: explicit override, leading flavor marker, extension, then Default. A marker/extension conflict is an error unless an explicit override resolves it. `.tsx`, `.mts`, `.cts` and `.d.ts` files are not TypeScript-flavor sources.

As with Python ([FLAVOR-PYTHON-SELECT]), `.ts` is a common extension, so project discovery is opt-in. An optional `[project].typescript_sources` list names the TypeScript-flavor files, as literal paths relative to the manifest inside a declared source root. Only listed files and a `.ts` entry explicitly named by the manifest are discovered. Listed paths must exist, be unique after normalization and end in `.ts`. Each file resolves its own flavor, so a project may import `.osp`, `.ospml`, `.py` and `.ts` together.

Editors must not register every `*.ts` file as Osprey. A listed project source, a leading marker or a user language-mode choice opts a document into the Osprey language server. The editor's own TypeScript service keeps serving the file as TypeScript, using the generated `tsconfig.json`, so both checkers report on the same text.

## Concrete syntax — [FLAVOR-TS-CST]

The compiler parses in process with a pinned Rust TypeScript parser that produces a TypeScript 7 syntax tree with comments and exact source ranges. No external process, no textual translation into another flavor and no execution of source is permitted.

The selected parser is `oxc_parser`. Plan 0061 Phase A must confirm it against the TypeScript 7 parser conformance corpus before the frontend depends on it. A parser lifted from a Rust port of the TypeScript 7 compiler may replace it only if it is a published, pinned crate that passes the same corpus at least as well. A tree-sitter TypeScript grammar is not the compiler parser: its error-tolerant grammar is not version-exact and cannot establish [FLAVOR-TS-COMPLIANCE-SYNTAX].

A leading `/** … */` comment on a file or declaration is documentation, lowered into Osprey's documentation model. Template literal segmentation and escape decoding belong to the frontend; interpolation expressions use the same frontend and original source locations.

## The Osprey ambient library — [FLAVOR-TS-LIB]

A TypeScript-flavor project's generated `tsconfig.json` sets `"noLib": true`, `"strict": true` and `"noImplicitAny": false`, and includes the Osprey ambient library, a set of `.d.ts` declarations shipped with the compiler. That library declares exactly what Osprey provides: the builtin types (`int`, `float32` and the other sized numeric types as aliases of `number`, `Result`, `List`, `Map`), the builtin functions, and the intrinsics this specification defines (`Success`, `Error`, `perform`, `handle`, `handler`, `resume`, `spawn` and the rest). The JavaScript standard library is absent, so a use of `Date`, `Promise`, `JSON` or a DOM API fails under `tsc` as well as under Osprey.

`noImplicitAny` is off because Osprey infers unannotated parameters with Hindley–Milner. Omitting an annotation never introduces `any` on the Osprey side: when the shared checker cannot infer a required type, compilation fails and an explicit annotation must supply it. An annotation that inference already determines is redundant and is left off, as in every flavor.

## Bindings and functions — [FLAVOR-TS-FUNCTIONS]

```ts
// osprey: flavor=typescript
function scale(value, factor) {
  return value * factor;
}

const twice = (value) => scale(value, 2.0);
```

A `function` declaration becomes a flat `Stmt::Function`; an arrow function becomes `Expr::Lambda`. A nested `function` declaration is rejected; bind an arrow function instead. Generic parameters `<T>` lower to type parameters, and an explicit type argument `f<int>(x)` lowers to the existing type-application node. Default parameters, optional parameters, rest parameters, overloads, `this`, getters, setters and `arguments` are rejected.

`const` declares an immutable binding. `let` declares a binding; the frontend finds later assignments in the same lexical scope, so a reassigned `let` becomes `Stmt::Let { mutable: true }` and each write becomes `Stmt::Assignment`. A `let` that is never reassigned is immutable. `var`, destructuring assignment, compound assignment (`+=`), increment and decrement, and assignment to a field or index are rejected.

`return` is allowed only in tail position: at the end of a function body or at the end of every branch of a tail `if` or `switch`. An early return is rejected. A function that ends without returning a value, or that returns `void`, produces Osprey `Unit`. An expression statement whose value is not `Unit` is rejected: values are immutable, so a discarded result is a lost update.

## Types — [FLAVOR-TS-TYPES]

| TypeScript | Osprey |
| --- | --- |
| `number` | `float` (binary64) |
| `int`, `int32`, `float32` and the other sized aliases from [FLAVOR-TS-LIB] | the matching [sized numeric type](0004-TypeSystem.md#sized-numeric-types--type-numeric-sized) |
| `string`, `boolean`, `void` | `string`, `bool`, `Unit` |
| `readonly T[]`, `T[]`, `List<T>` | `List<T>` |
| `Map<string, T>` from [FLAVOR-TS-LIB] | `Map<string, T>` |
| `(a: A, b: B) => R` | flat function type |
| `interface` or object `type` with `readonly` fields | closed record `Stmt::Type` |
| union of object types discriminated by `kind` | sum type ([FLAVOR-TS-UNIONS]) |
| `unknown` | Osprey `any`, with its mandatory pattern matching |

`any`, `null`, `undefined`, optional properties (`x?: T`), index signatures, intersection types, string or numeric literal union types outside a `kind` discriminant, mapped and conditional types, `keyof`, `typeof` in type positions, enums, namespaces, declaration merging and `declare` are rejected. TypeScript's structural typing does not apply on the Osprey side: records are nominal, and two interfaces with the same fields are different types.

A method signature in an interface (`discount(amount: B): B;`) declares a function-valued field, not a method; calling `quote.discount(5)` calls the stored function with no `this`. `class` declarations are rejected.

## Record and union values — [FLAVOR-TS-UNIONS]

The frontend lowers before type inference, so it must find a record's type name in the syntax. An object literal therefore needs a syntactic context: an annotated binding (`const p: Point = { x: 1, y: 2 };`), the annotated return type of the enclosing function, or a `satisfies` expression (`move({ x: 1, y: 2 } satisfies Point)`). An object literal with no such context is rejected with a diagnostic that suggests `satisfies`. Lifting this restriction needs a flavor-neutral contextual record type in the shared checker, which plan 0061 tracks separately.

A union of object types whose members each declare a `readonly kind` property with a distinct string-literal type declares an Osprey sum type. Each member is one variant. Its name is the PascalCase form of its `kind` literal, so `{ kind: "circle"; r: number }` is variant `Circle`. The `kind` property is the tag and is not stored as a field. Variant names must be distinct after case conversion. A union discriminated by any other property, or with a member lacking `kind`, is rejected. An object literal with a `kind` property constructs the variant its literal names.

A `switch (value.kind)` over such a union lowers to an exhaustive Osprey `match`. Each `case` binds the narrowed value's fields by name, and a missing case is a compile error even where TypeScript would allow fallthrough to the end. Case fallthrough, `default` beside complete cases and `break` are rejected.

`Result<T, E>` in the ambient library is the union `{ readonly kind: "success"; readonly value: T } | { readonly kind: "error"; readonly message: E }`, so it lowers to Osprey's own `Result`. `Success(value)` and `Error(message)` construct its variants.

## Expressions and iteration — [FLAVOR-TS-EXPRESSIONS]

| Surface | Canonical meaning |
| --- | --- |
| `===`, `!==` | Osprey `==`, `!=` |
| `&&`, `\|\|`, `!` | boolean-only `&&`, `\|\|`, `!` with short circuiting |
| `c ? a : b`, `if`/`else if`/`else` | shared boolean `Expr::Match` |
| `` `total ${n}` `` | Osprey interpolation |
| `/` | binary64 division; both operands must be `number` |
| `intDiv(a, b)` | integer division |
| `%` | Osprey remainder; the result's sign follows the dividend, as in JavaScript |
| `xs.length` on a list | the existing `length` builtin |
| `for (const x of iterator) { … }` | `forEach(iterator, callback)` |
| `{ ...record, field: value }` | immutable `Expr::Update` |
| `xs.map(f)`, `xs.filter(p)`, `xs.reduce(f, init)` | the existing iterator map, filter and fold |

`==`, `!=`, truthiness (a non-boolean operand of `&&`, `||`, `!` or a condition), `??`, `?.`, `typeof`, `instanceof`, `in`, `delete`, `void`, the comma operator, `new`, tagged templates, regular-expression literals, `while`, `do`, `for (;;)`, `for … in`, `break`, `continue`, labels, `throw`, `try`, `async`, `await`, generators and `yield` are rejected. Expected failure is a `Result`, matched exhaustively.

Integers do not follow JavaScript `number` semantics. An `int` is a 64-bit integer whose overflow goes through the `Arith` policy, not a binary64 value that loses precision above 2^53. A program that mixes `int` and `number` without an explicit conversion is rejected by Osprey even though `tsc` accepts it. `number` arithmetic is IEEE binary64, as in JavaScript.

A string's `.length` in TypeScript counts UTF-16 code units, and Osprey strings are UTF-8. `.length` on a string lowers to a UTF-16 code-unit count; until the runtime provides one, it is rejected. It never silently returns a byte count.

## Effects and handlers — [FLAVOR-TS-EFFECTS]

```ts
// osprey: flavor=typescript
interface Log extends Effect {
  write(message: string): void;
}

const Console = handler<Log>({
  write: (message) => print(message),
});

function work() {
  return perform<Log>().write("Model loaded");
}

Console(work);
```

An interface that extends `Effect` or `StaticEffect` from the ambient library declares an Osprey dynamic or static `Stmt::Effect`; each method signature is an operation with complete parameter and result types. An operation whose result type is wrapped in `Control<T>` is a control operation, and the `Once<T>`, `Many<T>` and `Replayable<T>` wrappers record the existing multiplicity and replay metadata. These wrappers are type aliases of `T` for `tsc` and are read syntactically by the frontend.

`perform<E>().op(args)` lowers to `Expr::Perform` of operation `E.op`; it is recognized syntactically and is not an ordinary call. `handler<E>({ … })` declares a callable handler value whose arms are the object's properties, and `handle<E>({ … }, () => body)` installs a lexical handler around `body`. `resume(value)` is permitted only in a control arm. Required arms, arity, captures, stages and diagnostics follow [Algebraic Effects](0017-AlgebraicEffects.md). The spelling grants no capability the other flavors lack.

A closed effect annotation is the return-type wrapper `Requires<T, [Log, Arith]>`; inference remains the default.

## Modules and concurrency — [FLAVOR-TS-INTEROP]

ES module syntax carries Osprey modules. A leading `// osprey: namespace=express` and `// osprey: module=Router` contribute the file to a logical namespace and module, as in the Python flavor. `import { Router } from "./router";` imports a module of a listed project source; `import express from "express";` imports the namespace of a dependency package. `export` marks a declaration public, and unmarked declarations stay module-private. Import resolution never executes code. Side-effect imports, `import *`, dynamic `import()`, `require`, `export default` of an expression and re-exports are rejected, except the default import form a port declares for its package's entry function ([TS-PORT-EXPRESS-SURFACE]).

`spawn(work)`, `awaitFiber(fiber)`, `yieldFiber()`, `send(…)` and `recv(…)` lower to the existing fiber expressions with their existing isolation and target restrictions. They are not promises. A program's entry is Osprey's `main` selection.

## First port: Express — [TS-PORT-EXPRESS]

The first TypeScript-flavor package port is Express, so that HTTP servers written in the TypeScript flavor run natively on Osprey. It follows the port rules of [Python-Flavor Package Ports](../ecosystem/0043-PythonFlavorPorts.md) with TypeScript in place of Python: [PORT-SCOPE], [PORT-TEST-ORACLE], [PORT-UPSTREAM-TEST-PARITY], [PORT-SUITE] and [PKG-COVERAGE-GATE] apply unchanged.

**[TS-PORT-EXPRESS-LAYOUT]** The package lives in `src/osprey/packages/express/`. Its `osprey.toml` lists `typescript_sources`, and its `src/*.ts` files use `// osprey: namespace=express`. The README names the pinned Express version it is compared with, maps every ported function to the original and lists each divergence with its reason. The oracle is ordinary JavaScript under `oracle/` (not `.ts`, which would be a TypeScript-flavor source) and runs the pinned real Express on Node.js.

**[TS-PORT-EXPRESS-SURFACE]** The port keeps Express's names and argument order where the flavor can spell them: `express()`, `app.get`, `app.post`, `app.put`, `app.delete`, `app.use`, `express.Router()`, `req.params`, `req.query`, `req.get`, `res.status`, `res.set`, `res.json`, `res.send` and `app.listen`. Values are immutable, so each registration returns the extended application. An application is built by chaining, which is also valid Express: `express().use(logger).get("/users/:id", show).listen(3000)`.

**[TS-PORT-EXPRESS-HANDLERS]** A handler returns the response it sends: `(req, res) => res.status(404).json(body)`. The same arrow is valid against Express, where `res.status()` and `res.json()` return the response and the handler's result is ignored. Statement sequences that mutate `res` are rejected by [FLAVOR-TS-FUNCTIONS]. Middleware returns `next()`, which yields the downstream response. Error middleware receives a typed error `Result` rather than a thrown exception. JSON request bodies decode through an explicit typed decoder into a record and produce `Result`, never `any`. Each such divergence from Express is listed in the README.

**[TS-PORT-EXPRESS-SERVE]** `app.listen(port, callback)` starts the native listener through the existing HTTP server ([HTTP-SERVER](../runtime/0014-HTTP.md)) and returns a `Result` holding the server handle or the creation or listen error. Request and response bytes, framing and client disconnects follow the HTTP server contract.

**[TS-PORT-EXPRESS-ORACLE]** Acceptance compares the port with the pinned real Express over real loopback HTTP: the same request corpus is sent to both servers, and status, security-relevant headers, content type, byte length and body bytes must match. The corpus covers routing and path parameters, repeated and empty query values, method rejection, middleware order, mounted routers, error handlers, JSON bodies (valid, malformed and wrongly typed) and binary bodies. The suite runs under native GC and ARC during source-coverage collection. A response that differs from Express only by a documented divergence is asserted as that divergence, never skipped.

## Acceptance — [FLAVOR-TS-VERIFICATION]

Each implemented section needs a source reference and a named regression. Frontend tests assert syntax conformance, exact lowering, diagnostics and positions. Differential programs in all four flavors share a Default golden and run on native GC and ARC with zero ARC leaks, WebAssembly and the supported mobile targets. Mixed projects import TypeScript-flavor modules from the other flavors and the reverse. The pinned `tsc` accepts every TypeScript-flavor source and every accepted fixture, and every rejection fixture names the Osprey rule it violates.

Editor acceptance includes completion, hover, diagnostics, rename and debugger locations through the Osprey language server, beside the editor's TypeScript service and without claiming ordinary TypeScript files. CLI test discovery, documentation, project discovery and CI change classification recognize only opted-in TypeScript-flavor sources. Existing required gates and coverage thresholds remain intact.

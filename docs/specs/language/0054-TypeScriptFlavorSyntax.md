# TypeScript Flavor Syntax

**Status: partly implemented.** [Plan 0061](../../plans/0061-typescript-flavor.md) owns delivery and records what is done. Implemented today: source selection, the `oxc_parser` frontend, functions and arrow functions, interfaces, `kind`-discriminated unions, object literals in a typed context, `switch` on `kind`, the program entry, receiver statements, ES imports and exports, documentation comments, and the first slice of the Express port, which serves the Express README example unchanged. Not implemented yet: effects and handlers other than `Requires` on a field, fibers, reassigned `let`, and editor support. Each section below says which of its rules are not implemented.

This specification adds a surface to [Language Flavors](0023-LanguageFlavors.md), not a JavaScript runtime. Its first package port is Express, so that HTTP servers written in the TypeScript flavor run natively on Osprey.

## One language, two checkers — [FLAVOR-TS-BOUNDARY]

The concrete syntax is TypeScript 7; the canonical AST is Osprey. The frontend parses a TypeScript syntax tree, validates the supported surface and lowers directly to `osprey_ast::Program`. TypeScript spelling disappears before project assembly, Hindley–Milner inference, effect checking, static specialization, memory management and LLVM generation. These phases must not inspect the source flavor ([FLAVOR-BOUNDARY]).

There is no JavaScript engine, prototype chain, garbage-collected object graph shared with JavaScript, event loop, `Promise`, Node.js API, npm dependency or JavaScript standard library in a compiled program. Existing Osprey types, arithmetic policies, closures, fibers, handlers and target restrictions retain their meaning. TypeScript-looking source is not a promise of JavaScript runtime behavior except where this specification states one.

Equivalent programs in Default, ML, Python and TypeScript have equal canonical ASTs after removing source locations and normalizing generated names. Equivalent flat-call programs emit identical LLVM IR under the same options.

## TypeScript 7 syntax — [FLAVOR-TS-COMPLIANCE]

The surface is TypeScript 7 syntax, and the Osprey compiler is the only tool in the chain. It parses the source itself ([FLAVOR-TS-CST]), lowers the syntax tree to the canonical AST and compiles that AST. Building or running a TypeScript-flavor program needs no TypeScript compiler, no Node.js and no `tsconfig.json`.

- **[FLAVOR-TS-COMPLIANCE-SYNTAX]** The frontend accepts only source that the TypeScript 7 parser accepts, and it reports a syntax error wherever TypeScript 7 does. A syntax-valid file can still be an unsupported Osprey surface form, which is a separate diagnostic.

Osprey rejects valid TypeScript that violates Osprey's type checking, effect checking or the supported surface. Examples are `null`, mutation of a field, a non-exhaustive `switch` and mixing `int` with `number`. Rejection is a compile error at the original TypeScript position, never a fallback to JavaScript semantics.

## Optional `tsc` lint — [FLAVOR-TS-LINT]

Because the surface is TypeScript, a project may also check its TypeScript-flavor sources with `tsc`, as it may run any other linter. This is optional. The compiler never runs it, never reads its configuration and does not depend on it; a program compiles and runs exactly the same whether or not the lint has ever run. Its only use is to confirm that a source is also valid TypeScript, so editors, formatters and readers that know TypeScript see the same program.

The repository's lint is `make typescript-lint`. It runs the pinned `tsc` with `"noLib": true`, `"strict": true` and `"noImplicitAny": false` against [`scripts/typescript/osprey.d.ts`](../../../scripts/typescript/osprey.d.ts), which declares what an Osprey program has in scope: `int` and the sized numeric types as aliases of `number`, `Result`, `List`, `Map`, `HttpResponse`, `console.log` and the intrinsics this specification defines. For each builtin a source calls, the lint adds a declaration translated from the compiler's own `osprey --hover` signature. The JavaScript standard library is absent, so `Date`, `Promise` values or `JSON` fail the lint as they fail in Osprey. A ported package supplies `types/<name>.d.ts` for the imports a program makes from it. The lint fails on a program it must reject, so it cannot pass vacuously.

`noImplicitAny` is off because Osprey infers unannotated parameters with Hindley–Milner. The TypeScript flavor has Osprey's rules, not TypeScript's: omitting an annotation never introduces `any`. When nothing in the program gives the checker evidence for a required type, compilation fails and an explicit annotation must supply it; an annotation that inference already determines is reported as redundant, as in every flavor.

## Source selection — [FLAVOR-TS-SELECT]

The extension is **`.ts`**, the CLI spelling is `--flavor typescript` and the source marker is `// osprey: flavor=typescript`. The shared selection precedence is unchanged: explicit override, leading flavor marker, extension, then Default. A marker/extension conflict is an error unless an explicit override resolves it. `.tsx`, `.mts`, `.cts` and `.d.ts` files are not TypeScript-flavor sources.

As with Python ([FLAVOR-PYTHON-SELECT]), `.ts` is a common extension, so project discovery is opt-in. An optional `[project].typescript_sources` list names the TypeScript-flavor files, as literal paths relative to the manifest inside a declared source root. Only listed files and a `.ts` entry explicitly named by the manifest are discovered. Listed paths must exist, be unique after normalization and end in `.ts`. Each file resolves its own flavor, so a project may import `.osp`, `.ospml`, `.py` and `.ts` together.

Editors must not register every `*.ts` file as Osprey. A listed project source, a leading marker or a user language-mode choice opts a document into the Osprey language server. The editor's own TypeScript service may keep serving the file as TypeScript; that is the same optional check as [FLAVOR-TS-LINT].

## Concrete syntax — [FLAVOR-TS-CST]

The compiler parses in process with a pinned Rust TypeScript parser that produces a TypeScript 7 syntax tree with comments and exact source ranges. No external process, no textual translation into another flavor and no execution of source is permitted.

The selected parser is `oxc_parser`, pinned at 0.158.0. `make typescript-conformance` checks it against TypeScript's own parser and compiler conformance corpus, at the commit `tsc` 7.0.2 pins (`4d4f005c`), beside `tsrs_parser` 7.1.0-beta.tsrs1, the published Rust port of the TypeScript 7 parser. A file counts as rejected by `tsc` when it reports a TS1xxx syntax or grammar error. Run on 2026-11-15:

| Suite | Files | `oxc_parser` agrees with `tsc` | `tsrs_parser` agrees with `tsc` | `oxc` accepts, `tsc` rejects | `oxc` rejects, `tsc` accepts | Frontend panics |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| parser | 788 | 80.6% | 99.9% | 1 | 152 | 0 |
| compiler | 6,408 | 96.8% | 99.9% | 5 | 197 | 0 |

Seven compiler cases are UTF-16 or Latin-1, not UTF-8, and are not compared; Osprey sources are UTF-8. The plan's rule was to adopt `oxc_parser` unless a published port parser agreed strictly more often, and `tsrs_parser` does. The decision on 2026-11-15 is to keep `oxc_parser`, for three reasons:

- **Its extra rejections are TypeScript 7 errors too.** They are ECMAScript early errors, such as `await` used as an identifier in an async generator or a misplaced numeric separator, which `tsc` reports from its checker under each case's own options rather than from its parser.
- **The direction that matters is nearly closed.** `oxc_parser` accepts only six files that `tsc` rejects. The frontend rejects the four legacy octal and leading-zero numbers among them itself; a `0123` would otherwise have read as 123 where JavaScript reads 83. The other two are a malformed `/// <reference>` comment and a JSDoc-only type argument.
- **`tsrs_parser` is a beta with a Go-shaped API** whose parsed source text is leaked and never freed, which a long-running language server cannot accept.

The decision is open to review and is revisited when `tsrs_parser` publishes a stable release. A tree-sitter TypeScript grammar is not the compiler parser: its error-tolerant grammar is not version-exact and cannot establish [FLAVOR-TS-COMPLIANCE-SYNTAX].

Before the parser runs, a lexical scan bounds the source's nesting at the shared `MAX_SYNTAX_DEPTH` (512): each open bracket or template hole is a level, and so is each operator or prefix keyword since the last `,` or `;` inside it, because the parser descends once for each. A deeper source is rejected with `syntax nesting limit exceeded` at the offending byte, as every other flavor rejects it, and the parser, the lowerer and the phases after them never recurse past the limit. Positions are 1-based lines and 0-based byte columns, as in every flavor; the language server converts a column to the client's encoding, UTF-16 by default, at its edge ([LSP-ENCODING](../tooling/0020-LanguageServerAndEditors.md)).

A leading `/** … */` comment on a file or declaration is documentation, lowered into Osprey's documentation model. A comment written directly before a function, type or binding documents that declaration. The first such comment before the file's first statement, when no declaration claims it, documents the file, or the module when the file declares one. Template literal segmentation and escape decoding belong to the frontend; interpolation expressions use the same frontend and original source locations.

## Bindings and functions — [FLAVOR-TS-FUNCTIONS]

```ts
// osprey: flavor=typescript
function scale(value, factor) {
  return value * factor;
}

const twice = (value) => scale(value, 2.0);
```

A `function` declaration becomes a flat `Stmt::Function`; an arrow function becomes `Expr::Lambda`. A nested `function` declaration is rejected; bind an arrow function instead. Generic parameters `<T>` lower to type parameters, and an explicit type argument `f<int>(x)` lowers to the existing type-application node. Default parameters, optional parameters, rest parameters, overloads, `this`, getters, setters and `arguments` are rejected.

`const` declares an immutable binding, and assigning to it is rejected. `let` declares a binding that an assignment statement may rebind, and so may a parameter. The statement `x = value;` lowers to a new binding of `x`, exactly as a receiver statement does ([FLAVOR-TS-RECEIVER]), and a compound assignment `x += value;` lowers to a new binding of `x + value`; `-=`, `*=`, `/=` and `%=` follow the same rule. Later statements read the new value. Rebinding equals in-place update only while the variable has one name, so the frontend rejects, at the statement: an assignment inside a branch whose result the following code would not see; an assignment after a nested function has read the variable; and an assignment to a variable of an enclosing function. An assignment used as an expression, `var`, destructuring assignment, increment and decrement, the logical and bitwise compound operators, and assignment to a field or index are rejected; build a new value with spread instead.

`return` is allowed only in tail position: at the end of a function body or at the end of every branch of a tail `if` or `switch`. A guard clause is in tail position: an `if` with no `else` whose block ends in `return` takes the statements after it as its `else`, so `if (bad) { return a; } return b;` is one conditional. Any other early return is rejected. A function that ends without returning a value, or that returns `void`, produces Osprey `Unit`. An expression statement whose value is not `Unit` is rejected by the shared checker: values are immutable, so a discarded result is a lost update. A receiver statement ([FLAVOR-TS-RECEIVER]) is the one statement whose value is kept.

## Program entry — [FLAVOR-TS-ENTRY]

A TypeScript file runs its top-level statements in order. Osprey's entry is `main`, and a top-level executable statement cannot sit beside it ([MODULES-ENTRYPOINT](0025-ModulesAndNamespaces.md#entry-point-modules-entrypoint)). The frontend therefore gathers a file's top-level statements, in source order, into a generated `main`.

Imports, functions, interfaces and type aliases stay file-scope declarations wherever they are written. A `const` or `let` stays a file-scope binding when it comes before the first executable statement and no receiver statement updates it; from the first executable statement on, each binding is a local of `main`. A file that declares its own `main` and also has top-level statements gets the ordinary entry conflict.

## Receiver statements — [FLAVOR-TS-RECEIVER]

JavaScript libraries are written against values updated in place: `app.get("/", handler);` adds a route to `app`, and `res.status(404);` changes `res`. Osprey values are immutable, so the same call returns the extended value and leaves the original alone. A statement-level method chain bridges the two.

A **receiver statement** is an expression statement that is a method call, or a chain of method calls, starting from a local binding or a parameter: `x.m(a);` or `x.m(a).n(b);`. The frontend lowers it to a rebinding of `x` to the value of the whole chain, so every later statement sees the update. A chain that starts from anything else, such as an import or `console`, is an ordinary expression statement.

```ts
const app = express();
app.get("/", home);       // app is now the application with the route
app.listen(3000);         // nothing reads this result, so it binds no name
```

A function that updates one of its parameters this way and ends without returning a value returns that parameter's final value, and a bare `return;` returns it too. This is what makes `(req, res) => { res.send("ok"); }` a handler that returns its response.

Rebinding equals in-place update only while the value has one name. The frontend rejects the cases where the two would differ, each at the statement that causes it:

- an update inside a branch whose result the following code would not see, unless the branch is in tail position and updates the function's returned parameter;
- an update to a local after a nested function has read it, because that function keeps the earlier value;
- a function that updates more than one parameter.

A rebinding that nothing reads afterwards binds no name, so the shared checker applies its own rule for a discarded value to it; a discarded `Result` is rejected there.

## Types — [FLAVOR-TS-TYPES]

| TypeScript | Osprey |
| --- | --- |
| `number` | `float` (binary64) |
| `int`, `int32`, `float32` and the other sized aliases from [FLAVOR-TS-LINT] | the matching [sized numeric type](0004-TypeSystem.md#sized-numeric-types--type-numeric-sized) |
| `string`, `boolean`, `void` | `string`, `bool`, `Unit` |
| `readonly T[]`, `T[]`, `List<T>` | `List<T>` |
| `Map<string, T>` from [FLAVOR-TS-LINT] | `Map<string, T>` |
| `(a: A, b: B) => R` | flat function type |
| `interface` or object `type` with `readonly` fields | closed record `Stmt::Type` |
| union of object types discriminated by `kind` | sum type ([FLAVOR-TS-UNIONS]) |
| `any`, `unknown` | Osprey `any`, which must be matched to a concrete type before use |

`null`, `undefined`, optional properties (`x?: T`), index signatures, intersection types, string or numeric literal union types outside a `kind` discriminant, mapped and conditional types, `keyof`, `typeof` in type positions, enums, namespaces, declaration merging and `declare` are rejected. TypeScript's structural typing does not apply on the Osprey side: records are nominal, and two interfaces with the same fields are different types.

A method signature in an interface (`discount(amount: B): B;`) declares a function-valued field, not a method; calling `quote.discount(5)` calls the stored function with no `this`. `class` declarations are rejected.

## Record and union values — [FLAVOR-TS-UNIONS]

An object literal constructs the record its syntax names: an annotated binding (`const p: Point = { x: 1, y: 2 };`), the annotated return type of the enclosing function, or a `satisfies` expression (`move({ x: 1, y: 2 } satisfies Point)`). Any other object literal is Default's anonymous record, and the shared checker gives it the record type of the declared context it flows into ([TYPE-RECORD-CONTEXT](0004-TypeSystem.md)): `move({ x: 1, y: 2 })` constructs a `Point` when `move` declares `p: Point`, and a nested literal takes the type its field declares. `satisfies` stays available, and is needed where no type is declared.

A union of object types whose members each declare a `readonly kind` property with a distinct string-literal type declares an Osprey sum type. Each member is one variant. Its name is the PascalCase form of its `kind` literal, so `{ kind: "circle"; r: number }` is variant `Circle` and `"not-found"` is `NotFound`. The `kind` property is the tag and is not stored as a field. Variant names must be distinct after case conversion. A union discriminated by any other property, or with a member lacking `kind`, is rejected. An object literal with a `kind` property constructs the variant its literal names and needs no other context; one with no other property is the variant's bare constructor.

A `switch (value.kind)` over such a union lowers to an exhaustive Osprey `match`. The value switched on is a binding. Each `case` binds the fields its body reads as `value.field`, under the field's own name. When the case also reads a parameter, local or function of that name, the field is bound under a fresh name that occurs nowhere in the file instead, so `value.field` never reads the other binding. A missing case is a compile error even where TypeScript would allow fallthrough to the end. A case in tail position ends with `return`; any other case ends with `break`, which the lowering drops. Adjacent `case` labels with no statements between them are one or-pattern ([PATTERN-OR](0007-PatternMatching.md#or-patterns--pattern-or), not implemented yet): every joined variant must declare each field the body reads, at one type. Fallthrough out of a case that has statements and `default` are rejected.

`Result<T, E>` is the union `{ readonly kind: "success"; readonly value: T } | { readonly kind: "error"; readonly message: E }`, so it lowers to Osprey's own `Result`. `Success(value)` and `Error(message)` construct its variants.

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
| `xs.length` | `listLength`, the number of elements of a list |
| `console.log(value)` | `print(value)`; exactly one argument |
| `new Map<string, T>()` | the empty map literal |
| `unwrapOr(result, fallback)` | Default `result ?: fallback`; the fallback is evaluated only for an error |
| `for (const x of iterator) { … }` | `forEach(iterator, callback)` |
| `{ ...record, field: value }` | immutable `Expr::Update` |
| `xs.map(f)`, `xs.filter(p)`, `xs.reduce(f, init)` | `mapList`, `filterList` and `foldList(xs, init, f)`; each returns a list or the folded value, as a JavaScript array method does |

`==`, `!=`, truthiness (a non-boolean operand of `&&`, `||`, `!` or a condition), `??`, `?.`, `typeof`, `instanceof`, `in`, `delete`, `void`, the comma operator, every other `new`, tagged templates, regular-expression literals, `while`, `do`, `for (;;)`, `for … in`, `break`, `continue`, labels, `throw`, `try`, `async`, `await`, generators and `yield` are rejected. Expected failure is a `Result`, matched exhaustively.

Integers do not follow JavaScript `number` semantics. An `int` is a 64-bit integer whose overflow goes through the `Arith` policy, not a binary64 value that loses precision above 2^53. A program that mixes `int` and `number` without an explicit conversion is rejected by Osprey even though `tsc` accepts it. `number` arithmetic is IEEE binary64, as in JavaScript.

A string's `.length` in TypeScript counts UTF-16 code units, and Osprey strings are UTF-8. `.length` lowers to `listLength`, so on a string it is a type error at the original position, never a silent byte count. `length(text)` is Osprey's byte count, spelled as the builtin it is. A `reduce` with no initial value is rejected: it has no result for an empty list. Because these method names lower before type inference, `map`, `filter` and `reduce` called as methods are always the list operations, and a record's own function field with one of those names must be called as `(record.map)(x)`.

Any other method call lowers to the shared method-call node, which resolves a record's function-valued field first and a free function of that name otherwise.

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

An interface that extends `Effect` or `StaticEffect`, and nothing else, declares an Osprey dynamic or static `Stmt::Effect`; each method signature is an operation with complete parameter and result types, and any other member is rejected. An operation whose result type is wrapped in `Control<T>` is a control operation. `Abort<T>`, `Once<T>` and `Many<T>` are control operations with that multiplicity, and `Replayable<T>` marks a replayable operation; wrappers nest, as in `Replayable<Many<int>>`, and a repeated wrapper is rejected. These wrappers are read syntactically by the frontend; the optional lint declares them as aliases of `T`.

`perform<E>().op(args)` lowers to `Expr::Perform` of operation `E.op`; it is recognized syntactically and is not an ordinary call, and a `perform<E>()` that is not called as an operation is rejected. `handler<E>({ … })` declares a callable handler value whose arms are the object's properties, and `handle<E>({ … }, () => body)` installs a lexical handler around `body`; `staticHandler<E>` and `handleStatic<E>` select the static stage. An arm is an arrow function whose parameters take their types from the operation, so an annotated arm parameter is rejected. The property `return: (x) => …` is the return clause and `finally: () => …` the finalizer. The body of `handle` is lowered in the enclosing function, so its bindings are that function's. `resume(value)` is permitted only in a control arm. Each name is an intrinsic only when no local shadows it.

A handler arm that assigns a `let` of an enclosing function writes handler-owned state ([EFFECTS-HANDLER-STATE](0017-AlgebraicEffects.md)): that `let` becomes an Osprey `mut` binding and the assignment a state write, so the code after the `handle` reads the value the arms left. Assigning the variable outside an arm after an arm captured it is rejected, as for any captured variable ([FLAVOR-TS-FUNCTIONS]). Required arms, arity, captures, stages and diagnostics follow [Algebraic Effects](0017-AlgebraicEffects.md). The spelling grants no capability the other flavors lack.

A closed effect annotation is the return-type wrapper `Requires<T, [Log, Arith]>`; inference remains the default.

`Requires` on a function-typed field or a method signature of an interface closes that field's effect row ([EFFECTS-FIELD-ROWS](0017-AlgebraicEffects.md)), so `send(body: string): Requires<Response, []>` declares a field whose stored function performs nothing. On a function declaration's return type it closes the function's row, as `fn work() -> int ![Log]` does. Every form in this section equals its Default AST, and the TypeScript twins in `src/osprey/tests/flavors/` share the Default goldens. Not implemented yet: `Requires` on an arrow function, `mask` and `instance`.

## Modules and concurrency — [FLAVOR-TS-INTEROP]

ES module syntax carries Osprey modules. A leading `// osprey: namespace=express` and `// osprey: module=Router` contribute the file to a logical namespace and module, as in the Python flavor. `export` marks a declaration public, and unmarked declarations stay module-private. Import resolution never executes code.

An import source names a module by convention, because the frontend reads one file at a time:

- A relative source names a module of the importing file's own namespace, so the file needs a namespace directive. The module is the PascalCase form of the file name: `import { total } from "./prices";` imports `total` from module `Prices`.
- Any other source names a package. Its namespace is the source, and its public module is the PascalCase form of that name: `"express"` is module `Express` of namespace `express`.
- A named import, `import { Request } from "express";`, imports those members.
- A default import, `import express from "express";`, imports every exported member of the package's module. Calling the default import calls the function named after the package, so `express()` is `Express::express`, and `express.Router()` is `Express::Router`. The package's other functions are in scope as the methods of its values: `app.get(path, handler)` is the method call that resolves to `Express::get(app, path, handler)`. The project that imports this way sets `[modules].allow_wildcard_imports = true`.

`export { remove as delete };` publishes a declared function under another name. It exists so that a module can export a function whose name TypeScript reserves, which a method call may still use.

Side-effect imports, `import *`, a default import of a project file, dynamic `import()`, `require`, `export default` and re-exports are rejected.

`spawn(work)`, `awaitFiber(fiber)`, `yieldFiber()`, `yieldFiber(value)`, `send(channel, value)` and `recv(channel)` lower to the existing fiber expressions with their existing isolation and target restrictions. They are not promises. `spawn(() => work(x))` spawns `work(x)`, and `spawn(work)` spawns `work()`; any other argument is rejected. A local of the same name is an ordinary value, which Default, where these are keywords, cannot express. A program's entry is Osprey's `main` selection, which top-level statements supply ([FLAVOR-TS-ENTRY]).

## First port: Express — [TS-PORT-EXPRESS]

The first TypeScript-flavor package port is Express, so that HTTP servers written in the TypeScript flavor run natively on Osprey. It follows the port rules of [Python-Flavor Package Ports](../ecosystem/0043-PythonFlavorPorts.md) with TypeScript in place of Python: [PORT-SCOPE], [PORT-TEST-ORACLE], [PORT-UPSTREAM-TEST-PARITY], [PORT-SUITE] and [PKG-COVERAGE-GATE] apply unchanged.

**[TS-PORT-EXPRESS-LAYOUT]** The package lives in `src/osprey/packages/express/`. Its `osprey.toml` lists `typescript_sources`, and its `src/*.ts` files use `// osprey: namespace=express`; the public module is `Express`. The README names the pinned Express version it is compared with, maps every ported function to the original and lists each divergence with its reason. The oracle is ordinary JavaScript under `oracle/` (not `.ts`, which would be a TypeScript-flavor source) and runs the pinned real Express on Node.js.

The port is plain records and free functions: an application is its ordered route table, and `get`, `post`, `listen` and the rest are functions whose first parameter is the application. A method call reaches them ([FLAVOR-TS-INTEROP]). It is built this way, and not from records of closures, so that callable provenance ([EFFECTS-PROVENANCE](0017-AlgebraicEffects.md)) follows each handler from its registration to the call that dispatches it. A response is the exception: its `status`, `set` and `send` are function-valued fields with closed empty effect rows, because they perform nothing.

**[TS-PORT-EXPRESS-SURFACE]** The port keeps Express's names and argument order where the flavor can spell them: `express()`, `app.get`, `app.post`, `app.put`, `app.delete`, `app.use`, `express.Router()`, `req.params`, `req.query`, `req.get`, `res.status`, `res.set`, `res.json`, `res.send` and `app.listen`. Values are immutable, so each registration returns the extended application. An application is built by chaining, which is also valid Express: `express().use(logger).get("/users/:id", show).listen(3000)`. The statement form Express code usually takes, `app.get(…);` on its own line, is a receiver statement ([FLAVOR-TS-RECEIVER]) and builds the same application.

Implemented today: `express()`, `express.Router()`, `app.get`, `app.post`, `app.put`, `app.delete`, route paths with `:name` captures, `app.use(path, handler)`, `app.mount(path, router)`, `req.params`, `req.query`, `req.get`, `jsonBody(req)`, `res.status`, `res.set`, `res.send`, `res.json`, error handlers, `app.listen`, and Express's final handler for an unmatched request. Routing follows Express's defaults: a literal segment matches without regard to case, and one trailing slash on the request path is ignored. Middleware matches a path prefix. `req.query` is parsed as Node's `querystring.parse` parses it, except that a repeated name keeps its first value. Osprey has no overloads by parameter count, so `app.use` always takes a path, and a router, which is an application rather than a handler, is mounted with `app.mount`. A mounted router's routes match under its path and its error handlers join the application's.

**[TS-PORT-EXPRESS-HANDLERS]** A handler returns the response it sends: `(req, res) => res.status(404).json(body)`. The same arrow is valid against Express, where `res.status()` and `res.json()` return the response and the handler's result is ignored. A handler written as statements, `(req, res) => { res.status(404); res.send("no"); }`, returns the response its receiver statements build. Every handler takes `(req, res)`: Osprey has no overloads by parameter count, so `next` is not a third parameter. A handler passes the request on by returning `req.next()`, which Express also sets on the request, and reports an error by returning `req.fail(error)`, the port's spelling of `next(error)`. Both return a response that carries a signal, and the dispatcher acts on it: it runs the next matching handler, or the error handlers. They call nothing themselves. A closure over the rest of the chain would be a stored callable whose effects the checker cannot prove ([EFFECTS-PROVENANCE](0017-AlgebraicEffects.md)), so a handler cannot change the reply that comes back from downstream.

An error is an `HttpError` record with a `status` and a `message`, never a thrown exception. `app.onError((error, req, res) => …)` registers an error handler, which is Express's four-parameter middleware. It replies, or returns `req.next()` to hand the same error to the next error handler, or `req.fail(other)` to hand on another. With none left, the final handler replies with the error's status.

`res.json(value)` takes Osprey's built-in `Json` value, so each record has an explicit encoder and JSON stays at the edge. A JSON request body is decoded in the handler by `jsonBody(req)`, a `Result` holding the `Json` document, never `any`: `JsonNull` when the request is not JSON, as Express leaves `req.body` unset, and the error `malformed JSON`, which Express's parser answers with 400, when the body is not a JSON object or array. Each such divergence from Express is listed in the README.

**[TS-PORT-EXPRESS-SERVE]** `app.listen(port, callback)` starts the native listener through the existing HTTP server ([HTTP-SERVER](../runtime/0014-HTTP.md)) and runs `callback`. The server then holds the process open ([HTTP-SERVER-KEEPALIVE](../runtime/0014-HTTP.md)): the program's entry returns, and the process keeps serving until the server is stopped, as a Node.js process does while a server is open. Request and response bytes, framing and client disconnects follow the HTTP server contract.

Express code calls `app.listen` as a statement and discards what it returns, and Osprey rejects a discarded `Result`. `app.listen` therefore returns a `Server` value, either `listening` with the server handle or `failed` with the reason, and it prints a listener that cannot start, as Node.js reports the unhandled error. A failure is never silent, and a program that wants to act on it switches on the returned value. `close(server)` stops a listening server and returns a `Result`. A failure inside the port while it serves a request becomes a 500 reply with Express's final-handler page, never a crash.

**[TS-PORT-EXPRESS-ORACLE]** Acceptance compares the port with the pinned real Express over real loopback HTTP: the same request corpus is sent to both servers, and status, security-relevant headers, content type, byte length and body bytes must match. The corpus covers routing and path parameters, repeated and empty query values, method rejection, middleware order, mounted routers, error handlers, JSON bodies (valid, malformed and wrongly typed) and binary bodies. The suite runs under native GC and ARC during source-coverage collection. A response that differs from Express only by a documented divergence is asserted as that divergence, never skipped.

Implemented today: `oracle/record.mjs` runs each example on the pinned Express under Node.js and records its replies. An example that is valid Express runs unchanged. An example ported from Express's own repository, such as `web-service`, is recorded from the original file kept in `oracle/upstream/`; `tests/e2e.mjs` compiles the same files under GC and ARC and compares every reply with the recording. The hello-world example is the example application from the Express README, byte for byte, and the test fails if the file changes. The corpus covers routing, unmatched methods and paths, query strings, trailing slashes, case, paths that need encoding and escaping, middleware order, error handlers, JSON replies, a mounted router, request headers, and JSON request bodies that are valid, malformed, wrongly typed, of another content type and binary. The compared parts include the status line's reason phrase.

## Acceptance — [FLAVOR-TS-VERIFICATION]

Each implemented section needs a source reference and a named regression. Frontend tests assert syntax conformance, exact lowering, diagnostics and positions. Differential programs in all four flavors share a Default golden and run on native GC and ARC with zero ARC leaks, WebAssembly and the supported mobile targets. Mixed projects import TypeScript-flavor modules from the other flavors and the reverse. Every rejection fixture names the Osprey rule it violates. The optional lint ([FLAVOR-TS-LINT]) may also run over the repository's TypeScript-flavor sources; it is not an acceptance condition for compiling them.

Editor acceptance includes completion, hover, diagnostics, rename and debugger locations through the Osprey language server, beside the editor's TypeScript service and without claiming ordinary TypeScript files. CLI test discovery, documentation, project discovery and CI change classification recognize only opted-in TypeScript-flavor sources. Existing required gates and coverage thresholds remain intact.

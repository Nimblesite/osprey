# Foreign Function Interface

Osprey calls C-ABI functions through `extern fn` declarations. Declaration
grammar and ABI types are specified in [Syntax](0003-Syntax.md#extern-declarations);
`--no-ffi` and `--sandbox` are specified in
[Security and Sandboxing](0016-SecurityAndSandboxing.md).

Each flavor has its own declaration spelling; both lower to `Stmt::Extern`.
ABI mapping, link directives, and linking are shared after that boundary
([FLAVOR-BOUNDARY]).

## Link Directives [FFI-LINK-DIRECTIVES]

A source comment directive links a system library at compile time:

```osprey
// @link: sqlite3        → clang -lsqlite3
// @linkdir: /opt/lib    → clang -L/opt/lib
```

Directives are read from the source file and passed to the compiler driver by
both `--run` and `--compile`. Each value is one process argument; no shell parses
it. Invalid library names or paths therefore fail in the compiler driver rather
than executing as commands.

## The `Ptr` Type [FFI-PTR]

`Ptr` is an opaque C pointer (`i8*`). It may appear in signatures and may be
stored or passed. It supports no arithmetic, dereference, or field access.

C out-parameters (`sqlite3_open(path, &db)`) use the runtime's **pointer cells** — themselves plain `extern fn` declarations against the bundled runtime archive, not builtins:

```osprey
extern fn osprey_ffi_cell() -> Ptr      // allocate a pointer-sized cell (pass where C expects T**)
extern fn osprey_ffi_deref(cell: Ptr) -> Ptr   // read back the pointer C wrote
extern fn osprey_ffi_free(cell: Ptr) -> int    // release the cell
extern fn osprey_ffi_null() -> Ptr             // a NULL argument
```

```osprey-ml
extern osprey_ffi_cell -> Ptr      // allocate a pointer-sized cell (pass where C expects T**)
extern osprey_ffi_deref (cell : Ptr) -> Ptr   // read back the pointer C wrote
extern osprey_ffi_free (cell : Ptr) -> int    // release the cell
extern osprey_ffi_null -> Ptr             // a NULL argument
```

## Callbacks [FFI-CALLBACKS]

A named top-level function passed where an `extern fn` expects a function parameter lowers to a raw C code pointer. A capture-free lambda is accepted the same way; a **capturing** lambda is a compile-time error (captures cannot cross the C boundary; use a named function).

## Databases Are Libraries [FFI-NO-DB-BUILTINS]

Database access is not compiler surface. Database drivers use `extern fn`
declarations; the SQLite example is tested in `examples/tested/db/`.

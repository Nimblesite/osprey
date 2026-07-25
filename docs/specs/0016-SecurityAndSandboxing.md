# Security and Sandboxing [SECURITY-CAPABILITY-GATES]

Capability flags reject restricted source before type checking and code
generation. A violation is a compile error naming the builtin and the flag
that disabled it.

## Flags

- `--sandbox` disables HTTP, WebSocket, filesystem, process, and FFI access.
- `--no-http` disables HTTP builtins.
- `--no-websocket` disables WebSocket builtins.
- `--no-fs` disables `readFile` and `writeFile`.
- `--no-ffi` rejects every `extern fn` declaration, including declarations in
  modules and namespaces.

The granular flags are independent. Process builtins (`spawnProcess`,
`awaitProcess`, and `cleanupProcess`) have no granular flag and are disabled by
`--sandbox`.

## Network gates

`--no-http` rejects the HTTP functions specified in [HTTP](0014-HTTP.md),
including the response-handle accessors. `--no-websocket` rejects the functions
specified in [WebSockets](0015-WebSockets.md).

## FFI gate [SECURITY-FFI-GATE]

`--no-ffi` gates foreign declarations, not libraries by name. SQLite and other
third-party C APIs are therefore disabled when their `extern fn` declarations
are present; there is no database-specific permission.

The sandbox pass checks all referenced identifiers in the parsed program,
including nested function and module bodies. It does not rely on a restricted
runtime archive or a post-link check.

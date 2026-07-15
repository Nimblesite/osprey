---
layout: page
title: "Security and Sandboxing"
description: "Osprey Language Specification: Security and Sandboxing"
date: 2026-07-15
tags: ["specification", "reference", "documentation"]
author: "Christian Findlay"
permalink: "/spec/0016-securityandsandboxing/"
---

# Security and Sandboxing

The compiler can disable categories of built-in functions at compile time. Restricted functions are not lowered into the program at all — calls to them produce "undefined function" compile errors. There is no runtime overhead.

> **Flavor layer — shared core (AST and above).**  Capability-based sandboxing and the effect system are one system across every flavor — entirely flavor-blind. The gated operations are ordinary calls that lower to `Expr::Call`; the permission gate enforces restrictions on the canonical `osprey_ast::Program` and in codegen, where restricted built-ins are simply never lowered or linked. Per [FLAVOR-BOUNDARY], no phase that decides what to block inspects which surface (`.osp` Default or `.ospml` ML) produced the program — security is enforced on the shared AST and below it in the runtime, never per-flavor. The flag spellings shown here are CLI concerns and are flavor-independent. See [Language Flavors](/spec/0023-languageflavors/).

## Security Flags

#### `--sandbox`
Enables sandbox mode, which disables all potentially risky operations:
- HTTP/HTTPS operations (httpCreateServer, httpGet, httpPost, etc.)
- WebSocket operations (websocketConnect, websocketSend, etc.)
- File system access (when implemented)
- Foreign Function Interface (FFI)
- Process execution

**Example:**
```bash
osprey program.osp --sandbox --llvm
```

#### Granular Security Flags

For more granular control, you can disable specific categories of operations:

- `--no-http`: Disable HTTP client and server functions
- `--no-websocket`: Disable WebSocket client and server functions  
- `--no-fs`: Disable file system read/write operations
- `--no-ffi`: Disable foreign function interface (also gates third-party C libraries such as SQLite)

**Examples:**
```bash
# Disable only HTTP operations
osprey program.osp --no-http --compile

# Disable HTTP and WebSocket operations
osprey program.osp --no-http --no-websocket --run

# Disable file system access only
osprey program.osp --no-fs --llvm
```

## Blocked Functions by Category

#### HTTP Functions
When HTTP access is disabled (`--no-http` or `--sandbox`), these functions are unavailable:
- `httpCreateServer` - Create HTTP server
- `httpListen` - Start HTTP server listening
- `httpStopServer` - Stop HTTP server
- `httpCreateClient` - Create HTTP client
- `httpGet` - HTTP GET request
- `httpPost` - HTTP POST request
- `httpPut` - HTTP PUT request
- `httpDelete` - HTTP DELETE request
- `httpRequest` - Generic HTTP request
- `httpCloseClient` - Close HTTP client

#### WebSocket Functions
When WebSocket access is disabled (`--no-websocket` or `--sandbox`), these functions are unavailable:
- `websocketConnect` - Connect to WebSocket server
- `websocketSend` - Send WebSocket message
- `websocketClose` - Close WebSocket connection
- `websocketCreateServer` - Create WebSocket server
- `websocketServerListen` - Start WebSocket server
- `websocketServerSend` - Send message to specific client
- `websocketServerBroadcast` - Broadcast message to all clients
- `websocketStopServer` - Stop WebSocket server

#### File System Functions (Future)
When file system access is disabled (`--no-fs` or `--sandbox`), these functions will be unavailable:
- `readFile` - Read file contents
- `writeFile` - Write file contents
- `deleteFile` - Delete file
- `createDirectory` - Create directory
- `listDirectory` - List directory contents

#### Third-Party C Libraries (FFI)
Database access is **not** a hardcoded builtin category. Osprey reaches SQLite (and any C library)
through the generic **FFI / interop** layer — `extern fn` declarations bound to the linked library
(see [Foreign Function Interface](/spec/0019-foreignfunctioninterface/)). It is therefore gated by `--no-ffi`
(`PermissionFFI`), exactly like any other foreign call. When FFI is disabled, `extern` declarations and
any library they bind (e.g. `libsqlite3`) are unavailable; no DB-specific permission exists.

## Compiler Output

When restrictions are active the compiler prints a summary line:

```
Security: SANDBOX MODE - All risky operations disabled
Security: Allowed=[FileRead,FileWrite,FFI] Blocked=[HTTP,WebSocket]
```

Restrictions cannot be bypassed by the compiled program; the relevant runtime functions are never linked in.
# Playground backend implementation status

The running backend is [server.js](server.js), launched by the package's `server` script and [Dockerfile](../Dockerfile). [server.osp](server.osp) is an unused earlier prototype; its TODOs do not describe missing compiler builtins or the deployed API.

## Implemented Node.js path

- Express parses JSON request bodies and serializes responses.
- The shared compiler runner always adds `--sandbox`; `/api/compile` generates an AST and `/api/run` compiles and executes source under that capability policy.
- Responses separate compiler diagnostics from program stdout and report compilation/runtime failure.
- Requests use independent temporary directories; the compiler child has a 20-second timeout and cleanup runs when it closes.
- `/lsp` bridges a WebSocket client to the compiler's stdio language server.
- The container includes the compiler and C runtime archives; the browser editor is built by the website.

## Remaining verification

- [x] Apply the compiler capability sandbox to both compile and run requests through the shared runner.
- [ ] Retain forbidden filesystem, FFI, HTTP and process probes in the checked-in API harness; `test.sh` currently checks a successful showcase only.
- [ ] Assert compiler-child and descendant cleanup on timeout and container shutdown, including concurrent requests.
- [ ] Exercise request/output bounds, malformed bodies, compiler failures and browser-origin handling through the actual API.

[The backend README](../README.md) documents setup and the capability boundary. [test.sh](../test.sh) verifies a successful showcase response; it does not prove all of these hardening requirements.

A replacement backend written in Osprey would need its own delivery plan, tests and explicit migration. The prototype is not an alternative implementation of today's service.

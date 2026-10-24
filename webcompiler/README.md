# Osprey playground backend

This directory contains the Node.js API and WebSocket bridge used by the [website playground](../website/src/playground/index.md). The editor lives in the website; there is no separate `webcompiler/index.html`.

| Route | Behavior |
| --- | --- |
| `GET /api` | Health and service information |
| `POST /api/compile` | Parse submitted source with `osprey --sandbox --ast` |
| `POST /api/run` | Compile and execute submitted source with `osprey --run` |
| WebSocket `/lsp` | Forward JSON-RPC to an `osprey lsp` child over stdio |

POST bodies use `{"code":"print(\"Hello\")","flavor":"default"}`. The server names ML requests `.ospml` and other requests `.osp`; source markers still pass to the compiler. Responses separate `compilerOutput` and `programOutput`, with a `success` flag. Each compiler request gets a UUID-named temporary directory and a 20-second child-process timeout.

## Run the backend

From the repository root, with Docker installed and running:

```sh
./webcompiler/start.sh
```

The script rebuilds the image, replaces the local `osprey-web-compiler` container, maps port 3001 and follows its logs. The image supplies Node.js, the compiler, LLVM/clang and C runtime archives; Python is not a backend prerequisite. In another terminal:

```sh
curl http://localhost:3001/api
```

For a local Node.js process, first build the compiler/runtime with `make build`, then run `npm ci` and `npm run server` from this directory. Development mode uses `target/release/osprey`; the container uses `osprey` on `PATH`.

The website runs separately with `npm run dev` from `website/`. Its playground currently configures the hosted API URL; set `API_URL` in the playground source to the local backend when testing both together. The backend allows the website's localhost:8080 origin.

## Capability boundary

The compile endpoint requests the compiler's capability sandbox. The run endpoint currently omits `--sandbox`, so the backend does not yet enforce the same policy for execution. The container's non-root user, memory limit and compiler timeout do not establish file, network or FFI confinement. This gap requires an API regression and implementation correction before the backend can claim sandboxed execution.

## Verification and ownership

[server.js](src/server.js), [Dockerfile](Dockerfile) and [start.sh](start.sh) define the running backend. [test.sh](test.sh) calls the local run endpoint with the assertion-driven showcase and requires its TAP success result; it needs `curl` and `jq` and is not a capability-isolation test.

[src/server.osp](src/server.osp) is an unused earlier prototype. Its [implementation note](src/plan.md) is separate from the Node.js service's current delivery status.

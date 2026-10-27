# Osprey playground backend

This directory contains the Node.js API and WebSocket bridge used by the [website playground](../website/src/playground/index.md). The editor lives in the website; there is no separate `webcompiler/index.html`.

| Route | Behavior |
| --- | --- |
| `GET /api` | Health and service information |
| `POST /api/compile` | Parse submitted source with `osprey --sandbox --ast` |
| `POST /api/run` | Compile and execute submitted source with `osprey --sandbox --run` |
| WebSocket `/lsp` | Forward JSON-RPC to an `osprey lsp` child over stdio |

POST bodies use `{"code":"print(\"Hello\")","flavor":"default"}`. Set `flavor` to `default`, `ml` or `python`; the server names their source files `.osp`, `.ospml` or `.py` respectively. Python flavor compiles Osprey source, not arbitrary CPython programs. Source markers still pass to the compiler. Responses separate `compilerOutput` and `programOutput`, with a `success` flag. Each compiler request gets a UUID-named temporary directory and a 20-second child-process timeout.

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

The shared compiler runner adds `--sandbox` to every compile and run request. The compiler rejects disabled filesystem, HTTP/WebSocket, process and FFI capabilities before generating or executing a program. Container permissions, memory limits and child timeouts are separate controls; the capability gate does not prove complete operating-system isolation or descendant cleanup.

## Verification and ownership

[server.js](src/server.js), [Dockerfile](Dockerfile) and [start.sh](start.sh) define the running backend. [test.sh](test.sh) requires the local run endpoint's showcase to pass, then checks that both endpoints reject forbidden filesystem, FFI, HTTP and process operations with HTTP 422 and the expected security diagnostic. It needs `curl` and `jq`; these capability probes do not establish full operating-system isolation.

[src/server.osp](src/server.osp) is an unused earlier prototype. Its [implementation note](src/plan.md) is separate from the Node.js service's current delivery status.

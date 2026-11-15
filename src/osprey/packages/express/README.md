# Express in TypeScript flavor Osprey

A port of [Express](https://github.com/expressjs/express) for Osprey's TypeScript flavor. An Express application written in the supported surface compiles to a native binary and serves HTTP with no Node.js and no JavaScript engine.

The pinned reference is Express 5.3.0. The port is in progress: the surface below is what exists, and everything else in Express is not ported yet.

## The Express README example runs unchanged

[`examples/hello-world/index.ts`](examples/hello-world/index.ts) is the example application from the README of Express 5.3.0, byte for byte. The test suite fails if the file changes.

```ts
import express from 'express'

const app = express()

app.get('/', (req, res) => {
  res.send('Hello World')
})

app.listen(3000, () => {
  console.log('Server is running on http://localhost:3000')
})
```

Run it from the repository root, then request it:

```sh
make build
target/debug/osprey src/osprey/packages/express/examples/hello-world --run
curl -i http://127.0.0.1:3000/
```

The process keeps serving until it is stopped, as the same file does under Node.js.

[`examples/routes/index.ts`](examples/routes/index.ts) is an original example that registers GET, POST, PUT and DELETE routes, sets headers and statuses, and uses both the statement form and the chained form.

## Compared with real Express [TS-PORT-EXPRESS-ORACLE]

`make -C src/osprey/packages express-e2e` compiles each example under GC and ARC, serves it natively and sends it the requests in [`tests/corpus.json`](tests/corpus.json) over real loopback HTTP. Every reply is compared with what Express 5.3.0 replied to the same file under Node.js: status, body, `Content-Type`, `Content-Length`, `X-Powered-By`, `Content-Security-Policy` and `X-Content-Type-Options`. 52 replies match.

The Express replies are recorded in [`tests/examples.expected.json`](tests/examples.expected.json) by `make -C src/osprey/packages express-oracle`, which runs the same example files on the pinned Express. The recording is never edited by hand, and CI compares against it without installing Express.

## Measured against real Express [TS-PORT-EXPRESS-BENCH]

`make -C src/osprey/packages express-bench` serves each example from Osprey and from Express 5 on Node.js, and sends both the same requests over loopback: 16 in flight, a fresh connection per request, 10,000 timed requests after 1,000 warm-up requests, five trials with the servers taken in rotation. Every timed reply is checked for its status, body and content type.

Apple M4 Max, 14 cores, macOS, Node.js 24.19.0, 15 November 2026, medians of five trials:

| Example | Server | Requests/s | Median latency µs | p95 latency µs | Peak memory MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| hello-world | Osprey, GC | 17,628 | 704 | 1,025 | 97 |
| hello-world | Osprey, ARC | 21,920 | 679 | 862 | 3 |
| hello-world | Node.js Express | 20,854 | 684 | 940 | 157 |
| routes | Osprey, GC | 18,913 | 691 | 1,003 | 100 |
| routes | Osprey, ARC | 22,241 | 668 | 851 | 3 |
| routes | Node.js Express | 20,665 | 668 | 879 | 153 |

Under ARC the port answers 5 to 8 percent more requests per second than Express and uses about a fiftieth of the memory. Under GC it answers 8 to 15 percent fewer. Osprey handlers do not yet run in parallel across cores ([#397](https://github.com/Nimblesite/osprey/issues/397)).

## How Express code maps to Osprey

Osprey values are immutable, and Express code updates `app` and `res` in place. The TypeScript flavor bridges the two in the frontend ([FLAVOR-TS-RECEIVER](../../../../docs/specs/language/0054-TypeScriptFlavorSyntax.md)):

- A statement such as `app.get('/', handler)` or `res.status(201)` rebinds its local to the value the call returns, so later statements see the update.
- A handler that updates `res` this way and returns nothing returns the final `res`.
- The chained form, `app.get(…).post(…)` or `res.status(404).send(…)`, needs no bridging and is also valid Express.

The compiler rejects the cases where rebinding would differ from in-place update: an update inside a branch that later code would not see, an update after a function has captured the value, and a function that updates two parameters.

## Ported surface [TS-PORT-EXPRESS-SURFACE]

| Express | Port |
| --- | --- |
| `express()` | `express()` |
| `app.get`, `app.post`, `app.put`, `app.delete` | the same names; each returns the extended application |
| route paths with `:name` captures | matched in registration order; a literal segment ignores case and one trailing slash is optional, as in Express's default routing |
| `res.status(code)`, `res.set(name, value)`, `res.send(text)` | the same names; each returns the extended response |
| `app.listen(port, callback)` | starts the native listener on `127.0.0.1` and runs `callback`; the process then stays alive while the server listens ([HTTP-SERVER-KEEPALIVE](../../../../docs/specs/runtime/0014-HTTP.md)) |
| the final handler's `Cannot METHOD /path` page | the same page, status and security headers, with the path encoded and escaped as Express does |

Not ported yet: `req.params` and `req.query` access from handlers, `req.get`, `res.json`, JSON bodies, middleware and `next()`, `app.use`, `express.Router()`, error middleware, and HEAD and OPTIONS handling.

## Divergences

- **No `ETag`.** Express adds a weak `ETag` to a successful reply. The port sends none, and the end-to-end test asserts that it sends none.
- **`app.listen` binds `127.0.0.1`.** Express binds every interface when no host is given.
- **A listener that cannot start.** Node.js reports the unhandled error and exits with a failure status. The port prints the error and returns a `failed` server value; the program then ends with status 0 because nothing is listening. The printed reason is the native listener's, which today says only `Error`.
- **`app.listen` returns a `Server` value**, either `listening` with its handle or `failed` with the reason, where Express returns Node's `http.Server`. `close(server)` stops a listening server.
- **Integers are 64-bit.** A port or status is an `int`, not a binary64 `number`.

## Origin, license and thanks

**Original project:** [Express](https://github.com/expressjs/express).

**How it was ported.** Osprey's TypeScript flavor reads like TypeScript, so this port keeps Express's function and argument names wherever it can, and its tests compare replies with Express 5.3.0 running under Node.js. It is still a reimplementation: the code is written for Osprey's types, effects and runtime, and Express is not bundled, linked or needed at run time.

**License.** Express is MIT licensed. This package is MIT, Copyright NIMBLESITE PTY LTD, Australia; that grant covers Osprey's own code and does not relicense anything of Express's. [NOTICE.md](NOTICE.md) lists the third-party material the package carries and the terms that come with it (copies in [LICENSES/](LICENSES/)).

**Thanks** to TJ Holowaychuk, Roman Shtylman, Douglas Christopher Wilson and the Express contributors for [Express](https://github.com/expressjs/express). Naming Express credits it; it does not imply that its maintainers endorse this port.

## Development

- `make -C src/osprey/packages express-e2e`: the end-to-end comparison above. Logs are in `target/express/`.
- `target/debug/osprey src/osprey/packages/express/tests --run`: the package suite, a TypeScript-flavor program that serves the routes example over loopback and checks every reply against the Express recording. The package matrix runs it under GC and ARC.
- `make -C src/osprey/packages express-oracle`: re-record the Express replies. It needs Node.js and installs the pinned Express into `oracle/node_modules`.
- The port's source is [`src/express.ts`](src/express.ts), one TypeScript-flavor module, `express::Express`.

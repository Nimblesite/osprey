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

[`examples/web-service`](examples/web-service/service.ts) is Express's own [`examples/web-service`](https://github.com/expressjs/express/tree/v5.3.0/examples/web-service), ported: an API key checked by middleware mounted on `/api`, JSON replies, an error handler and a JSON 404. It does not run unchanged, because it relies on `next(error)`, thrown `Error` objects and in-place updates. The original is kept in [`oracle/upstream/web-service/index.js`](oracle/upstream/web-service/index.js), and the tests compare the port's replies with it running on real Express.

```ts
function checked(req: Request, res: Response) {
  const key = mapGet(req.query, "api-key");
  switch (key.kind) {
    case "error":
      return req.fail(httpError(400, "api key required"));
    case "success":
      return known(req, key.value);
  }
}

app.use("/api", checked);
app.get("/api/users", (req, res) => res.json(JsonArray(mapList(users, userJson))));
```

[`examples/json-api`](examples/json-api/api.ts) decodes and validates JSON request bodies, mounts a router on `/tasks` and reads a request header. Its reference, [`oracle/upstream/json-api/index.js`](oracle/upstream/json-api/index.js), is an original Express application written for the comparison, and the tests compare the two reply by reply, including malformed, wrongly typed and binary bodies.

[`examples/routes/index.ts`](examples/routes/index.ts) is an original example that registers GET, POST, PUT and DELETE routes, sets headers and statuses, and uses both the statement form and the chained form.

## Compared with real Express [TS-PORT-EXPRESS-ORACLE]

`make -C src/osprey/packages express-e2e` compiles each example under GC and ARC, serves it natively and sends it the requests in [`tests/corpus.json`](tests/corpus.json) over real loopback HTTP. Every reply is compared with what Express 5.3.0 replied to the same file under Node.js: status, body, `Content-Type`, `Content-Length`, `X-Powered-By`, `Content-Security-Policy` and `X-Content-Type-Options`, and the status line's reason phrase. 164 replies match: 82 requests under each memory backend.

The Express replies are recorded in [`tests/examples.expected.json`](tests/examples.expected.json) by `make -C src/osprey/packages express-oracle`, which runs the same example files on the pinned Express. The recording is never edited by hand, and CI compares against it without installing Express.

## Measured against real Express [TS-PORT-EXPRESS-BENCH]

`make -C src/osprey/packages express-bench` serves each example from Osprey and from Express 5 on Node.js, and sends both the same requests over loopback: 16 in flight, a new connection for each request, 10,000 timed requests after 1,000 warm-up requests, five trials with the servers taking turns. Every timed reply is checked for its status, body and content type.

One run on an Apple M4 Max (14 cores, macOS 26.7, Node.js 24.19.0) on 16 November 2026, with nothing else heavy running. Each figure is the median of five trials. The [TypeScript flavor film](https://www.ospreylang.dev/docs/typescript/flavor/) shows an earlier run, from before Osprey's HTTP handlers ran one at a time.

| Example | Server | Requests/s | Median latency µs | p95 latency µs | Peak memory MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| hello-world | Osprey, GC | 19,888 | 657 | 788 | 103 |
| hello-world | Osprey, ARC | 24,260 | 615 | 719 | 3 |
| hello-world | Node.js Express | 25,736 | 583 | 697 | 145 |
| routes | Osprey, GC | 19,609 | 652 | 810 | 108 |
| routes | Osprey, ARC | 24,264 | 611 | 675 | 3 |
| routes | Node.js Express | 25,485 | 574 | 707 | 146 |
| web-service | Osprey, GC | 15,066 | 836 | 1,126 | 214 |
| web-service | Osprey, ARC | 24,680 | 600 | 672 | 3 |
| web-service | Node.js Express | 25,263 | 584 | 730 | 126 |

Under ARC the port answers 2 to 6 percent fewer requests per second than Express, and its 95th-percentile latency is lower on two of the three examples. The ARC build uses about a fortieth of the memory. Under GC the port answers 23 percent fewer requests per second on the two small examples and 40 percent fewer on web-service, which allocates a JSON reply for every request. Osprey runs one handler at a time, so its handlers do not yet use more than one core ([#397](https://github.com/Nimblesite/osprey/issues/397)).

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
| `res.status(code)`, `res.set(name, value)`, `res.type(mediaType)`, `res.send(text)` | the same names; each returns the extended response. `res.type` takes a full media type such as `text/plain; charset=utf-8`; Express also accepts an extension and looks it up, which the port does not |
| `res.json(value)` | takes Osprey's built-in `Json` value, built with `JsonObject`, `JsonArray` and the other constructors, and replies with `application/json; charset=utf-8` |
| `req.params`, `req.query` | maps from name to text, read with `mapGet`; the query is parsed as Node's `querystring.parse` parses it |
| `req.get(name)` | the header's value as a `Result`, matched without regard to case |
| `express.json()` and `req.body` | `jsonBody(req)` in the handler: `JsonNull` when the request is not JSON, the `Json` document when it is, and the error `malformed JSON` when it is not an object or an array, which Express's parser answers with 400 |
| `express.Router()` | the same name; a router is an application of routes |
| `app.use(path, router)` | `app.mount(path, router)` |
| `app.use(path, handler)` | runs for every method on `path` and the paths under it; the path is required, and `"/"` means every request |
| `next()` | a handler returns `req.next()` to pass the request to the next matching handler |
| `next(error)` | a handler returns `req.fail(error)`, where `error` is an `HttpError` record with a `status` and a `message` |
| error middleware, `app.use((err, req, res, next) => …)` | `app.onError((error, req, res) => …)`; it replies, or returns `req.next()` or `req.fail(other)` to reach the next error handler |
| `app.listen(port, callback)` | starts the native listener on `127.0.0.1` and runs `callback`; the process then stays alive while the server listens ([HTTP-SERVER-KEEPALIVE](../../../../docs/specs/runtime/0014-HTTP.md)) |
| the final handler's `Cannot METHOD /path` page | the same page, status and security headers, with the path encoded and escaped as Express does |

Not ported yet: HEAD and OPTIONS handling, `res.redirect`, `res.sendFile` and static files, and every other `req` and `res` member.

## Divergences

- **Handlers take two parameters.** Express passes `next` as a third parameter. Osprey has no overloads by parameter count, so a handler reaches it as `req.next()`, which Express also sets on the request.
- **`req.next()` and `req.fail(error)` must be returned.** They return a response that tells the dispatcher what to do, and they run nothing themselves. A handler cannot call `req.next()` and then change the reply that comes back, as Express middleware can.
- **Errors are values.** Express passes any thrown or forwarded value to error middleware. The port passes an `HttpError` record, and error handlers are registered with `app.onError`. With no error handler left, the reply is the error's status and its message on Express's error page, where Express prints a stack trace outside production.
- **A repeated query name keeps its first value.** Express collects repeated names into an array. A percent escape of a byte outside printable ASCII stays as written, because Osprey has no conversion from a byte to text yet.
- **`app.use` takes a path and one handler.** Osprey has no overloads by parameter count, so `app.use(handler)` is written `app.use("/", handler)`, and a router is mounted with `app.mount(path, router)` because a router is not a handler.
- **JSON bodies are decoded in the handler.** `express.json()` sets `req.body` before the handler runs, and Osprey values cannot change, so a handler calls `jsonBody(req)` and matches its result. Only a JSON content type is read, and the document must be an object or an array, as `express.json()` requires by default. A router's error handlers join the application's.
- **`res.json` takes a `Json` value.** Express serialises any object. Osprey keeps JSON at the edge: each record has an encoder function.
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
- `target/debug/osprey src/osprey/packages/express/tests --run`: the package suite, a TypeScript-flavor program that serves the routes and web-service examples over loopback and checks every reply against the Express recording, then the error handler chain and query strings. The package matrix runs it under GC and ARC.
- `make -C src/osprey/packages express-oracle`: re-record the Express replies. It needs Node.js and installs the pinned Express into `oracle/node_modules`.
- The port's source is four TypeScript-flavor modules: [`src/express.ts`](src/express.ts) (`express::Express`), the header lookup [`src/headers.ts`](src/headers.ts), the query parser [`src/query.ts`](src/query.ts) and the text encoders [`src/encode.ts`](src/encode.ts).

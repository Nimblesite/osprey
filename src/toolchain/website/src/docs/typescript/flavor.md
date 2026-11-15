---
mlTwins: manual
layout: page
video: express-typescript
eleventyComputed:
  title: "{{ (video | video).name }}"
description: The TypeScript flavor is Osprey source written in TypeScript syntax. The Osprey compiler checks it and compiles it to a native binary with no Node.js and no JavaScript engine. Its first port is Express, measured here against Express on Node.js.
permalink: /docs/typescript/flavor/
tags: [flavors, typescript, express, http, video, tutorials]
---

{% videoPlayer video %}

**Safe, elegant, fast:** the TypeScript flavor is Osprey written in TypeScript syntax. The film opens on the Express port's comparison bench, run live: the same Express applications, served by Osprey and by Express on Node.js, under the same load. Then it serves the Express README example natively, requests it, runs Express's own web-service example as ported, and changes a reply.

The flavor is in progress. What works today is listed [below](#what-works-today), beside what does not. The contract is [spec 0054](https://github.com/Nimblesite/osprey/blob/main/docs/specs/language/0054-TypeScriptFlavorSyntax.md), and [plan 0061](https://github.com/Nimblesite/osprey/blob/main/docs/plans/0061-typescript-flavor.md) records what is built.

## Express on Osprey, measured against Node.js

[`@osprey/express`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/express) is a port of Express 5 written in the TypeScript flavor. Its bench serves each example file three ways: compiled by Osprey with the tracing garbage collector (GC), compiled by Osprey with reference counting (ARC), and run by Express 5.3.0 on Node.js.

Each server gets the same requests over loopback: 16 in flight, a new connection for each request, 1,000 warm-up requests and then 10,000 timed ones. That runs for five trials, with the servers taking turns. Every timed reply is checked against the reply Express recorded for the same file, so a wrong answer does not count.

The run the film shows, on an Apple M4 Max with Node.js 24.19.0 on 15 November 2026. Each figure is the median of five trials: requests per second, then peak memory.

| Example | Osprey, ARC | Osprey, GC | Express on Node.js |
| --- | ---: | ---: | ---: |
| hello-world | 22,691/s · 3 MiB | 17,669/s · 103 MiB | 22,432/s · 144 MiB |
| routes | 22,545/s · 3 MiB | 17,928/s · 107 MiB | 21,942/s · 152 MiB |
| web-service | 20,957/s · 3 MiB | 12,845/s · 214 MiB | 21,058/s · 130 MiB |

Median latency was between 650 and 700 µs for every server except the GC build of web-service, at 1,065 µs. The [package README](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/express#measured-against-real-express-ts-port-express-bench) has the full table with 95th-percentile latency.

How to read it:

- **Throughput is close.** Under ARC, Osprey and Express are within 3 percent of each other on every example, and neither leads on all three. Across other runs on the same machine the gap moves by a few percent either way. Neither is clearly faster.
- **Memory is not close.** The ARC build peaked at 3 MiB. Node.js peaked at 130 to 152 MiB.
- **GC is the slowest.** The GC build answered 18 to 21 percent fewer requests per second than Express on the two small examples and 39 percent fewer on web-service, which allocates a JSON reply for every request.
- **The examples are small.** web-service is Express's own example, with middleware, JSON replies and an error handler, but three small applications say nothing about a large service. Osprey handlers do not yet run in parallel across cores ([issue 397](https://github.com/Nimblesite/osprey/issues/397)).

Run it on your own machine from a checkout. It needs Node.js for the Express side:

```sh
make build
(cd src/osprey/packages/express/oracle && npm ci)
make -C src/osprey/packages express-bench
```

## The Express README example runs unchanged

This is the example application from the Express 5.3.0 README, byte for byte. It is [`examples/hello-world/index.ts`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/express/examples/hello-world/index.ts) in the package:

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

Compile and serve it, then request it from a second terminal:

```sh
target/release/osprey src/osprey/packages/express/examples/hello-world --run
curl -i http://127.0.0.1:3000/
```

```text
HTTP/1.1 200 OK
X-Powered-By: Express
Content-Type: text/html; charset=utf-8
Content-Length: 11
Connection: keep-alive
Keep-Alive: timeout=5

Hello World
```

The process keeps serving until you stop it, as the same file does under Node.js. A path with no route gets Express's own 404 page and security headers. The package's end-to-end test compares 64 replies with what Express 5.3.0 sent for the same files: status, body and the headers Express sets. `ETag` and `Date` are not sent yet ([issue 398](https://github.com/Nimblesite/osprey/issues/398)), so the replies are not byte-identical to Express.

## How the TypeScript flavor differs from TypeScript

A **syntax flavor** is a way to write Osprey. Default, ML, Python and TypeScript flavors share one compiler, one type checker and one effect checker. TypeScript-flavor files end in `.ts`, as TypeScript files do, but a different program runs them:

| | TypeScript flavor | TypeScript |
| --- | --- | --- |
| Language | Osprey | TypeScript, compiled to JavaScript |
| Run by | `osprey`, which compiles a native binary | Node.js or a browser |
| Types | Inferred by Osprey and checked before the program runs | Checked by `tsc`, erased at run time |
| Unannotated parameter | Its type is inferred | `any` |
| Values | Immutable records; failure is a `Result` | Mutable objects; exceptions |
| `null`, `Promise`, the Node.js API | Not available | Available |
| Imports | Osprey modules, including Osprey's ports | npm packages |

Osprey rejects valid TypeScript that breaks its rules, such as `null`, assignment to a field or a `switch` that misses a case. The error is reported at the TypeScript line. It never falls back to JavaScript behaviour, and an arbitrary TypeScript program or npm package does not run unchanged.

A project opts its `.ts` files in by listing them, so the compiler never claims every TypeScript file in a folder:

```toml
[project]
name = "express-hello-world"
source_roots = [".", "../../src"]
typescript_sources = ["../../src/express.ts", "index.ts"]
entry = "index.ts"
```

A single file can opt in with a first line of `// osprey: flavor=typescript`, or with `--flavor typescript` on the command line.

## Checking with tsc is optional

Because the syntax is TypeScript, you can also run `tsc` over the same files, as you would run any linter. Editors and formatters that know TypeScript then see the same program. The Osprey compiler never runs `tsc`, never reads `tsconfig.json` and builds the same binary whether or not the lint has run. In the repository the lint is `make typescript-lint`.

## What works today

Built and tested:

- Functions and arrow functions, with inferred parameter and return types.
- Interfaces as records, and unions discriminated by a `kind` field.
- `switch` on `kind`, checked for missing cases, with each case's fields in scope.
- Object literals where the expected type is known, and object spread.
- Template literals, `const` bindings, ES `import` and `export`, and documentation comments.
- Statements such as `app.get(...)` and `res.send(...)` that rebind the value they are called on.
- In `@osprey/express`: `express()`, `get`, `post`, `put`, `delete`, `:name` path segments, `res.status`, `res.set`, `res.send`, `app.listen`, `server.close` and Express's final 404 handler.

Not built yet:

- Effects and handlers, apart from `Requires` on a field.
- Fibers, reassigned `let`, `map`, `filter` and `reduce` on arrays.
- Editor support for `.ts` files in the Osprey extension.
- In `@osprey/express`: middleware and `next()`, `express.Router()`, `res.json`, JSON bodies, error middleware, and reading `req.params` and `req.query`.

Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.

## Next step

Read [`src/express.ts`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/express/src/express.ts). It is the whole port in one file, written in the flavor it serves.

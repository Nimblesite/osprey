---
mlTwins: manual
layout: page
video: express-typescript
eleventyComputed:
  title: "{{ (video | video).name }}"
description: "Write Osprey in TypeScript syntax and compile it to a native binary with no Node.js. Its first port is Express, benchmarked against Express on Node.js."
permalink: /docs/typescript/flavor/
tags: [flavors, typescript, express, http, video, tutorials]
---

{% from "native-capture.njk" import capture %}
{% videoPlayer video %}

The TypeScript flavor is Osprey written in TypeScript syntax: the `osprey` compiler checks it with Osprey's types and effects and builds a native binary, with no Node.js. The film opens on the Express port's comparison bench, run live: the same Express applications, served by Osprey and by Express on Node.js, under the same load. Then it serves the Express README example natively, requests it, runs Express's own web-service example as ported, and changes a reply.

The flavor is in progress. What works today is listed [below](#what-works-today), beside what does not. The contract is [spec 0054](https://github.com/Nimblesite/osprey/blob/main/docs/specs/language/0054-TypeScriptFlavorSyntax.md).

## Express on Osprey, measured against Node.js

[`@osprey/express`](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/express) is a port of Express 5 written in the TypeScript flavor. Its bench serves each example file three ways: compiled by Osprey with the tracing garbage collector (GC), compiled by Osprey with reference counting (ARC), and run by Express 5.3.0 on Node.js.

Each server gets the same requests over loopback: 16 in flight, a new connection for each request, 1,000 warm-up requests and then 10,000 timed ones. That runs for five trials, with the servers taking turns. Every timed reply is checked against the reply Express recorded for the same file, so a wrong answer does not count.

A run on an Apple M4 Max with Node.js 24.19.0 on 16 November 2026, with nothing else heavy running. Each figure is the median of five trials: requests per second, then peak memory. The film shows an earlier run, from before Osprey's HTTP handlers ran one at a time.

| Example | Osprey, ARC | Osprey, GC | Express on Node.js |
| --- | ---: | ---: | ---: |
| hello-world | 24,260/s · 3 MiB | 19,888/s · 103 MiB | 25,736/s · 145 MiB |
| routes | 24,264/s · 3 MiB | 19,609/s · 108 MiB | 25,485/s · 146 MiB |
| web-service | 24,680/s · 3 MiB | 15,066/s · 214 MiB | 25,263/s · 126 MiB |

{{ capture("/assets/images/guides/typescript-express-bench.png", "The editor shows the Express README example in index.ts. Below it, the terminal prints the bench table: hello-world, routes and web-service, each served by osprey-gc, osprey-arc and node-express, with requests per second, median and 95th-percentile latency in microseconds, and peak RSS in MiB.", "The bench as the film runs it: the same three applications served by Osprey (GC and ARC) and by Express on Node.js.", 1600, 900) }}

Median latency was between 574 and 657 µs for every server except the GC build of web-service, at 836 µs. The [package README](https://github.com/Nimblesite/osprey/tree/main/src/osprey/packages/express#measured-against-real-express-ts-port-express-bench) has the full table with 95th-percentile latency.

How to read it:

- **Express is slightly faster.** Under ARC, Osprey answered 2 to 6 percent fewer requests per second than Express on every example. Its 95th-percentile latency was lower on two of the three.
- **Memory is not close.** The ARC build peaked at 3 MiB. Node.js peaked at 126 to 146 MiB.
- **GC is the slowest.** The GC build answered 23 percent fewer requests per second than Express on the two small examples and 40 percent fewer on web-service, which allocates a JSON reply for every request.
- **The examples are small.** web-service is Express's own example, with middleware, JSON replies and an error handler, but three small applications say nothing about a large service. Osprey runs one handler at a time, so its handlers do not yet use more than one core ([issue 397](https://github.com/Nimblesite/osprey/issues/397)).

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

The process keeps serving until you stop it, as the same file does under Node.js. A path with no route gets Express's own 404 page and security headers. The package's end-to-end test compares 164 replies from four example applications with what Express 5.3.0 sent: status line, body and the headers Express sets. `ETag` and `Date` are not sent yet ([issue 398](https://github.com/Nimblesite/osprey/issues/398)), so the replies are not byte-identical to Express.

{{ capture("/assets/images/guides/typescript-express-port.png", "Lines 131 to 142 of src/osprey/packages/express/src/express.ts: the function signalled returns a response record whose status, set, send and json fields each build a new response with one thing changed.", "The port itself is TypeScript-flavor code: each Express response method returns a new, immutable response.", 1600, 900) }}

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

Built and tested, with TypeScript twins of the corpus programs sharing the Default goldens:

- Functions and arrow functions, with inferred parameter and return types; `let` and parameters that are reassigned.
- Interfaces as records, and unions discriminated by a `kind` field, with `switch` on `kind` checked for missing cases.
- Object literals, which take the record type a declared context gives them, and object spread.
- `xs.map`, `xs.filter` and `xs.reduce` on lists, template literals, `for … of`, ES `import` and `export`, and documentation comments.
- Effects: `interface Log extends Effect`, `perform<Log>().write(…)`, `handler<Log>({ … })`, `handle<Log>({ … }, () => …)`, `resume`, handler state, static handlers and `Requires<T, [Log]>`.
- Fibers: `spawn`, `awaitFiber`, `send` and `recv`.
- Imports between all four syntax flavors in one project.
- Editor support: the Osprey extension checks each `.ts` file a project lists, beside VS Code's own TypeScript checks, and `osprey test` finds `.test.ts` suites.
- In `@osprey/express`: routing and `:name` parameters, `req.params`, `req.query`, `req.get`, JSON request bodies through `jsonBody(req)`, middleware with `app.use` and `req.next()`, error handlers, `express.Router()` mounted with `app.mount`, `res.status`, `res.set`, `res.send`, `res.json`, `app.listen` and Express's 404 page.

Not built yet:

- `null`, `undefined`, classes, exceptions, `async` and `await`; failure is a `Result`, and concurrency is fibers.
- An object literal with no declared type that is used later as a record ([issue 405](https://github.com/Nimblesite/osprey/issues/405)).
- In `@osprey/express`: HEAD and OPTIONS handling, redirects, static files, `ETag` and `Date` headers, and Express's overloads by parameter count, such as `app.use(handler)` without a path.

Osprey is actively evolving. Try it on your projects, validate it against your workload, and assess the risks before relying on it in production.

## Next step

Read [`src/express.ts`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/express/src/express.ts), the port's main module, written in the flavor it serves, then the ported [`web-service`](https://github.com/Nimblesite/osprey/blob/main/src/osprey/packages/express/examples/web-service/service.ts) example.

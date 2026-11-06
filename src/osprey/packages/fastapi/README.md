# FastAPI in Python flavor Osprey

A port of FastAPI's public behavior for Osprey's Python flavor.

The pinned reference is FastAPI 0.135.3. This port is MIT licensed by NIMBLESITE PTY LTD, Australia, and retains FastAPI's MIT notice, Copyright (c) 2018 Sebastián Ramírez; see [NOTICE.md](NOTICE.md). Compiled applications do not embed the CPython framework.

## Origin, license and thanks

**Original project:** [FastAPI](https://github.com/fastapi/fastapi).

**How it was ported.** Osprey's Python flavor reads like Python, so this port keeps FastAPI's module, function and argument names wherever it can, and its tests compare results with FastAPI 0.135.3 running under CPython. That keeps it closer to the original than a port to a different-looking language usually is. It is still a reimplementation: apart from what NOTICE.md lists, the code is written for Osprey's types, effects and runtime, and FastAPI is not bundled, linked or needed at run time.

**License.** FastAPI is MIT licensed. This package is MIT, Copyright NIMBLESITE PTY LTD, Australia; that grant covers Osprey's own code and does not relicense anything of FastAPI's. [NOTICE.md](NOTICE.md) lists any third-party material the package carries and the terms that come with it (copies in [LICENSES/](LICENSES/)).

**Thanks** to Sebastián Ramírez and the FastAPI contributors for [FastAPI](https://github.com/fastapi/fastapi), and to Encode OSS Ltd and the [Starlette](https://github.com/Kludex/starlette) contributors (BSD-3-Clause), whose CORS middleware `src/applications.py` adapts. Naming FastAPI credits it; it does not imply that its maintainers endorse this port.

## HTTP boundary [FASTAPI-HTTP]

`Applications.serve(port, host, app)` returns a fallible server handle. `Routing.route(method, path, handler)` stores a typed request handler; `include_router` composes immutable router lists with a prefix. `Applications.application(routes, middleware)` wraps routing in ordered request middleware. `Responses` constructs responses and translates them to the runtime ABI. Errors from listener creation propagate to the application.

Routing now supports ordered literal, single-segment and greedy `{path:path}` routes, percent-decoded Unicode, method selection, query-preserving redirects, 404/405 and custom responses. `Params` exposes repeated queries, scalar last-value selection and case-insensitive headers. `Validation` and `Uuids` return typed `Valid`/`Invalid` outcomes, combine errors and render 422 locations, inputs and typed contexts. `Errors` renders typed HTTP exceptions with status and headers. Domain handlers receive typed values after this boundary; CPython decorator execution and automatic Pydantic binding remain unfinished.

## Compatibility checklist

Checked items have real HTTP test coverage.

- [x] Pin the upstream FastAPI version and record reference responses.
- [x] Prove basic routing, prefix inclusion, custom responses, health/readiness distinction, redirects and method failures over real HTTP under GC and ARC. [FASTAPI-HTTP-ROUTING]
- [ ] Path parameters including `{path:path}`, percent decoding, ordered matching, query/header aliases, repeated values, integer/UUID validation and 422 error locations. [FASTAPI-HTTP-PARAMETERS]
- [ ] Body validation and response serialization from generated typeDiagram contracts: aliases, required/optional/null, nested models, strict extra-field rejection and response-model filtering. [FASTAPI-HTTP-VALIDATION]
- [ ] Dependency graphs, per-request caching, overrides and cleanup for database sessions, authenticated identities and authorization gates. Failures return typed errors. [FASTAPI-HTTP-DEPENDENCIES]
- [ ] HTTP exceptions and registered error handlers with exact status/body/headers; no error converted into a successful empty value. [FASTAPI-HTTP-ERRORS]
- [ ] Middleware ordering, trace IDs, authentication middleware failures, CORS preflight and error-path CORS. [FASTAPI-HTTP-MIDDLEWARE]
- [x] Lifespan startup and shutdown, startup refusal, and cleanup after bind or work failure. [FASTAPI-HTTP-LIFESPAN]
- [ ] Request cancellation and cleanup on disconnect. [FASTAPI-HTTP-LIFESPAN]
- [ ] Incremental SSE delivery, event framing, terminal events/errors, heartbeat comments, deadlines, cancellation and cleanup. Buffered concatenation does not qualify. [FASTAPI-HTTP-STREAMING]
- [ ] ACP Streamable HTTP GET/POST/DELETE and WebSocket upgrade, auth rejection, session lifecycle, tool results and cancellation. [FASTAPI-HTTP-ACP]
- [x] Raw binary uploads and responses with media type checks and byte limits. [FASTAPI-HTTP-FILES]
- [ ] Multipart uploads and webhook signature verification. [FASTAPI-HTTP-FILES]
- [ ] OpenAPI generation/customization, request/response schemas and documented security/error responses. [FASTAPI-HTTP-OPENAPI]
- [ ] Run the complete compatibility suite over real sockets under GC and ARC, including authorization denials, tool-result validation and streaming failures, with per-package coverage above the enforced floor. [FASTAPI-HTTP-ACCEPTANCE]

## Development

Generate records: `python3 src/osprey/packages/fastapi/generate_models.cpython`. The generator consumes `models.td` through the installed typeDiagram CLI and emits the Python flavor records in `src/models.py`.

Real HTTP acceptance: `python3 src/osprey/packages/fastapi/tests/e2e.cpython`. It compiles the actual Osprey sources and makes HTTP requests against the native listener, including parallel requests. Logs are in `target/fastapi/`.

The native GC and ARC suites pass the 98.5% package coverage gate with 824 of 836 executable source lines covered. The suites assert 331 routing/dependency records, 300 CORS records, 808 JSON body records and 72 raw upload permutations over real HTTP.

The native HTTP suite directly exercises application and router dispatch, raw and decoded query handling, repeated headers, path matching, explicit response helpers and validation issue rendering. The model generator keeps URL decoding state and dependency acquisition state private to the modules that own them.

Dependency cases cover shared and uncached acquisitions, overrides, nested denials, multiple validation errors with distinct locations/inputs, missing and malformed integer headers, and HTTP/validation failures during cleanup. The expanded corpus caught and now guards the compiler's nested pattern-alias shadowing bug, acceptance of `2.`, rejection of valid `1_000`, and incorrect handling of Latin-1 HTTP header bytes.

The committed reference records in `tests/expected.py` come from real Uvicorn listeners and include reference version and schema provenance. The oracle also writes `tests/parameters.headers.bin`, record-separated Latin-1 header bytes so the native client sends exactly the same bytes as CPython's client. The external suite verifies that fixture against every recorded header; it is data, not a second set of expectations. CPython development helpers use `.cpython` to distinguish them from the port's Osprey `.py` sources.

Known remaining parameter limits: implicit `Annotated`/decorator binding is not implemented, integers use the native 64-bit domain, and embedded NUL cannot traverse the current native string ABI. NUL path input produces an explicit 400 rather than truncation; this is a recorded runtime limitation, not complete CPython parity.

The HTTP drivers refuse `python -O`, retain diagnostic logs, and bound owned processes by runtime, memory and output limits. Native suites retry occupied loopback ports and verify listener recovery; GC/ARC acceptance runs concurrently. The external corpus also checks all eight non-GET methods, empty HEAD bodies, raw non-NUL binary payloads, CRLF content and larger echo bodies.

Run `make -C src/osprey/packages fastapi-benchmark` to compare health and bundle routing, valid identity-provider bodies and indexed 422 responses on GC, ARC and pinned CPython FastAPI. Every timed response is asserted. The benchmark writes per-trial results to `target/fastapi/benchmark/results.json`. See [FASTAPI-BENCHMARK-SUBSET](../../../../docs/specs/ecosystem/0043-PythonFlavorPorts.md).

Results from 5 November 2026 on an Apple-silicon Mac, over 18,603 verified responses (IDP is the identity-provider model-binding workload):

| Runtime | Health req/s | Bundle req/s | Valid IDP req/s | Invalid IDP req/s |
|---|---:|---:|---:|---:|
| osprey-gc | 3,302 | 3,185 | 2,479 | 2,662 |
| osprey-arc | 6,075 | 5,791 | 4,272 | 4,408 |
| cpython | 5,585 | 4,509 | 4,905 | 3,594 |

`Applications.with_exception_handler(app, status, handler)` registers an immutable handler for typed HTTP failures from routing, dependencies or endpoints. The latest registration for a status wins. Handlers receive the request and typed exception, and their response passes through the app middleware. Ordinary responses keep their original body and headers. `Applications.include_router` preserves registrations. Class-based uncaught exception and database error handling remain open.

`Applications.with_lifespan(port, host, startup, build, run, shutdown)` runs startup before binding. It passes startup's state to the application builder and to shutdown. When startup succeeds, the scope stops its listener and runs shutdown after work returns, including when work or binding fails. When both work and cleanup fail, the result preserves both errors in order. Real loopback tests cover startup refusal, state available to requests, occupied ports, work and shutdown failures, and socket closure before shutdown. Request cancellation and disconnect cleanup remain open. [FASTAPI-HTTP-LIFESPAN]

CORS middleware: `Applications.cors(Models.CorsPolicy(...))` supports exact/wildcard origins, method and header policies, credentials, exposed headers and max-age. The wildcard policy without credentials is exercised alongside restricted-origin and credentialed wildcard policies by 300 independently generated FastAPI/Starlette real HTTP records under GC and ARC, including Vary merging, denied preflights, case-insensitive request headers, empty/null origins, combined failures and 503/404/405 responses. The native coverage suite starts the actual listener and runs the same external HTTP assertions. Regex origins and allowed private-network preflights remain unsupported; requests for private-network access are explicitly rejected. Regenerating the existing FastAPI oracle also regenerates `CORS_JSON` in `tests/expected.py`; original 331 records are unchanged.

Body binding provides typed readers and typeDiagram-generated fixture records for `BundleFileWriteRequest`, `ApiKeyCreateRequest`, strict `EndUserExecutionContext`, aliased `EmbeddedResource` and `EndUserIdpUpsertRequest`. The IDP binding validates every `allowed_algs: list[str]` element and preserves numeric error indices, field order and each invalid input. Its 117 real HTTP cases include simultaneous field errors, multiple invalid elements, 64-element arrays, duplicate keys, ignored/private extras and Unicode. Both GC and ARC match all 117 cases, including the escaped NUL case. The provider-error envelope validates the nested detail object and preserves ordered errors at `body.detail.code` and `body.detail.message`. All 146 cases match the pinned 502 or 422 response on both memory managers. The model-list envelope validates each model at its numeric index and preserves field and item order. Its 90 cases include mixed failures, duplicate keys, filtered extras and 128-item lists. Arrays nested inside other arrays, full response validation and other untested model contracts are unfinished.

The explicit `jsonDecodePython` boundary decoder preserves first-key insertion order with final values, including nested objects and escaped key spellings, and accepts Python's NaN/infinity forms before model validation. Tests include 2,000 repeated keys, malformed duplicate input, non-finite values in ignored fields and non-finite values overwritten by later keys; they assert FastAPI's exact success or failure response. Ordinary `jsonDecode` retains its strict duplicate-key and finite-number rules, verified by the exact Default/ML JSON goldens under GC and ARC. The compatibility decoder preserves escaped NUL in keys and values. Native int64 and nesting limits still apply.

The body oracle records 808 cases from seven generated fixture models, with contract and schema hashes in `BODY_JSON`. GC and ARC match all responses, including the 68 escaped NUL cases. The HTTP server preserves literal NUL in binary bodies and rejects invalid JSON with the recorded status and offset, even after a complete JSON prefix. The native HTTP client preserves binary request and response bytes across Content-Length, chunked and close-delimited response framing. General UTF-16/UTF-32 body decoding remains open.

`Body.bind_binary(request, media_types, max_bytes, endpoint)` checks the first Content-Type against the configured exact tokens, ignoring parameters. It returns 415 for an unsupported type, 400 for an empty body, or 413 for a body over the byte limit. The endpoint receives the original bytes. The native listener's request limit also applies. [FASTAPI-HTTP-FILES]

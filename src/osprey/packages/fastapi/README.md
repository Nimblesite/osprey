# FastAPI in Python flavor Osprey

A port of FastAPI's public behavior, targeting the API use cases in NimblesiteAgenticPlatform (the local successor repository to NimblesiteAgenticFramework). This is implementation work in progress: it is not yet a replacement for that application's CPython server.

The reference is FastAPI 0.135.3, pinned in NAP's `uv.lock`. This port is MIT licensed by NIMBLESITE PTY LTD, Australia, and retains FastAPI's MIT notice, Copyright (c) 2018 Sebastián Ramírez; see [NOTICE.md](NOTICE.md). Compiled applications do not embed the CPython framework.

## HTTP boundary [FASTAPI-HTTP]

`Applications.serve(port, host, app)` returns a fallible server handle. `Routing.route(method, path, handler)` stores a typed request handler; `include_router` composes immutable router lists with a prefix. `Applications.application(routes, middleware)` wraps routing in ordered request middleware. `Responses` constructs responses and translates them to the runtime ABI. Errors from listener creation propagate to the application.

Routing now supports ordered literal, single-segment and greedy `{path:path}` routes, percent-decoded Unicode, method selection, query-preserving redirects, 404/405 and custom responses. `Params` exposes repeated queries, scalar last-value selection and case-insensitive headers. `Validation` and `Uuids` return typed `Valid`/`Invalid` outcomes, combine errors and render 422 locations, inputs and typed contexts. `Errors` renders typed HTTP exceptions with status and headers. Domain handlers receive typed values after this boundary; CPython decorator execution and automatic Pydantic binding remain unfinished.

## Required NAP parity [FASTAPI-NAP]

This checklist preserves the complete requested scope. An unchecked item is unfinished, including any item not yet proven by real transport tests.

- [x] Inspect current NAP imports, application assembly and route modules; pin the reference framework version.
- [x] Prove basic routing, prefix inclusion, custom responses, health/readiness distinction, redirects and method failures over real HTTP under GC and ARC. [FASTAPI-HTTP-ROUTING]
- [ ] Path parameters including `{path:path}`, percent decoding, ordered matching, query/header aliases, repeated values, integer/UUID validation and 422 error locations. [FASTAPI-NAP-PARAMETERS]
- [ ] Body validation and response serialization from generated typeDiagram contracts: aliases, required/optional/null, nested models, strict extra-field rejection and response-model filtering. [FASTAPI-NAP-VALIDATION]
- [ ] Dependency graphs, per-request caching, override support and cleanup: DB session, authenticated tenant/member/end-user identity, authorization and credit gates. Errors must be explicit typed values. [FASTAPI-NAP-DEPENDENCIES]
- [ ] HTTP exceptions and registered error handlers with exact status/body/headers; no error converted into a successful empty value. [FASTAPI-NAP-ERRORS]
- [ ] Middleware ordering, trace IDs, edge-secret failures, CORS preflight and error-path CORS. [FASTAPI-NAP-MIDDLEWARE]
- [ ] Lifespan startup/shutdown, startup failure, request cancellation and cleanup on disconnect. [FASTAPI-NAP-LIFESPAN]
- [ ] Incremental SSE delivery, event framing, terminal events/errors, heartbeat comments, deadlines, cancellation and cleanup. Buffered concatenation does not qualify. [FASTAPI-NAP-STREAMING]
- [ ] ACP Streamable HTTP GET/POST/DELETE and WebSocket upgrade, auth rejection, session lifecycle, tool results and cancellation. [FASTAPI-NAP-ACP]
- [ ] Multipart upload, binary tarball/file responses, size limits, content types, raw Stripe webhook bytes and signature header preservation. [FASTAPI-NAP-FILES]
- [ ] OpenAPI generation/customization, request/response schemas and documented security/error responses. [FASTAPI-NAP-OPENAPI]
- [ ] All NAP endpoint families: tenants/API keys/members, configs, conversations, ACP/member and end-user routes, workspaces/previews/tools/instances, bundles/files, providers, usage/credit/checkout/webhook, admin cost cards, invitations and end-user identity providers. [FASTAPI-NAP-ENDPOINTS]
- [ ] Translate and pass the existing NAP real-socket acceptance cases, including cross-tenant denials, mixed tool-result rejection and streaming failure diagnostics; then run both managed memory backends with per-package coverage above the enforced floor. [FASTAPI-NAP-ACCEPTANCE]

The source inventory is reproducible with `rg -n 'from fastapi|from starlette|@router\.|@app\.' ../NimblesiteAgenticPlatform/src/nap` from the parent of this repository. NAP's `tests/test_http_e2e.py`, `test_acp_rpc_e2e.py`, `test_acp_cancel_e2e.py` and `test_acp_permission_e2e.py` are transport acceptance references; ASGITransport-only tests must become real socket tests here.

## Development

Generate records: `python3 src/osprey/packages/fastapi/generate_models.cpython`. The generator consumes `models.td` through the installed typeDiagram CLI and emits the Python flavor records in `src/models.py`.

Real HTTP acceptance: `python3 src/osprey/packages/fastapi/tests/e2e.cpython`. It compiles the actual Osprey sources and makes HTTP requests against the native listener, including parallel requests. Logs are in `target/fastapi/`.

Verified own-source coverage is 498/508 executable lines (98.0%) under both GC and ARC; the gate ratchets to 98.0. This measures the implemented routing, validation and function-scope dependency layer, not the unfinished NAP parity checklist. The native suite replays all 331 independently recorded FastAPI 0.135.3 HTTP cases, checking exact status/body and applicable content/challenge headers; the external suite repeats the corpus three times per backend and also asserts byte lengths, parsed bodies, redirects, graph-configuration failures, request isolation and LIFO cleanup traces. ARC exits with zero live objects.

Dependency cases cover shared and uncached acquisitions, overrides, nested denials, multiple validation errors with distinct locations/inputs, missing and malformed integer headers, and HTTP/validation failures during cleanup. The expanded corpus caught and now guards the compiler's nested pattern-alias shadowing bug, acceptance of `2.`, rejection of valid `1_000`, and incorrect handling of Latin-1 HTTP header bytes.

Regenerate all reference records with `../NimblesiteAgenticPlatform/.venv/bin/python -I src/osprey/packages/testing/oracle/oracle.cpython fastapi > src/osprey/packages/fastapi/tests/parameters.expected.json`. The oracle starts real Uvicorn listeners and writes `tests/expected.py` through the shared provenance writer. It also derives `tests/parameters.headers.bin`: record-separated Latin-1 header bytes so the native client sends exactly the same bytes as CPython's client. The external suite verifies that fixture against every recorded header; it is data, not a second set of expectations. CPython development helpers use `.cpython` to distinguish them from the port's Osprey `.py` sources.

Known remaining parameter limits: implicit `Annotated`/decorator binding is not implemented, integers use the native 64-bit domain, and embedded NUL cannot traverse the current native string ABI. NUL path input produces an explicit 400 rather than truncation; this is a recorded runtime limitation, not complete CPython parity.

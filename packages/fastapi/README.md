# FastAPI in Python flavor Osprey

An independent port of FastAPI's public behavior, targeting the API use cases in NimblesiteAgenticPlatform (the local successor repository to NimblesiteAgenticFramework). This is implementation work in progress: it is not yet a replacement for that application's CPython server.

The reference is FastAPI 0.135.3, pinned in NAP's `uv.lock`. Source modules are original Osprey code under MIT from NIMBLESITE PTY LTD, Australia; see [NOTICE.md](NOTICE.md). Compiled applications do not embed the CPython framework.

## HTTP boundary [FASTAPI-HTTP]

`Applications.serve(port, host, routes)` returns a fallible server handle. `Routing.route(method, path, handler)` stores a typed request handler; `include_router` composes immutable router lists with a prefix. `Responses` constructs responses and translates them to the runtime ABI. Errors from listener creation propagate to the application.

The current route implementation provides exact paths, method selection, query-independent path lookup, trailing-slash redirects, 404 and 405 responses, and custom status/media type/headers. JSON text crosses only the response boundary; domain handlers should use typed records. It does not yet provide CPython decorator execution or automatic Pydantic binding.

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

Generate records: `python3 packages/fastapi/generate_models.py`. The generator consumes `models.td` through the installed typeDiagram CLI and emits the Python flavor records in `src/models.py`.

Real HTTP acceptance: `python3 packages/fastapi/tests/e2e.py`. It compiles the actual Osprey sources and makes HTTP requests against the native listener, including parallel requests. Logs are in `target/fastapi/`.

Current own-source measurement: 59/61 executable lines (96.7%) under both GC and ARC; the gate ratchets to 96.7. This measures the implemented HTTP layer, not the unfinished NAP parity checklist.

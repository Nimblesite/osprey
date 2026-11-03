# Plan 0053 — HTTP client response framing

Contract: [HTTP-CHUNKED-DECODE] and [HTTP-RESPONSE-FRAMING] in [0014 HTTP](../specs/runtime/0014-HTTP.md).

Out of scope: binary bodies, because bodies are NUL-terminated strings and one holding a NUL is refused; the shared handle counter `get_next_id`, which is never reused and caps a process at 100 clients before no server can start, and 1,000 before no client can.

## Done

- [x] Decode chunked bodies strictly, with every size checked before any copy: `http_decode_chunked` in `compiler/runtime/http_client_framing.c` (`8f96e8bb`).
- [x] Refuse what could pass framing off as a body: NUL bytes, bad status lines, cut or malformed heads, short or invalid `Content-Length`, control bytes in extensions and trailers. Skip interim `1xx`. `101`, `204` and `304` have no body (`8f96e8bb` and the commits up to `1e2ea064`).
- [x] Second adversarial review (`d7393587`): a failed or timed-out read, or TLS with no `close_notify`, no longer ends an unframed body; `EINTR` no longer ends a read; `chunked,` is chunked; any coding overrides `Content-Length`; a malformed interim head is refused.
- [x] Gate `http_client_framing` at 100% C coverage in `coverage-thresholds.json` (`d7393587`).
- [x] Attack coverage: `compiler/runtime/http_framing_tests.c`, `http_chunked_tests.c` and `http_tls_tests.c` at C level; the Osprey hostile server `tests/regressions/http/chunked_hostile_server.test.osp` and `.ospml` end to end. NUL bytes, interim responses and cut heads are C-only, because the Osprey server can't send them.
- [x] Mutation runs against the framing and transport code killed every mutant except `ERR_clear_error`, listed below. ASan and UBSan are clean; these runs are local, as ASan isn't in CI yet (Plan 0048, Cross-cutting).

## Verification still open

- [ ] Run the rust, Rust coverage, conformance (GC and ARC), C suite and fmt gates green on one commit. Check: all exit 0. The last rust run failed on two unused bindings in this work's tests, since renamed; osprey-cli coverage was last measured at 94.8% against a 95% threshold, on an earlier commit.
- [ ] Rerun the 24 live raw-socket attacks against the code after `d7393587`; they last ran before it. Check: each attack gets the answer its spec line gives.
- [x] Fetch from real HTTPS servers that omit `close_notify` and send no `Content-Length`. Check: the refusal in [HTTP-RESPONSE-FRAMING] is what a real unframed response gets, and framed responses from the same servers still succeed. Ran 2026-11-03: `www.craigslist.org/` (an unframed 302, no `close_notify`) is refused with `body: truncated`; its framed `/robots.txt` and `/about/`, and framed responses from `www.google.com`, `www.sqlite.org` and `www.openssl.org`, which omit `close_notify` too, succeed.

## Gaps still open

- [x] Limit the size of a response. `http_perform` (`compiler/runtime/http_client_runtime.c`) buffers until the peer closes, so a hostile server can exhaust the client's memory. Check: a test server sending past the limit gets a negative code and nothing leaks, and the spec states the limit.
- [x] Bound the whole read. The client's timeout applies to each `recv`, so a server sending one byte per timeout never ends. Check: a dripping test server is cut off at a stated total, and a slow honest response within it still succeeds.
- [x] Stop reading once the framing is complete. A complete chunked or `Content-Length` response from a server that holds the socket open currently waits out the timeout. Check: such a response returns well inside the timeout.
- [x] Treat `Transfer-Encoding` on an HTTP/1.0 response as faulty framing (RFC 9112 6.1); it is decoded today. Check: a test for each of chunked and gzip on `HTTP/1.0`, and a line in the spec.
- [x] Decide `ERR_clear_error` in `transport_recv`. OpenSSL's documentation requires it before `SSL_read`, but removing it survives every test, because `SSL_connect` clears the queue first. Check: a test that can fail without it, or a comment recording why none can.

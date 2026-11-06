//! The attack table for `http_client_raw_origin.rs`: one hostile reply per
//! row, and the answer [HTTP-CHUNKED-DECODE] and [HTTP-RESPONSE-FRAMING]
//! demand for it. It lives beside its parent test because the table is data,
//! not code, and the parent file has a size budget of its own. Every
//! expectation is pinned by the C twins (`http_framing_tests.c`,
//! `http_read_tests.c`) or the Osprey corpus
//! (`src/osprey/tests/regressions/http/chunked_hostile_server.test.osp`).

use super::{
    Attack, Body, Expected, BODY_BAD_LENGTH, BODY_TRUNCATED, CHUNK_BAD_SIZE, CHUNK_BAD_TRAILER,
    CHUNK_NO_CRLF, CHUNK_TOO_BIG, CHUNK_TRUNCATED, DEFAULT_TIMEOUT_MS, HEAD_MALFORMED, HEAD_NUL,
    HEAD_TE_10, HEAD_TRUNCATED, HOLD_OPEN_TIMEOUT_MS, MIB64, NOT_HTTP_1, READ_OVER_TIME,
    SIZE_OVER_LIMIT,
};

/// A reply the client must answer, with the default close and timeout.
const fn ok(name: &'static str, reply: &'static [u8], status: i32, body: &'static str) -> Attack {
    Attack {
        name,
        reply,
        fill_to: None,
        hold_open: false,
        drip_ms: 0,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        expect: Expected::Response(status, Body::Text(body)),
    }
}

/// A reply the client must refuse, with the default close and timeout.
const fn refused(name: &'static str, reply: &'static [u8], message: &'static str) -> Attack {
    Attack {
        name,
        reply,
        fill_to: None,
        hold_open: false,
        drip_ms: 0,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        expect: Expected::Refused(message),
    }
}

/// A framed answer the server holds the socket open after, on its own patient
/// client: complete framing must end the read without waiting for the close.
const fn held(name: &'static str, reply: &'static [u8], status: i32, body: &'static str) -> Attack {
    Attack {
        name,
        reply,
        fill_to: None,
        hold_open: true,
        drip_ms: 0,
        timeout_ms: HOLD_OPEN_TIMEOUT_MS,
        expect: Expected::Response(status, Body::Text(body)),
    }
}

/// A refusal the server holds the socket open after: the timeout alone ends it.
const fn held_refused(name: &'static str, reply: &'static [u8], message: &'static str) -> Attack {
    Attack {
        name,
        reply,
        fill_to: None,
        hold_open: true,
        drip_ms: 0,
        timeout_ms: DEFAULT_TIMEOUT_MS,
        expect: Expected::Refused(message),
    }
}

/// A dripped reply, one byte at a time, on its own client and timeout.
const fn slow(
    name: &'static str,
    reply: &'static [u8],
    timeout_ms: i32,
    drip_ms: u64,
    expect: Expected,
) -> Attack {
    Attack {
        name,
        reply,
        fill_to: None,
        hold_open: false,
        drip_ms,
        timeout_ms,
        expect,
    }
}

/// A reply padded with filler to a total size, on its own patient client —
/// the size-limit attacks.
const fn sized(
    name: &'static str,
    reply: &'static [u8],
    fill_to: usize,
    expect: Expected,
) -> Attack {
    Attack {
        name,
        reply,
        fill_to: Some(fill_to),
        hold_open: false,
        drip_ms: 0,
        timeout_ms: HOLD_OPEN_TIMEOUT_MS,
        expect,
    }
}

/// Every hostile reply, in the order the generated client fetches them.
pub(super) const ATTACKS: &[Attack] = &[
    // Chunked framing [HTTP-CHUNKED-DECODE]
    ok("chunked: valid", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n", 200, "hello"),
    ok("chunked: multi chunk", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n", 200, "hello world"),
    ok("chunked: extensions", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5;name=value\r\nhello\r\n0;last\r\n\r\n", 200, "hello"),
    ok("chunked: trailers", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nabc\r\n0\r\nX-Trailer: yes\r\n\r\n", 200, "abc"),
    ok("chunked: uppercase hex", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nA\r\n0123456789\r\n0\r\n\r\n", 200, "0123456789"),
    ok("chunked: trailing comma in the coding list", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked,\r\n\r\n2\r\nhi\r\n0\r\n\r\n", 200, "hi"),
    ok("chunked: gzip then chunked", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n2\r\nhi\r\n0\r\n\r\n", 200, "hi"),
    ok("chunked: repeated TE lines are one list", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n", 200, "hello"),
    refused("chunked: 7FFFFFFF beyond the body", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n7FFFFFFF\r\nhi\r\n0\r\n\r\n", CHUNK_TOO_BIG),
    refused("chunked: seventeen hex digits", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\nFFFFFFFFFFFFFFFFF\r\nhi\r\n0\r\n\r\n", CHUNK_BAD_SIZE),
    refused("chunked: negative size", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n-1\r\nhi\r\n0\r\n\r\n", CHUNK_BAD_SIZE),
    refused("chunked: 0x prefix", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0x5\r\nhello\r\n0\r\n\r\n", CHUNK_BAD_SIZE),
    refused("chunked: no size at all", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n\r\n0\r\n\r\n", CHUNK_BAD_SIZE),
    refused("chunked: data not followed by CRLF", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhelloXX0\r\n\r\n", CHUNK_NO_CRLF),
    refused("chunked: bare LF inside an extension", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5;a\nb\r\nhello\r\n0\r\n\r\n", CHUNK_BAD_SIZE),
    refused("chunked: control byte in a trailer", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX\x01: 1\r\n\r\n", CHUNK_BAD_TRAILER),
    refused("chunked: NUL in a size line", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\x00\r\nabc\r\n0\r\n\r\n", CHUNK_BAD_SIZE),
    refused("chunked: NUL in a trailer", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nX-A: \x00\r\n\r\n", CHUNK_BAD_TRAILER),
    ok("chunked: binary body", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n3\r\n\x00a\x00\r\n2\r\nb\x00\r\n0\r\n\r\n", 200, "\0a\0b\0"),
    refused("chunked: cut off before the terminator", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n", CHUNK_TRUNCATED),
    // The head [HTTP-RESPONSE-FRAMING]
    held("head: interim 100 then 200", b"HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok", 200, "ok"),
    held("head: interim 103 twice then 204", b"HTTP/1.1 103 a\r\n\r\nHTTP/1.1 103 b\r\n\r\nHTTP/1.1 204 No\r\n\r\n", 204, ""),
    refused("head: malformed interim", b"HTTP/1.1 100 X\n\nHTTP/1.1 200 OK\r\n\r\nHTTP/1.1 404 Forged\r\n\r\nno", HEAD_MALFORMED),
    refused("head: truncated", b"HTTP/1.1 200 OK\r\nX: y\r\n\r", HEAD_TRUNCATED),
    refused("head: bare LF never ends the head", b"HTTP/1.1 200 OK\nTransfer-Encoding: chunked\n\n7FFFFFFF\nhi\n", HEAD_TRUNCATED),
    refused("head: bare LF in a header value", b"HTTP/1.1 200 OK\r\nX: a\nTransfer-Encoding: chunked\r\n\r\n7FFFFFFF\r\nhi\r\n", HEAD_MALFORMED),
    refused("head: folded line", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip,\r\n chunked\r\n\r\n2\r\nhi\r\n0\r\n\r\n", HEAD_MALFORMED),
    refused("head: line with no colon", b"HTTP/1.1 200 OK\r\nno colon\r\n\r\nb", HEAD_MALFORMED),
    refused("head: space before the colon", b"HTTP/1.1 200 OK\r\nTransfer-Encoding : chunked\r\n\r\n7FFFFFFF\r\nhi\r\n", HEAD_MALFORMED),
    refused("head: NUL in a header value", b"HTTP/1.1 200 OK\r\nX-A: \x00\r\n\r\nhello", HEAD_NUL),
    ok("close: binary body", b"HTTP/1.1 200 OK\r\n\r\n\x00he\x00lo\x00", 200, "\0he\0lo\0"),
    refused("status: HTTP/2", b"HTTP/2 200\r\n\r\nb", NOT_HTTP_1),
    refused("status: two digits", b"HTTP/1.1 20 OK\r\n\r\nb", NOT_HTTP_1),
    refused("status: four digits", b"HTTP/1.1 2000 OK\r\n\r\nb", NOT_HTTP_1),
    // Content-Length framing [HTTP-RESPONSE-FRAMING]
    ok("length: exact", b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello", 200, "hello"),
    ok("length: binary body", b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhe\x00lo", 200, "he\0lo"),
    ok("length: extra bytes dropped", b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nhello", 200, "he"),
    refused("length: short and closed", b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\n\r\nshort", BODY_TRUNCATED),
    refused("length: junk", b"HTTP/1.1 200 OK\r\nContent-Length: x\r\n\r\nhello", BODY_BAD_LENGTH),
    refused("length: negative", b"HTTP/1.1 200 OK\r\nContent-Length: -1\r\n\r\nshort", BODY_BAD_LENGTH),
    refused("length: two that disagree", b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 2\r\n\r\nhello", BODY_BAD_LENGTH),
    ok("length: two that agree", b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Length: 5\r\n\r\nhello", 200, "hello"),
    ok("length: a coding overrides the length", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip\r\nContent-Length: 2\r\n\r\nhello", 200, "hello"),
    // Statuses whose body ends at the blank line (RFC 9112 6.3), held open.
    held("no body: 204 despite Content-Length", b"HTTP/1.1 204 No Content\r\nContent-Length: 9\r\n\r\n", 204, ""),
    held("no body: 304 despite chunked", b"HTTP/1.1 304 Not Modified\r\nTransfer-Encoding: chunked\r\n\r\n", 304, ""),
    held("no body: 101 despite Content-Length", b"HTTP/1.1 101 Switching\r\nUpgrade: websocket\r\nContent-Length: 5\r\n\r\nraw", 101, ""),
    // The close-delimited body
    ok("close: unframed body", b"HTTP/1.1 200 OK\r\n\r\nok", 200, "ok"),
    held_refused("close: none comes, refused at the timeout", b"HTTP/1.1 200 OK\r\n\r\nhalf", BODY_TRUNCATED),
    // Transfer-Encoding on HTTP/1.0 is faulty framing (RFC 9112 6.1)
    refused("HTTP/1.0: Transfer-Encoding chunked", b"HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n", HEAD_TE_10),
    refused("HTTP/1.0: Transfer-Encoding gzip", b"HTTP/1.0 200 OK\r\nTransfer-Encoding: gzip\r\n\r\nok", HEAD_TE_10),
    refused("HTTP/1.0: TE with Content-Length", b"HTTP/1.0 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Length: 2\r\n\r\nok", HEAD_TE_10),
    refused("HTTP/1.0: 204 with Transfer-Encoding", b"HTTP/1.0 204 No Content\r\nTransfer-Encoding: chunked\r\n\r\n", HEAD_TE_10),
    ok("HTTP/1.0: plain body by the close", b"HTTP/1.0 200 OK\r\n\r\nok", 200, "ok"),
    ok("HTTP/1.0: Content-Length honored", b"HTTP/1.0 200 OK\r\nContent-Length: 2\r\n\r\nokEXTRA", 200, "ok"),
    // Complete framing ends the read: the held socket costs nothing.
    held("stops: content-length", b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello", 200, "hello"),
    held("stops: the chunked terminator", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n", 200, "ok"),
    held("stops: trailers then extra bytes", b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\n\r\n2;a=b\r\nok\r\n0;c\r\nX-T: 1\r\nX-U: 2\r\n\r\nEXTRA", 200, "ok"),
    held("stops: NUL past the framing ignored", b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok\x00NUL past the framing", 200, "ok"),
    // The size limit: 64 MiB heads and framing included.
    sized("size: exactly 64 MiB by the close", b"HTTP/1.1 200 OK\r\n\r\n", MIB64, Expected::Response(200, Body::Len(MIB64 - 19))),
    sized("size: one byte past 64 MiB", b"HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n", MIB64 + 1, Expected::Refused(SIZE_OVER_LIMIT)),
    sized("size: endless flood of a header", b"HTTP/1.1 200 OK\r\nX-Endless: ", MIB64 + 8 * 1024 * 1024, Expected::Refused(SIZE_OVER_LIMIT)),
    // The whole-read bound: ten times the timeout.
    slow("time: drip cut off at ten timeouts", b"HTTP/1.1 200 OK\r\nX-Long: xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", 100, 40, Expected::Refused(READ_OVER_TIME)),
    slow("time: slow but honest drip", b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok", 200, 40, Expected::Response(200, Body::Text("ok"))),
];

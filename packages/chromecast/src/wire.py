# osprey: flavor=python
# osprey: namespace=chromecast
# osprey: module=Wire
"""Every byte the sender exchanges with the outside world, as one effect.

`Session`, `Cast` and `Player` never call the runtime directly: they
`perform` a `Wire` operation. `Native` answers each operation with the C
runtime ([FFI-TLS-CLIENT], [FFI-FILE-SERVER]), so a program runs its casting
code under `Native(work)`. A test runs the same code under its own handler
instead, which can play a whole Cast device in memory: it sees every frame
the sender writes and decides every byte the sender reads.
"""

__all__ = ["Wire", "Native"]


@extern
def osprey_tls_connect(host: str, port: int, timeout_ms: int) -> int:
    ...


@extern
def osprey_tls_send_hex(handle: int, hex: str) -> int:
    ...


@extern
def osprey_tls_recv_hex(handle: int, count: int, timeout_ms: int) -> str:
    ...


@extern
def osprey_tls_status(handle: int) -> int:
    ...


@extern
def osprey_tls_close(handle: int) -> int:
    ...


@extern
def osprey_net_local_address(host: str, port: int) -> str:
    ...


@extern
def osprey_file_server_start(path: str, content_type: str, address: str) -> int:
    ...


@extern
def osprey_file_server_stop(port: int) -> int:
    ...


@effect
class Wire:
    """The sender's I/O. `dial` returns a session handle of at least 1, or a
    negative reason; `write` sends hex bytes and returns 0 or a negative code;
    `read` returns exactly `count` bytes as hex, or "" when they did not
    arrive within `timeout_ms`; `status` says why the last read or write on
    a handle ended (0 ok, 1 timed out, 2 closed, 3 failed, -1 unknown);
    `close` ends a session. `route` is the local address that reaches a host,
    "" if none; `serve` serves a file at `/media` on that address and returns
    its port or a negative reason; `unserve` stops it (0, or -1)."""
    def dial(host: str, port: int, timeout_ms: int) -> int: ...
    def write(handle: int, hex: str) -> int: ...
    def read(handle: int, count: int, timeout_ms: int) -> str: ...
    def status(handle: int) -> int: ...
    def close(handle: int) -> int: ...
    def route(host: str) -> str: ...
    def serve(path: str, content_type: str, address: str) -> int: ...
    def unserve(port: int) -> int: ...


@handler(Wire)
class Native:
    """Runs `work` against real devices: TLS sessions, the local routing table
    and the runtime's one-file HTTP server."""
    def dial(host, port, timeout_ms):
        return osprey_tls_connect(host, port, timeout_ms)

    def write(handle, hex):
        return osprey_tls_send_hex(handle, hex)

    def read(handle, count, timeout_ms):
        return osprey_tls_recv_hex(handle, count, timeout_ms)

    def status(handle):
        return osprey_tls_status(handle)

    def close(handle):
        return osprey_tls_close(handle)

    def route(host):
        return osprey_net_local_address(host, 9)

    def serve(path, content_type, address):
        return osprey_file_server_start(path, content_type, address)

    def unserve(port):
        return osprey_file_server_stop(port)

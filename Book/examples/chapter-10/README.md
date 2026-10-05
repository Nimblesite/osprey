# Chapter 10 examples

Run commands from the repository root. The examples use Default flavor and the current value-effect syntax.

## Offline checks and native file round trip

`flight-log.osp` compares scripted storage handlers with a real native file handler. It writes or replaces `flight-log.txt` in its working directory. Run it only in a fresh temporary directory:

```sh
book_root="$PWD"
"$book_root/target/release/osprey" "$book_root/Book/examples/chapter-10/flight-log.osp" --check
(
    book_tmp=$(mktemp -d)
    trap 'rm -r "$book_tmp"' EXIT
    cd "$book_tmp"
    "$book_root/target/release/osprey" "$book_root/Book/examples/chapter-10/flight-log.osp" --run
)
```

The exact output is in `flight-log.expectedoutput`. The saved file is exactly the 24-byte ASCII text `Build a small Osprey CLI`, with no trailing newline. The fixture handler checks the write payload but returns its scripted read value; it is not an implementation of mutable storage.

`web-title.osp` makes no network requests. Its handlers deterministically supply a title, an HTTP-status error, and a transport error. Run it with `target/release/osprey Book/examples/chapter-10/web-title.osp --run`; compare with `web-title.expectedoutput`.

The compiler rejection in `failscompilation/undecoded-result.osp` passes a file-reading `Result` to a function requiring a string. Its golden diagnostic uses paths relative to the `Book` directory, matching the book harness.

## Optional real native HTTP adapter

`native/http-title.osp` fetches a local title with the response-handle HTTP API. This native example is separate from the offline golden suite because it opens a socket. It does not contact a public service.

In one terminal, serve the fixture from the repository root:

```sh
python3 -m http.server 18872 --bind 127.0.0.1 --directory Book/examples/chapter-10/native/fixtures
```

In a second terminal, from the same root:

```sh
target/release/osprey Book/examples/chapter-10/native/http-title.osp --check
target/release/osprey Book/examples/chapter-10/native/http-title.osp --run
```

The output matches `native/http-title.expectedoutput`: `Success(Build a small Osprey CLI)`. Stop the fixture server with Ctrl-C afterwards. If port 18872 is occupied, stop your own fixture server or choose another port in both commands and the source; do not stop an unrelated process.

The adapter has been exercised against a loopback server returning HTTP 200 and HTTP 404, and with no listener. A 404 becomes `Error(HTTP 404)`. A connection failure remains an `Error`; the runtime may provide a generic message. The adapter frees each successfully acquired response handle and closes the client after either request outcome. Its cleanup-error policy reports a cleanup failure if cleanup itself fails.

Native HTTP builtins are unavailable on WebAssembly and mobile C ABI targets. Mobile applications send commands to their native host and receive completion events. File availability on WebAssembly depends on the WASI host's permissions.

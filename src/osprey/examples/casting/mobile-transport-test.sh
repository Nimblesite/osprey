#!/usr/bin/env bash
# [CAST-MOBILE-DELIVERY] Live TLS receiver test of the native transport sources.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
platform=${1:-ios}
work=$(mktemp -d)
server_pid=
remote=
mapped_ports=()
cleanup() {
    local status=$?
    for at in ${mapped_ports[@]+"${mapped_ports[@]}"}; do "$adb" -s "$serial" reverse --remove "tcp:$at" || { [[ $status -ne 0 ]] || status=1; }; done
    if [[ -n "$remote" ]]; then "$adb" -s "$serial" shell rm "$remote" || { [[ $status -ne 0 ]] || status=1; }; fi
    if [[ -n "$server_pid" ]]; then kill "$server_pid" 2>/dev/null || true; wait "$server_pid" 2>/dev/null || true; fi
    python3 - "$work" <<'CLEAN'
import shutil,sys
shutil.rmtree(sys.argv[1])
CLEAN
    exit "$status"
}
trap cleanup EXIT
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$work/key.pem" -out "$work/cert.pem" -subj /CN=localhost -days 1 >/dev/null 2>&1
cat > "$work/server.py" <<'PYTHON'
import socket,ssl,time,sys,threading
ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.load_cert_chain(sys.argv[1] + '/cert.pem',sys.argv[1] + '/key.pem')
refused=socket.socket();refused.bind(('127.0.0.1',0))
open(sys.argv[1] + '/refused-port','w').write(str(refused.getsockname()[1]))
plain=socket.socket();plain.bind(('127.0.0.1',0));plain.listen(1)
open(sys.argv[1] + '/plain-port','w').write(str(plain.getsockname()[1]))
def reject_tls():
 with plain.accept()[0] as client:
  client.settimeout(3);client.recv(65536)
  client.sendall(b'HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n')
  try:
   while client.recv(65536): pass
  except OSError: pass
threading.Thread(target=reject_tls,daemon=True).start()
with socket.socket() as server:
 server.bind(('127.0.0.1',0));server.listen(1)
 open(sys.argv[1] + '/port','w').write(str(server.getsockname()[1]))
 with ctx.wrap_socket(server.accept()[0],server_side=True) as client:
  client.sendall(bytes([1,2]));time.sleep(.2);client.sendall(bytes([3,4]))
  received=b''
  while len(received)<2:
   part=client.recv(2-len(received))
   assert part
   received+=part
  assert received==bytes([5,6])
  client.settimeout(5)
  try: client.unwrap().close()  # Send TLS close_notify so EOF is distinct from truncation.
  except (ssl.SSLError, TimeoutError): pass  # Some clients cancel rather than reciprocate.
PYTHON
cat > "$work/main.swift" <<'SWIFT'
import Foundation
func require(_ ok: Bool, _ message: String) throws {
    if !ok { throw NSError(domain: message, code: 1) }
}
do {
    let port = Int64(CommandLine.arguments[1]) ?? 0
    try require("".withCString { castConnect($0, port, 100) } == -1, "empty host is an invalid argument")
    try require(castConnect(nil, port, 100) == -1, "null host")
    for (badPort, timeout) in [(0, 100), (65536, 100), (Int(port), 0), (Int(port), 60001)] {
        try require("127.0.0.1".withCString { castConnect($0, Int64(badPort), Int64(timeout)) } == -1, "invalid port or timeout")
    }
    let refused = Int64(CommandLine.arguments[2]) ?? 0
    try require("127.0.0.1".withCString { castConnect($0, refused, 1000) } == -4, "refused TCP connection is not a TLS handshake failure")
    let plain = Int64(CommandLine.arguments[3]) ?? 0
    try require("127.0.0.1".withCString { castConnect($0, plain, 1000) } == -7, "unencrypted peer is a TLS handshake failure")
    let handle = "127.0.0.1".withCString { castConnect($0, port, 3000) }
    try require(handle == 1, "TLS handshake after rejected connections without leaked slots")
    try require(castStatus(999) == -1, "unknown handle")
    let partial = castRead(handle, 4, 10).map { String(cString: $0) }
    try require(partial == "" && castStatus(handle) == 1, "partial read timeout")
    let whole = castRead(handle, 4, 3000).map { String(cString: $0) }
    try require(whole == "01020304", "partial bytes retained across timeout")
    try require("0506".withCString { castWrite(handle, $0) } == 0, "write")
    _ = castRead(handle, 1, 3000)
    try require(castStatus(handle) == 2, "peer EOF")
    try require(castClose(handle) == 0 && castClose(handle) == -1, "close and duplicate close")
    // [CAST-MOBILE-ROUTE] The shared HTTP media server binds IPv4, including dual-stack names.
    for host in ["127.0.0.1", "localhost"] {
        let route = host.withCString { castRoute($0, 8009).map { String(cString: $0) } }
        try require(route == "127.0.0.1", "IPv4 media-server route for \(host): \(route ?? "nil")")
    }
    print("PASS: connection refusal, TLS rejection, handshake, partial-read timeout, retained bytes, write, EOF, cleanup and route")
} catch { fputs("FAIL: \(error)\n", stderr); exit(1) }
SWIFT
# Probe the production C TLS runtime on the device, never the retired Java transport.
cat > "$work/check.c" <<'C'
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <netinet/in.h>
#include <sys/socket.h>
#include <unistd.h>
#include "memory_hooks.h"
int64_t osprey_tls_connect(char *, int64_t, int64_t);
int64_t osprey_tls_send_hex(int64_t, char *);
char *osprey_tls_recv_hex(int64_t, int64_t, int64_t);
int64_t osprey_tls_status(int64_t);
int64_t osprey_tls_close(int64_t);
char *osprey_net_local_address(char *, int64_t);
#define REQUIRE(test, message) do { if (!(test)) { fprintf(stderr, "FAIL: %s\n", message); return 1; } } while (0)
int main(int argc, char **argv) {
    REQUIRE(argc == 3, "TLS and plaintext fixture ports required");
    osp_mem_boot();
    int64_t port = strtoll(argv[1], NULL, 10);
    REQUIRE(osprey_tls_connect("", port, 100) == -1 && osprey_tls_connect(NULL, port, 100) == -1, "empty or null host");
    REQUIRE(osprey_tls_connect("127.0.0.1", 0, 100) == -1, "zero port");
    REQUIRE(osprey_tls_connect("127.0.0.1", 65536, 100) == -1, "oversize port");
    REQUIRE(osprey_tls_connect("127.0.0.1", port, 0) == -1, "zero timeout");
    REQUIRE(osprey_tls_connect("127.0.0.1", port, 60001) == -1, "oversize timeout");
    // Reserve a device port without listening: adb reverse would accept TCP itself.
    int refused = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in address = {.sin_family = AF_INET, .sin_addr.s_addr = htonl(INADDR_LOOPBACK)};
    REQUIRE(refused >= 0 && bind(refused, (struct sockaddr *)&address, sizeof address) == 0, "reserve refused device port");
    socklen_t length = sizeof address;
    REQUIRE(getsockname(refused, (struct sockaddr *)&address, &length) == 0, "read refused device port");
    REQUIRE(osprey_tls_connect("127.0.0.1", ntohs(address.sin_port), 1000) == -4, "refused TCP is distinct from TLS failure");
    close(refused);
    REQUIRE(osprey_tls_connect("127.0.0.1", strtoll(argv[2], NULL, 10), 1000) == -7, "unencrypted peer rejected");
    int64_t handle = osprey_tls_connect("127.0.0.1", port, 3000);
    REQUIRE(handle == 1, "handshake after rejected connections without leaked slots");
    REQUIRE(osprey_tls_status(999) == -1, "unknown handle");
    char *partial = osprey_tls_recv_hex(handle, 4, 10);
    REQUIRE(partial && !strcmp(partial, "") && osprey_tls_status(handle) == 1, "partial read timeout");
    osp_release(partial);
    char *whole = osprey_tls_recv_hex(handle, 4, 3000);
    REQUIRE(whole && !strcmp(whole, "01020304"), "partial bytes retained across timeout");
    osp_release(whole);
    REQUIRE(osprey_tls_send_hex(handle, "0506") == 0, "write");
    char *closed = osprey_tls_recv_hex(handle, 1, 3000);
    REQUIRE(closed && !strcmp(closed, "") && osprey_tls_status(handle) == 2, "peer EOF");
    osp_release(closed);
    REQUIRE(osprey_tls_close(handle) == 0 && osprey_tls_close(handle) == -1, "close and duplicate close");
    REQUIRE(osprey_tls_connect("", 0, 0) == -1, "invalid connection");
    char *route = osprey_net_local_address("127.0.0.1", 8009);
    REQUIRE(route && !strcmp(route, "127.0.0.1"), "IPv4 loopback route");
    osp_release(route);
    puts("PASS: Android production C TLS handshake, failures, retained reads, write, EOF, cleanup and route");
    return 0;
}
C
repo=$(cd "$here/../../../.." && pwd)

# [CAST-MOBILE-DELIVERY] The capability and bounded-read boundaries are probed
# inside the simulator as standalone executables: nothing is installed, no app
# is launched, and no picker or UI is presented. The read probe is sliced out of
# the shipped PlatformServices.swift by declaration marker, so it exercises the
# code that ships rather than a paraphrase.
write_capability_probe() {
    mkdir -p "$work/capability"
    cat > "$work/capability/main.swift" <<'SWIFT'
import Foundation
import UIKit
var failures = 0
func expect(_ name: String, _ condition: Bool, _ detail: String) {
    if condition {
        print("ok   \(name)")
    } else {
        failures += 1
        print("FAIL \(name): \(detail)")
    }
}
func ask(_ request: String) -> (answer: String, seconds: TimeInterval) {
    var answer = "<none>"
    let done = DispatchSemaphore(value: 0)
    let started = Date()
    Thread.detachNewThread {
        if let raw = osprey_gpui_capability(request) {
            answer = String(cString: raw)
            osprey_gpui_capability_free(raw)
        }
        done.signal()
    }
    while done.wait(timeout: .now() + 0.02) == .timedOut {
        _ = RunLoop.current.run(mode: .default, before: Date().addingTimeInterval(0.02))
    }
    return (answer, Date().timeIntervalSince(started))
}
func members(_ answer: String) -> [String: Any] {
    ((try? JSONSerialization.jsonObject(with: Data(answer.utf8))) as? [String: Any]) ?? [:]
}
func refused(_ name: String, _ request: String, _ wanted: String, within: TimeInterval) {
    let (answer, seconds) = ask(request)
    let empty = members(answer).isEmpty
    expect(name, !empty && members(answer)["ok"] as? Bool == false, "answer \(answer)")
    let problem = members(answer)["error"] as? String ?? ""
    expect("\(name) names the problem", problem.contains(wanted), "error \(problem)")
    expect("\(name) answers before any service starts", seconds < within, "took \(seconds)s")
}
func accepted(_ name: String, _ request: String, slice key: String, within: TimeInterval) {
    let (answer, seconds) = ask(request)
    expect(name, members(answer)["ok"] as? Bool == true, "answer \(answer)")
    let slices = members(answer)["slices"] as? [[String: Any]] ?? []
    expect("\(name) answers one \(key) slice", slices.count == 1 && slices.first?["key"] as? String == key,
           "slices \(slices)")
    expect("\(name) finishes inside the wait", seconds < within, "took \(seconds)s")
}
for (name, limit) in [("boolean", "true"), ("negative", "-1"), ("fractional", "1.5"),
                      ("null", "null"), ("text", "\"2\""), ("beyond 9e15", "10000000000000000")] {
    refused("choose-file limit \(name)",
            "{\"kind\":\"choose-file\",\"types\":[\"mp3\"],\"limit\":\(limit),\"slice\":\"\"}",
            "The request's limit is not a whole number", within: 1)
}
refused("choose-file types shape",
        "{\"kind\":\"choose-file\",\"types\":5,\"limit\":0}",
        "The request's types is not a list of extensions", within: 1)
refused("choose-file types with a number",
        "{\"kind\":\"choose-file\",\"types\":[\"mp3\",2],\"limit\":0}",
        "The request's types is not a list of extensions", within: 1)
refused("choose-file slice shape",
        "{\"kind\":\"choose-file\",\"types\":[\"mp3\"],\"limit\":0,\"slice\":5}",
        "The request's slice is not text", within: 1)
refused("choose-file without any type",
        "{\"kind\":\"choose-file\",\"types\":[],\"limit\":0}",
        "No supported file types", within: 1)
refused("choose-file with an unknown type",
        "{\"kind\":\"choose-file\",\"types\":[\"not-a-real-extension\"],\"limit\":0}",
        "No foreground window for the file picker", within: 1)
refused("run timeout boolean",
        "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\",\"timeout\":true}",
        "The request's timeout is not a whole number of milliseconds", within: 1)
refused("run timeout text",
        "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\",\"timeout\":\"5000\"}",
        "The request's timeout is not a whole number of milliseconds", within: 1)
refused("run timeout negative",
        "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\",\"timeout\":-5}",
        "The request's timeout is not a whole number of milliseconds", within: 1)
refused("run timeout zero",
        "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\",\"timeout\":0}",
        "The requested 0 ms wait is outside 1...60000 ms", within: 1)
refused("run timeout above the ceiling",
        "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\",\"timeout\":60001}",
        "The requested 60001 ms wait is outside 1...60000 ms", within: 1)
refused("run ticket shape",
        "{\"kind\":\"run\",\"ticket\":5,\"slice\":\"cast-browse\",\"timeout\":5000}",
        "The request's ticket is not text", within: 1)
refused("run slice shape",
        "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":7,\"timeout\":5000}",
        "The request's slice is not text", within: 1)
refused("run that is not the discover ticket",
        "{\"kind\":\"run\",\"ticket\":\"reveal\",\"slice\":\"cast-browse\",\"timeout\":5000}",
        "Mobile casting does not execute processes", within: 1)
refused("unknown capability kind", "{\"kind\":\"reveal\",\"path\":\"/tmp\"}",
        "unsupported capability reveal", within: 1)
refused("request that is not an object", "[]", "The capability request is not a JSON object", within: 1)
refused("request that is not JSON", "nope", "The capability request is not a JSON object", within: 1)
refused("request without a kind", "{}", "The capability request names no kind", within: 1)
accepted("discover with no timeout",
         "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\"}",
         slice: "cast-browse", within: 5)
accepted("discover with the app's 5000 ms timeout",
         "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\",\"timeout\":5000}",
         slice: "cast-browse", within: 5)
accepted("discover at the 60000 ms ceiling",
         "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\",\"timeout\":60000}",
         slice: "cast-browse", within: 5)
refused("discover shorter than discovery's deadline",
        "{\"kind\":\"run\",\"ticket\":\"discover\",\"slice\":\"cast-browse\",\"timeout\":1000}",
        "Timed out after 1000 ms", within: 3)
func noticeRequest(_ change: [String: Any]) -> String {
    var base: [String: Any] = ["kind": "host-call", "namespace": "ui", "operation": "notice", "arguments": ["title": "Notice", "message": "Test"], "timeout": 1, "limit": 64, "slice": ""]
    base.merge(change) { _, new in new }
    return String(decoding: (try? JSONSerialization.data(withJSONObject: base, options: [.sortedKeys])) ?? Data(), as: UTF8.self)
}
let noticeCases: [(String, [String: Any], String)] = [
    ("namespace unknown", ["namespace": "cast"], "unsupported host-call namespace cast"),
    ("namespace nontext", ["namespace": 5], "The request's namespace is not text"),
    ("operation unknown", ["operation": "alert"], "unsupported host-call operation alert"),
    ("operation nontext", ["operation": 5], "The request's operation is not text"),
    ("arguments non-object", ["arguments": 5], "The request's arguments is not an object"),
    ("title empty", ["arguments": ["title": "", "message": "Test"]], "The notice title must be 1...200 characters"),
    ("title nontext", ["arguments": ["title": 5, "message": "Test"] as [String: Any]], "The request's title is not text"),
    ("title over the scalar limit", ["arguments": ["title": String(repeating: "🎬", count: 201), "message": "Test"]], "The notice title must be 1...200 characters"),
    ("message empty", ["arguments": ["title": "Notice", "message": ""]], "The notice message must be 1...2000 characters"),
    ("message nontext", ["arguments": ["title": "Notice", "message": 5] as [String: Any]], "The request's message is not text"),
    ("message over the scalar limit", ["arguments": ["title": "Notice", "message": String(repeating: "🎬", count: 2001)]], "The notice message must be 1...2000 characters"),
    ("title combining scalars", ["arguments": ["title": String(repeating: "e\u{301}", count: 101), "message": "Test"]], "The notice title must be 1...200 characters"),
    ("message combining scalars", ["arguments": ["title": "Notice", "message": String(repeating: "e\u{301}", count: 1001)]], "The notice message must be 1...2000 characters"),
    ("timeout zero", ["timeout": 0], "The requested 0 ms wait is outside 1...600000 ms"),
    ("timeout negative", ["timeout": -1], "The request's timeout is not a whole number"),
    ("timeout fractional", ["timeout": 1.5], "The request's timeout is not a whole number"),
    ("timeout boolean", ["timeout": true], "The request's timeout is not a whole number"),
    ("timeout text", ["timeout": "1"], "The request's timeout is not a whole number"),
    ("timeout null", ["timeout": NSNull()], "The request's timeout is not a whole number"),
    ("timeout above the ceiling", ["timeout": 600001], "The requested 600001 ms wait is outside 1...600000 ms"),
    ("limit zero", ["limit": 0], "The notice limit must be at least 2"),
    ("limit one", ["limit": 1], "The notice limit must be at least 2"),
    ("limit boolean", ["limit": true], "The request's limit is not a whole number"),
    ("limit text", ["limit": "64"], "The request's limit is not a whole number"),
    ("slice nonempty", ["slice": "cast"], "The notice writes no slice"),
    ("slice nontext", ["slice": 0], "The request's slice is not text"),
]
for (name, change, wanted) in noticeCases {
    refused("host-call notice \(name)", noticeRequest(change), wanted, within: 1)
}
expect("a nil request answers nil", osprey_gpui_capability(nil) == nil, "expected nil")
let onMain = osprey_gpui_capability("{\"kind\":\"run\"}")
let mainAnswer = onMain.map { String(cString: $0) } ?? "<none>"
if let onMain { osprey_gpui_capability_free(onMain) }
expect("the main thread is refused",
       (members(mainAnswer)["error"] as? String) == "Capabilities cannot be answered on the main thread",
       "answer \(mainAnswer)")

print(failures == 0 ? "ALL PASSED" : "\(failures) FAILED")
exit(failures == 0 ? 0 : 1)
SWIFT
}

write_read_probe() {
    mkdir -p "$work/read"
    cat > "$work/read/harness.swift" <<'SWIFT'
import Foundation
var failures = 0

func expect(_ name: String, _ condition: Bool, _ detail: String) {
    if condition {
        print("ok   \(name)")
    } else {
        failures += 1
        print("FAIL \(name): \(detail)")
    }
}

private func problem(_ result: Result<String, PlatformFailure>) -> String? {
    switch result {
    case let .failure(.detail(problem)): return problem
    case .success: return nil
    }
}

func accepted(_ name: String, _ path: String, limit: Int, equals wanted: String) {
    switch readText(path, limit: limit) {
    case let .success(text):
        expect(name, text == wanted, "read \(text.count) chars, wanted \(wanted.count)")
    case let .failure(.detail(problem)):
        expect(name, false, "refused: \(problem)")
    }
}

func refused(_ name: String, _ path: String, limit: Int, naming wanted: String) {
    switch readText(path, limit: limit) {
    case let .success(text):
        expect(name, false, "accepted \(text.count) chars instead of refusing")
    case let .failure(.detail(problem)):
        expect(name, problem.contains(wanted), "problem \(problem)")
    }
}

let folder = URL(fileURLWithPath: NSTemporaryDirectory())
    .appendingPathComponent("cast-read-probe-\(UUID().uuidString)", isDirectory: true)
try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
defer { try? FileManager.default.removeItem(at: folder) }

func written(_ name: String, _ bytes: [UInt8]) -> String {
    let url = folder.appendingPathComponent(name)
    FileManager.default.createFile(atPath: url.path, contents: Data(bytes))
    return url.path
}

// Absent paths, directories, links and special files are typed refusals.
refused("missing file", folder.appendingPathComponent("absent").path, limit: 64,
        naming: "Cannot read the chosen file")
refused("directory", folder.path, limit: 64, naming: "Cannot read the chosen file")
let regular = written("regular.txt", Array("hello".utf8))
let link = folder.appendingPathComponent("link.txt").path
try FileManager.default.createSymbolicLink(atPath: link, withDestinationPath: regular)
refused("symlink", link, limit: 64, naming: "Cannot read the chosen file")
let fifo = folder.appendingPathComponent("pipe").path
mkfifo(fifo, 0o600)
// A FIFO would block a plain open+read forever; the regular-file check refuses it.
refused("fifo", fifo, limit: 64, naming: "Cannot read the chosen file")

// Bounded reads: empty, exact, one byte over, and a multi-chunk file.
let empty = written("empty.txt", [])
accepted("empty file", empty, limit: 10, equals: "")
let exact = written("exact.txt", Array(repeating: UInt8(ascii: "a"), count: 32))
accepted("file exactly at the limit", exact, limit: 32, equals: String(repeating: "a", count: 32))
let over = written("over.txt", Array(repeating: UInt8(ascii: "b"), count: 33))
refused("file one byte over the limit", over, limit: 32,
        naming: "33 bytes exceeds the 32 byte limit")
let big = written("big.txt", Array(repeating: UInt8(ascii: "c"), count: 1_000_000))
// The reported size is exactly limit + 1, matching the host's own bounded read,
// and only that many bytes are ever read from a much larger file.
refused("file far over the limit", big, limit: 1000,
        naming: "1001 bytes exceeds the 1000 byte limit")
expect("file far over the limit reports no more than limit + 1",
       problem(readText(big, limit: 1000)) == "1001 bytes exceeds the 1000 byte limit", "size")
accepted("file read across chunks", big, limit: 1_000_000,
         equals: String(repeating: "c", count: 1_000_000))
refused("limit zero on a nonempty file", exact, limit: 0,
        naming: "1 bytes exceeds the 0 byte limit")
accepted("limit zero on an empty file is empty text", empty, limit: 0, equals: "")

// UTF-8: complete multibyte text is accepted, a cut character is refused.
let unicode = written("unicode.txt", Array("«Découverte» — 中文キャスト 🎬\n".utf8))
accepted("unicode text", unicode, limit: 4096,
         equals: "«Découverte» — 中文キャスト 🎬\n")
// A file that ends mid-character is refused as text even though its size fits.
refused("character cut at the end of the file", written("cut.txt", [0x61, 0xc3]), limit: 2,
        naming: "Not UTF-8 text")
refused("binary bytes", written("binary.bin", [0x80, 0x81]), limit: 8, naming: "Not UTF-8 text")
accepted("text with a NUL byte is still text", written("nul.txt", [0x61, 0x00, 0x62]),
         limit: 8, equals: "a\u{0}b")

print(failures == 0 ? "ALL PASSED" : "\(failures) FAILED")
exit(failures == 0 ? 0 : 1)
SWIFT
    python3 - "$here/ios/PlatformServices.swift" "$work/read/harness.swift" <<'PYTHON'
import pathlib, sys
source, harness = (pathlib.Path(path) for path in sys.argv[1:])
text = source.read_text()
failure = "/// One typed platform failure at the document-import boundary"
discover = "/// The `discover` run answered by native Bonjour discovery"
bounded = "/// The largest block one bounded read asks for"
shipped = text[text.index(failure):text.index(discover)] + "\n" + text[text.index(bounded):]
harness.write_text(shipped + "\n" + harness.read_text())
PYTHON
    mv "$work/read/harness.swift" "$work/read/main.swift"
}

build_ios_probe() {
    local dir=$1; shift
    xcrun --sdk iphonesimulator swiftc -swift-version 5 -warnings-as-errors \
        -target arm64-apple-ios18.0-simulator \
        -sdk "$(xcrun --sdk iphonesimulator --show-sdk-path)" "$@" "$dir/main.swift" -o "$dir/probe"
}

run_ios_probe() {
    local log="$work/$1.log" status=0
    xcrun simctl spawn "$device" "$2" > "$log" 2>&1 || status=$?
    cat "$log"
    [[ $status -eq 0 ]] || { echo "FAIL: $1 probe exited $status" >&2; return 1; }
    grep -qx 'ALL PASSED' "$log" || { echo "FAIL: $1 probe did not report ALL PASSED" >&2; return 1; }
}

ios_boundary_probes() {
    device=$(bash "$repo/scripts/mobile/ios-simulator.sh")
    write_capability_probe
    build_ios_probe "$work/capability" \
        "$here/ios/CastTLS.swift" "$here/ios/CastDiscovery.swift" "$here/ios/PlatformServices.swift"
    run_ios_probe capability "$work/capability/probe"
    write_read_probe
    build_ios_probe "$work/read"
    run_ios_probe bounded-read "$work/read/probe"
}
case "$platform" in
    ios) xcrun swiftc -swift-version 5 -warnings-as-errors "$here/ios/CastTLS.swift" "$work/main.swift" -o "$work/check" ;;
    android)
        source "$repo/scripts/mobile/android-env.sh"
        adb="$ANDROID_HOME/platform-tools/adb"
        serial=${OSPREY_ANDROID_SERIAL:?Set OSPREY_ANDROID_SERIAL to the device under test}
        abi=$("$adb" -s "$serial" shell getprop ro.product.cpu.abi | tr -d '\r')
        case "$abi" in
            arm64-v8a) target=android-arm64; triple=aarch64-linux-android26 ;;
            x86_64) target=android-x64; triple=x86_64-linux-android26 ;;
            *) echo "Unsupported Android ABI: $abi" >&2; exit 2 ;;
        esac
        openssl="$here/build/$target/openssl"
        runtime="$repo/src/toolchain/compiler/bin/libosprey_runtime_${target//-/_}_arc.a"
        "$android_tools/clang" --target="$triple" -std=c11 -Wall -Wextra -Werror \
            -fstack-protector-strong -D_FORTIFY_SOURCE=2 -O2 \
            -I"$repo/src/toolchain/compiler/runtime" -I"$openssl/include" \
            "$work/check.c" "$repo/src/toolchain/compiler/runtime/tls_client_runtime.c" \
            "$openssl/lib/libssl.a" "$openssl/lib/libcrypto.a" "$runtime" \
            -lm -ldl -llog -o "$work/check" ;;
    *) echo 'Usage: mobile-transport-test.sh [ios|android]' >&2; exit 2 ;;
esac
python3 "$work/server.py" "$work" &
server_pid=$!
for ((attempt=0; attempt<100; attempt++)); do
    [[ -f "$work/port" ]] && break
    sleep 0.05
done
[[ -f "$work/port" ]] || { echo 'TLS fixture did not start' >&2; exit 1; }
if [[ "$platform" == ios ]]; then "$work/check" "$(cat "$work/port")" "$(cat "$work/refused-port")" "$(cat "$work/plain-port")"
else
    remote="/data/local/tmp/osprey-casting-transport-$$"
    "$adb" -s "$serial" push "$work/check" "$remote" >/dev/null
    "$adb" -s "$serial" shell chmod 700 "$remote"
    status=0
    for name in port plain-port; do
        at=$(cat "$work/$name")
        "$adb" -s "$serial" reverse "tcp:$at" "tcp:$at"
        mapped_ports+=("$at")
    done
    "$adb" -s "$serial" shell "$remote" "$(cat "$work/port")" "$(cat "$work/plain-port")" || status=$?
    [[ $status -eq 0 ]] || exit "$status"
fi
wait "$server_pid"
server_pid=
if [[ "$platform" == ios ]]; then
    sdk=$(xcrun --sdk iphonesimulator --show-sdk-path)
    xcrun swiftc -swift-version 6 -warnings-as-errors -typecheck -target arm64-apple-ios18.0-simulator -sdk "$sdk" "$here/ios/CastTLS.swift" "$here/ios/CastDiscovery.swift"
    ios_boundary_probes
fi

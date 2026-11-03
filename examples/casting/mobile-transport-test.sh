#!/usr/bin/env bash
# [CAST-MOBILE-DELIVERY] Live TLS receiver test of the native transport sources.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
platform=${1:-ios}
work=$(mktemp -d)
server_pid=
cleanup() {
    if [[ -n "$server_pid" ]]; then kill "$server_pid" 2>/dev/null || true; fi
    python3 - "$work" <<'CLEAN'
import shutil,sys
shutil.rmtree(sys.argv[1])
CLEAN
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
    let route = "127.0.0.1".withCString { castRoute($0, 8009).map { String(cString: $0) } }
    try require(route == "127.0.0.1", "local route")
    print("PASS: connection refusal, TLS rejection, handshake, partial-read timeout, retained bytes, write, EOF, cleanup and route")
} catch { fputs("FAIL: \(error)\n", stderr); exit(1) }
SWIFT
cat > "$work/Check.java" <<'JAVA'
import org.ospreylang.casting.CastTLS;
public final class Check {
    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
    public static void main(String[] args) {
        long port = Long.parseLong(args[0]);
        require(CastTLS.connect("", port, 100) == -1 && CastTLS.connect(null, port, 100) == -1, "empty or null host");
        for (long[] input : new long[][]{{0, 100}, {65536, 100}, {port, 0}, {port, 60001}})
            require(CastTLS.connect("127.0.0.1", input[0], input[1]) == -1, "invalid port or timeout");
        require(CastTLS.connect("127.0.0.1", Long.parseLong(args[1]), 1000) == -4, "refused TCP connection is not a TLS handshake failure");
        require(CastTLS.connect("127.0.0.1", Long.parseLong(args[2]), 1000) == -7, "unencrypted peer is a TLS handshake failure");
        long handle = CastTLS.connect("127.0.0.1", Long.parseLong(args[0]), 3000);
        require(handle == 1, "TLS handshake after rejected connections without leaked slots");
        require(CastTLS.status(999) == -1, "unknown handle");
        require(CastTLS.read(handle, 4, 10).isEmpty() && CastTLS.status(handle) == 1, "partial timeout");
        require(CastTLS.read(handle, 4, 3000).equals("01020304"), "retained bytes");
        require(CastTLS.write(handle, "0506") == 0, "write");
        require(CastTLS.read(handle, 1, 3000).isEmpty() && CastTLS.status(handle) == 2, "peer EOF");
        require(CastTLS.close(handle) == 0 && CastTLS.close(handle) == -1, "cleanup");
        require(CastTLS.connect("", 0, 0) == -1, "invalid connection");
        require(!CastTLS.route("127.0.0.1", 8009).isEmpty(), "local route");
        System.out.println("PASS: Android connection refusal, TLS rejection, handshake, timeout recovery, bytes, write, EOF, cleanup and route");
    }
}
JAVA
case "$platform" in
    ios) xcrun swiftc -swift-version 5 -warnings-as-errors "$here/ios/CastTLS.swift" "$work/main.swift" -o "$work/check" ;;
    android)
        platform_jar=$(python3 - "${ANDROID_HOME:-$HOME/Library/Android/sdk}/platforms" <<'JAR'
import pathlib,sys
paths=list(pathlib.Path(sys.argv[1]).glob('*/android.jar'))
if not paths: sys.exit('An Android SDK platform is required')
print(paths[0])
JAR
)
        javac -Xlint:all -Werror -classpath "$platform_jar" -d "$work" "$here/android/src/main/java/org/ospreylang/casting/CastTLS.java" "$work/Check.java" ;;
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
else java -cp "$work" Check "$(cat "$work/port")" "$(cat "$work/refused-port")" "$(cat "$work/plain-port")"; fi
wait "$server_pid"
server_pid=
if [[ "$platform" == ios ]]; then
    sdk=$(xcrun --sdk iphonesimulator --show-sdk-path)
    xcrun swiftc -swift-version 6 -warnings-as-errors -typecheck -target arm64-apple-ios18.0-simulator -sdk "$sdk" "$here/ios/CastTLS.swift" "$here/ios/CastDiscovery.swift"
fi

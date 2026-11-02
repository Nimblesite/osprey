#!/usr/bin/env bash
# Builds Casting.app and makes it the default MP3 application: Finder's "Open" and double-click then
# launch it, and a running Casting receives the file as `open:<path>`.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
app=$("$here/run.sh" --bundle | tail -1)
swift_file=$(mktemp).swift
cat > "$swift_file" <<'SWIFT'
import AppKit
import UniformTypeIdentifiers
let done = DispatchSemaphore(value: 0)
NSWorkspace.shared.setDefaultApplication(at: URL(fileURLWithPath: CommandLine.arguments[1]), toOpen: .mp3) { error in
    print(error.map { "could not claim MP3: \($0)" } ?? "Casting opens MP3 files")
    done.signal()
}
_ = done.wait(timeout: .now() + 30)
SWIFT
swift "$swift_file" "$app"
rm -f "$swift_file"

echo "$app"

#!/usr/bin/env bash
set -euo pipefail

# Bundle the prebuilt GPUI host as a macOS MP3 player.
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
app="$here/build/Casting.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$repo/hosts/gpui/target/release/osprey-gpui" "$app/Contents/MacOS/osprey-gpui"

# Bake absolute paths into the executable launcher.
cat > "$app/Contents/MacOS/Casting" <<EOF
#!/usr/bin/env bash
set -euo pipefail
cd "$repo"
export OSPREY_GPUI_APP_NAME=casting
export OSPREY_GPUI_TITLE=Casting
export OSPREY_GPUI_CHROME=bare
export OSPREY_GPUI_WINDOW=434x557
export OSPREY_GPUI_LIGHTS=9,8
exec "$app/Contents/MacOS/osprey-gpui" "\$@" --machine "$here/player.machine.json"
EOF
chmod +x "$app/Contents/MacOS/Casting"

# Render every standard iconset size, then compile the icns file.
iconset=$(mktemp -d)/Casting.iconset
mkdir -p "$iconset"
icon() { sips -z "$1" "$1" "$here/designs/logo-icon.png" --out "$iconset/icon_$2.png" >/dev/null; }
icon 16 16x16
icon 32 16x16@2x
icon 32 32x32
icon 64 32x32@2x
icon 128 128x128
icon 256 128x128@2x
icon 256 256x256
icon 512 256x256@2x
icon 512 512x512
icon 1024 512x512@2x
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Casting.icns"

cat > "$app/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Casting</string>
    <key>CFBundleDisplayName</key><string>Casting</string>
    <key>CFBundleIdentifier</key><string>dev.ospreylang.casting</string>
    <key>CFBundleExecutable</key><string>Casting</string>
    <key>CFBundleIconFile</key><string>Casting</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>CFBundleDocumentTypes</key>
    <array>
        <dict>
            <key>CFBundleTypeName</key><string>MP3 audio</string>
            <key>CFBundleTypeRole</key><string>Viewer</string>
            <key>LSHandlerRank</key><string>Owner</string>
            <key>LSItemContentTypes</key>
            <array><string>public.mp3</string></array>
        </dict>
    </array>
</dict>
</plist>
EOF

# Refresh Launch Services and make Casting the default MP3 application: Finder's "Open" and
# double-click then launch it, and a running Casting receives the file as `open:<path>`.
lsregister=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
"$lsregister" -f "$app"
swift_file=$(mktemp).swift
cat > "$swift_file" <<'EOF'
import AppKit
import UniformTypeIdentifiers
let done = DispatchSemaphore(value: 0)
NSWorkspace.shared.setDefaultApplication(at: URL(fileURLWithPath: CommandLine.arguments[1]), toOpen: .mp3) { error in
    print(error.map { "could not claim MP3: \($0)" } ?? "Casting opens MP3 files")
    done.signal()
}
_ = done.wait(timeout: .now() + 30)
EOF
swift "$swift_file" "$app"
rm -f "$swift_file"

echo "$app"

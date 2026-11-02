#!/usr/bin/env bash
# Bundles the built GPUI host as a macOS application with its own name and Dock icon.
#   bundle.sh <out-dir> <Name> <bundle id> <icon.png> <machine.json> [plist fragment]
# Every OSPREY_GPUI_* variable set now (title, window, chrome, app name) is baked into the launcher.
# The optional fragment is inserted into Info.plist's top dictionary, for document types.
# Prints the bundle's path.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
out=$1 name=$2 id=$3 icon=$4 machine=$5 extra=${6:-}
app="$out/$name.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$here/target/release/osprey-gpui" "$app/Contents/MacOS/osprey-gpui"

{
    echo '#!/usr/bin/env bash'
    echo 'set -euo pipefail'
    echo "cd \"$repo\""
    env | grep '^OSPREY_GPUI_' | grep -v '^OSPREY_GPUI_APP_DIR=' | sed 's/^\([^=]*\)=\(.*\)$/export \1="\2"/'
    echo "exec \"$app/Contents/MacOS/osprey-gpui\" \"\$@\" --machine \"$machine\""
} > "$app/Contents/MacOS/$name"
chmod +x "$app/Contents/MacOS/$name"

# Render every standard iconset size, then compile the icns file.
iconset=$(mktemp -d)/"$name".iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$icon" --out "$iconset/icon_${size}x${size}.png" >/dev/null
    sips -z $((size * 2)) $((size * 2)) "$icon" --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/$name.icns"

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>$name</string>
    <key>CFBundleDisplayName</key><string>$name</string>
    <key>CFBundleIdentifier</key><string>$id</string>
    <key>CFBundleExecutable</key><string>$name</string>
    <key>CFBundleIconFile</key><string>$name</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>1.0</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
$( [ -n "$extra" ] && cat "$extra" )
</dict>
</plist>
EOF

/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
echo "$app"

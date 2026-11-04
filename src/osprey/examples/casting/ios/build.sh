#!/usr/bin/env bash
# [CAST-MOBILE-GPUI] Package the shared desktop GPUI host with iOS platform services.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
target=${1:-ios-sim}
case "$target" in
    ios-sim) sdk=iphonesimulator; triple=arm64-apple-ios18.0-simulator; rust_target=aarch64-apple-ios-sim ;;
    ios) sdk=iphoneos; triple=arm64-apple-ios18.0; rust_target=aarch64-apple-ios ;;
    *) echo 'Usage: build.sh [ios|ios-sim]' >&2; exit 2 ;;
esac
cd "$repo"
export IPHONEOS_DEPLOYMENT_TARGET=18.0
if [[ ${OSPREY_MOBILE_SKIP_LIBRARY:-0} != 1 ]]; then bash src/osprey/examples/casting/mobile-build.sh "$target"; fi
output="$repo/examples/casting/build/$target"
bundle="$output/Casting.app"
mkdir -p "$bundle"
sdk_path=$(xcrun --sdk "$sdk" --show-sdk-path)
for unit in file_server_runtime media_render_runtime; do
    xcrun --sdk "$sdk" clang -target "$triple" -isysroot "$sdk_path" -std=c11 \
        -Wall -Wextra -Werror -fstack-protector-strong -D_FORTIFY_SOURCE=2 \
        -c "src/toolchain/compiler/runtime/$unit.c" -o "$output/$unit.o"
done
xcrun --sdk "$sdk" swiftc -swift-version 5 -warnings-as-errors -parse-as-library \
    -target "$triple" -sdk "$sdk_path" -emit-library -static -module-name OspreyCastServices \
    "$here/CastTLS.swift" "$here/CastDiscovery.swift" "$here/PlatformServices.swift" \
    "$output/file_server_runtime.o" "$output/media_render_runtime.o" \
    -o "$output/libosprey_cast_services.a"
swift_search=$(xcrun --sdk "$sdk" swiftc -print-target-info -target "$triple" -sdk "$sdk_path" | \
    python3 -c 'import json,sys; print(":".join(json.load(sys.stdin)["paths"]["runtimeLibraryPaths"]))')
python3 src/toolchain/hosts/gpui/dependencies.py
OSPREY_GPUI_APP_DIR="$output" OSPREY_GPUI_LINK_SEARCH="$output:$swift_search" \
OSPREY_GPUI_LINK_LIBS="static=osprey_cast_services,framework=Network,framework=Security,framework=Foundation" \
    cargo build --manifest-path src/toolchain/hosts/gpui/Cargo.toml --target "$rust_target" --locked
cp "$repo/hosts/gpui/target/$rust_target/debug/osprey-gpui" "$bundle/Casting"
python3 - "$bundle/osprey-arguments.json" <<'ARGS'
import json,sys
with open(sys.argv[1], 'w') as file:
    json.dump(["--machine", "${RESOURCES}/player.machine.json"], file)
ARGS
cp src/osprey/examples/casting/player.machine.json "$bundle/player.machine.json"
rm -rf "$bundle/skins"
cp -R src/osprey/examples/casting/skins "$bundle/"
catalog="$output/Assets.xcassets"
xcrun swift -warnings-as-errors "$here/tools/Icon.swift" src/osprey/examples/casting/designs/logo-icon.png "$catalog"
xcrun --sdk "$sdk" actool "$catalog" --compile "$bundle" --platform "$sdk" \
    --minimum-deployment-target 18.0 --target-device iphone --target-device ipad \
    --app-icon AppIcon --output-partial-info-plist "$output/icons.plist" > "$output/icons.log"
python3 - "$bundle/Info.plist" "$output/icons.plist" <<'PY'
import plistlib,sys
with open(sys.argv[2], 'rb') as file:
    icons = plistlib.load(file)
assert icons['CFBundleIcons']['CFBundlePrimaryIcon']['CFBundleIconName'] == 'AppIcon', icons
with open(sys.argv[1], 'wb') as file:
    plistlib.dump(dict(CFBundleIdentifier='org.ospreylang.casting', CFBundleName='Casting',
        CFBundleExecutable='Casting', CFBundlePackageType='APPL', CFBundleVersion='1',
        CFBundleShortVersionString='1.0', MinimumOSVersion='18.0', LSRequiresIPhoneOS=True,
        UILaunchScreen={}, UIRequiresFullScreen=True,
        UIApplicationSceneManifest={'UIApplicationSupportsMultipleScenes': False, 'UISceneConfigurations': {
            'UIWindowSceneSessionRoleApplication': [{'UISceneConfigurationName': 'Default Configuration',
                'UISceneDelegateClassName': 'GPUIIosSceneDelegate'}]}},
        UIFileSharingEnabled=True, LSSupportsOpeningDocumentsInPlace=True, NSLocalNetworkUsageDescription='Find your Chromecast or Google TV and play your selected audio.',
        NSBonjourServices=['_googlecast._tcp'], UISupportedInterfaceOrientations=['UIInterfaceOrientationPortrait'],
        UIDeviceFamily=[1,2], **icons), file)
PY
if [[ "$target" == ios-sim ]]; then codesign --force --sign - "$bundle"; fi
echo "$bundle"

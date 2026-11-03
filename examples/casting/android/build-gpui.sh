#!/usr/bin/env bash
# [CAST-MOBILE-GPUI] Package the shared desktop GPUI host as an Android APK: no Java views,
# the same hosts/gpui view code, and the same TLS and media runtime as desktop.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../../.." && pwd)
target=${1:-android-arm64}
case "$target" in
    android-arm64) rust_target=aarch64-linux-android; abi=arm64-v8a; openssl_target=android-arm64; clang_target=aarch64-linux-android26 ;;
    android-x64) rust_target=x86_64-linux-android; abi=x86_64; openssl_target=android-x86_64; clang_target=x86_64-linux-android26 ;;
    *) echo 'Usage: build-gpui.sh [android-arm64|android-x64]' >&2; exit 2 ;;
esac
sdk=${ANDROID_HOME:-$HOME/Library/Android/sdk}
ndk=$sdk/ndk/27.0.12077973
tools=$ndk/toolchains/llvm/prebuilt/darwin-x86_64/bin
build_tools=$(ls -d "$sdk"/build-tools/* | sort -V | tail -1)
output="$repo/examples/casting/build/$target"
cd "$repo"
if [[ ${OSPREY_MOBILE_SKIP_LIBRARY:-0} != 1 ]]; then bash examples/casting/mobile-build.sh "$target"; fi
mkdir -p "$output"

# OpenSSL, pinned, for the same TLS client desktop links.
openssl_version=3.4.1
openssl_sha256=002a2d6b30b58bf4bea46c43bdd96365aaf8daa6c428782aa4feee06da197df3
openssl="$output/openssl"
if [[ ! -f $openssl/lib/libssl.a ]]; then
    source_archive="$output/openssl-$openssl_version.tar.gz"
    curl -sSfL -o "$source_archive" "https://github.com/openssl/openssl/releases/download/openssl-$openssl_version/openssl-$openssl_version.tar.gz"
    echo "$openssl_sha256  $source_archive" | shasum -a 256 -c - >/dev/null
    tar -xzf "$source_archive" -C "$output"
    (cd "$output/openssl-$openssl_version" && ANDROID_NDK_ROOT=$ndk PATH="$tools:$PATH" ./Configure "$openssl_target" -D__ANDROID_API__=26 \
        no-shared no-tests no-docs --prefix="$openssl" >/dev/null && PATH="$tools:$PATH" make -j10 build_sw >/dev/null && PATH="$tools:$PATH" make install_sw >/dev/null)
fi

for unit in tls_client file_server media_render; do
    "$tools/clang" --target="$clang_target" -fPIC -O2 -std=c11 -D_FORTIFY_SOURCE=2 -fstack-protector-strong -Wall -Wextra -Werror \
        -Icompiler/runtime -I"$openssl/include" -c "compiler/runtime/${unit}_runtime.c" -o "$output/${unit}_runtime.o"
done
rm -f "$output/libosprey_cast_net.a"
"$tools/llvm-ar" rcs "$output/libosprey_cast_net.a" "$output/tls_client_runtime.o" "$output/file_server_runtime.o" "$output/media_render_runtime.o"

python3 hosts/gpui/dependencies.py
linker_variable=CARGO_TARGET_$(echo "$rust_target" | tr 'a-z-' 'A-Z_')_LINKER
env "$linker_variable=$tools/$clang_target-clang" OSPREY_GPUI_APP_DIR="$output" \
    OSPREY_GPUI_LINK_SEARCH="$output:$openssl/lib" OSPREY_GPUI_LINK_LIBS="static=osprey_cast_net,static=ssl,static=crypto" \
    cargo build --manifest-path hosts/gpui/Cargo.toml --target "$rust_target" --lib --release --locked

apk="$output/apk"
rm -rf "$apk"
mkdir -p "$apk/lib/$abi" "$apk/assets"
"$tools/llvm-strip" --strip-unneeded "hosts/gpui/target/$rust_target/release/libosprey_gpui.so" -o "$apk/lib/$abi/libosprey_gpui.so"
cp examples/casting/player.machine.json "$apk/assets/"
cp -R examples/casting/skins "$apk/assets/"
echo '["--machine", "${RESOURCES}/player.machine.json"]' > "$apk/assets/osprey-arguments.json"
(cd "$apk/assets" && find . -type f ! -name osprey-files.txt | sed 's|^\./||' | sort > osprey-files.txt)
"$build_tools/aapt2" link -o "$apk/base.apk" --manifest "$here/gpui/AndroidManifest.xml" -I "$(ls -d "$sdk"/platforms/android-* | sort -V | tail -1)/android.jar" -A "$apk/assets"
(cd "$apk" && zip -q -0 base.apk "lib/$abi/libosprey_gpui.so")
"$build_tools/zipalign" -f -P 16 4 "$apk/base.apk" "$apk/aligned.apk"
"$build_tools/apksigner" sign --ks "$HOME/.android/debug.keystore" --ks-pass pass:android --key-pass pass:android --out "$output/Casting.apk" "$apk/aligned.apk"
echo "$output/Casting.apk"

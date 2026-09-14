#!/usr/bin/env bash
# Shared SDK/NDK discovery for the runtime, JNI and APK builds. [ANDROID-TARGET-LINK]
set -euo pipefail
android_sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}
if [[ -z "$android_sdk" ]]; then
    if [[ $(uname -s) == Darwin ]]; then android_sdk="$HOME/Library/Android/sdk"; else android_sdk="$HOME/Android/Sdk"; fi
fi
android_ndk=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}
if [[ -z "$android_ndk" ]]; then
    android_ndk=$(python3 - "$android_sdk/ndk" <<'PY'
import pathlib, sys
root = pathlib.Path(sys.argv[1])
versions = [p for p in root.iterdir() if p.is_dir()] if root.is_dir() else []
if not versions: sys.exit('Android NDK missing; install it in Android Studio or set ANDROID_NDK_HOME')
print(max(versions, key=lambda p: tuple(int(v) for v in p.name.split('.'))))
PY
)
fi
case $(uname -s) in
    Darwin) android_host=darwin-x86_64 ;;
    Linux) android_host=linux-x86_64 ;;
    *) echo 'Android builds currently require macOS or Linux' >&2; exit 1 ;;
esac
android_tools="$android_ndk/toolchains/llvm/prebuilt/$android_host/bin"
[[ -x "$android_tools/clang" ]] || { echo "Android NDK clang missing: $android_tools" >&2; exit 1; }
export ANDROID_HOME="$android_sdk" ANDROID_NDK_HOME="$android_ndk"

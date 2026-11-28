#!/usr/bin/env bash
# Lays the split repositories' main branches over this checkout, each file at the path it had before
# the split [REPO-SPLIT-CANARY]. Those paths are gitignored here, so the laid files never show as
# changes. Only CI tests read them: the canaries, the compiler-side tests that build their programs
# and the checks that the package manager still builds on each platform.
# Usage: scripts/ci/downstream.sh [name...]; no name lays every repository. A repository already
# checked out under target/downstream/<name> (CI checks out its main) is used as it is; otherwise
# main is cloned, or fetched and reset when the clone exists.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
[ "$#" -gt 0 ] || set -- osprey-packages perception
for name in "$@"; do
    dir="$root/target/downstream/$name"
    if [ ! -e "$dir/.git" ]; then
        git clone --quiet --depth 1 --branch main "https://github.com/Nimblesite/$name.git" "$dir"
    elif [ -z "${CI:-}" ]; then
        git -C "$dir" fetch --quiet --depth 1 origin main
        git -C "$dir" reset --quiet --hard FETCH_HEAD
    fi
    paths=()
    # A Windows checkout gives `paths` CRLF line ends: read them as LF.
    while IFS= read -r path; do path=${path%$'\r'}; [ -n "$path" ] && paths+=("$path"); done < "$dir/paths"
    for path in "${paths[@]}"; do
        if [ -n "$(git -C "$root" ls-files -- "$path")" ]; then
            echo "downstream: $path is tracked by ospreylang; $name cannot own it" >&2
            exit 1
        fi
        rm -rf "${root:?}/${path:?}"
    done
    files=$(git -C "$dir" ls-files -- "${paths[@]}" | wc -l | tr -d " ")
    [ "$files" -gt 0 ] || { echo "downstream: $name lays no files; its paths match nothing" >&2; exit 1; }
    git -C "$dir" ls-files -z -- "${paths[@]}" | tar -C "$dir" --null -T - -cf - | tar -C "$root" -xf -
    echo "downstream: $name at main $(git -C "$dir" rev-parse --short HEAD), $files files"
done

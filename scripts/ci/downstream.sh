#!/usr/bin/env bash
# Lays the split repositories ospreylang pins in scripts/ci/downstream.json over this checkout, each
# file at the path it had before the split [REPO-SPLIT-CANARY]. Those paths are gitignored here, so the
# laid files never show as changes. The compiler-side tests, the package client every build ships and
# each repository's `make canary` then run against the exact pinned commit.
# Usage: scripts/ci/downstream.sh [name...]; no name lays every repository. A repository already
# checked out under target/downstream/<name> (CI does that with its read-only deploy key) is reused;
# otherwise it is cloned over HTTPS with your GitHub credentials.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
pins="$root/scripts/ci/downstream.json"
[ "$#" -gt 0 ] || set -- osprey-packages perception
for name in "$@"; do
    commit=$(node -e 'const pins = require(process.argv[1]); const c = pins[process.argv[2]]; if (!c) process.exit(1); console.log(c)' "$pins" "$name") \
        || { echo "downstream: $name is not pinned in $pins" >&2; exit 1; }
    dir="$root/target/downstream/$name"
    if [ ! -e "$dir/.git" ]; then
        git clone --quiet --no-checkout "https://github.com/Nimblesite/$name.git" "$dir"
    fi
    git -C "$dir" cat-file -e "$commit^{commit}" 2>/dev/null || git -C "$dir" fetch --quiet origin "$commit"
    git -C "$dir" -c advice.detachedHead=false checkout --quiet --force "$commit"
    paths=()
    while IFS= read -r path; do [ -n "$path" ] && paths+=("$path"); done < "$dir/paths"
    for path in "${paths[@]}"; do
        if [ -n "$(git -C "$root" ls-files -- "$path")" ]; then
            echo "downstream: $path is tracked by ospreylang; $name cannot own it" >&2
            exit 1
        fi
        rm -rf "${root:?}/${path:?}"
    done
    git -C "$dir" ls-files -z -- "${paths[@]}" | tar -C "$dir" --null -T - -cf - | tar -C "$root" -xf -
    echo "downstream: $name at $commit"
done

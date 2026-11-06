#!/usr/bin/env bash
# Implements [MEDIA-EXTERNAL]: videos, their posters and caption tracks are
# not stored in this repository. They are made and kept in the Videos
# repository (github.com/MelbourneDeveloper/Videos, projects/osprey) and
# published there as one release archive. This downloads the archive that
# media.lock pins, checks its SHA-256, and unpacks it at the paths the website,
# the Workbench lessons and the design docs read. It does nothing when that
# archive is already unpacked.
#
# The Videos repository is private: set GH_TOKEN (CI passes the VIDEOS_TOKEN
# secret) or sign in with `gh auth login`.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
lock="$root/scripts/media/media.lock"
tag="$(sed -n 's/^tag=//p' "$lock")"
sha="$(sed -n 's/^sha256=//p' "$lock")"
cache="$root/target/media"
stamp="$cache/$sha.unpacked"
archive="$cache/osprey-media.tar"
digest() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }
[ -f "$stamp" ] && { echo "media: $tag already unpacked"; exit 0; }
mkdir -p "$cache"
if [ ! -f "$archive" ] || [ "$(digest "$archive")" != "$sha" ]; then
  echo "media: downloading $tag"
  gh release download "$tag" -R MelbourneDeveloper/Videos -p osprey-media.tar -D "$cache" --clobber
fi
actual="$(digest "$archive")"
[ "$actual" = "$sha" ] || { echo "media: $tag has SHA-256 $actual, but media.lock pins $sha" >&2; exit 1; }
tar -xf "$archive" -C "$root"
rm -f "$cache"/*.unpacked
touch "$stamp"
echo "media: unpacked $tag ($(tar -tf "$archive" | wc -l | tr -d ' ') files)"

#!/usr/bin/env bash
# Implements [MEDIA-EXTERNAL]: videos, their posters and caption tracks are
# not stored in this repository. They are made and kept in the Videos
# repository (github.com/MelbourneDeveloper/Videos, projects/osprey) and
# published there as one release archive. This downloads the archive that
# media.lock pins, checks its SHA-256, and unpacks it at the paths the website,
# the Workbench lessons and the design docs read. It does nothing when every
# file of that archive is already in place.
#
# The Videos repository is private: set GH_TOKEN (CI passes the VIDEOS_TOKEN
# secret) or sign in with `gh auth login`.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
lock="$root/scripts/media/media.lock"
tag="$(sed -n 's/^tag=//p' "$lock")"
sha="$(sed -n 's/^sha256=//p' "$lock")"
cache="$root/target/media"
archive="$cache/osprey-media.tar"
digest() { if command -v sha256sum >/dev/null; then sha256sum "$1"; else shasum -a 256 "$1"; fi | cut -d' ' -f1; }
# Every file the archive holds is in place: a deleted file (a clean of ignored
# files, say) is unpacked again rather than trusted to a stamp.
present() { [ -f "$archive" ] && [ "$(digest "$archive")" = "$sha" ] && tar -tf "$archive" | while read -r f; do [ -f "$root/$f" ] || exit 1; done; }
present && { echo "media: $tag already in place"; exit 0; }
mkdir -p "$cache"
if [ ! -f "$archive" ] || [ "$(digest "$archive")" != "$sha" ]; then
  echo "media: downloading $tag"
  gh release download "$tag" -R MelbourneDeveloper/Videos -p osprey-media.tar -D "$cache" --clobber
fi
actual="$(digest "$archive")"
[ "$actual" = "$sha" ] || { echo "media: $tag has SHA-256 $actual, but media.lock pins $sha" >&2; exit 1; }
tar -xf "$archive" -C "$root"
echo "media: unpacked $tag ($(tar -tf "$archive" | wc -l | tr -d ' ') files)"

#!/usr/bin/env bash
# Implements [MEDIA-VIDEOS-REPO]: films are made in the Videos repository and
# committed here (Git LFS). This copies its projects/osprey/published/ tree,
# which mirrors this repository's paths, into this checkout. Commit the result.
#
#   make media                      reads ../Videos next to this checkout
#   make media VIDEOS_DIR=<path>    reads another Videos checkout
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
published="${VIDEOS_DIR:-$root/../Videos}/projects/osprey/published"
[ -d "$published" ] || { echo "media: no Videos checkout at $published (set VIDEOS_DIR)" >&2; exit 1; }
count="$(cd "$published" && find . -type f ! -name '.DS_Store' | wc -l | tr -d ' ')"
(cd "$published" && find . -type f ! -name '.DS_Store' -print0 | rsync -a --from0 --files-from=- ./ "$root/")
echo "media: copied $count files from $published; commit them"

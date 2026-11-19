#!/usr/bin/env bash
# Downloads the .deb files `apt-get install <packages>` would fetch, 16 at a time, into apt's
# archive cache, so the install that follows downloads nothing. The hosted runner's Azure mirror
# is slow per request rather than per byte: fetched one by one, Playwright's Chromium and WebKit
# libraries on Ubuntu 26 outlasted the Website E2E job's whole timeout.
# Usage: apt-prefetch.sh [apt-get install options] <package>...   (after `apt-get update`)
set -euo pipefail

[ "$#" -gt 0 ] || { echo "apt-prefetch: no packages given" >&2; exit 1; }
archives=/var/cache/apt/archives
staging=$(mktemp -d)
trap 'rm -rf "$staging"' EXIT

# --print-uris lists each needed archive as: 'URI' file size hash; curl reads them as a config.
# Hosted runners name their mirror through a mirror list (mirror+file:/etc/apt/apt-mirrors.txt/pool/...),
# which curl cannot fetch; such a URI is rewritten onto the list's first mirror.
apt-get --print-uris -qq install -y "$@" \
  | awk -F"'" -v dir="$staging" '
      /^\x27/ {
        uri = $2
        if (match(uri, /^mirror\+file:[^ ]*\.txt\//)) {
          list = substr(uri, 13, RLENGTH - 13)
          if (!(list in mirror)) { getline line < list; split(line, field, /[ \t]/); mirror[list] = field[1]; sub(/\/$/, "", mirror[list]) }
          uri = mirror[list] substr(uri, RLENGTH)
        }
        split($3, rest, " ")
        printf "url = \"%s\"\noutput = \"%s/%s\"\n", uri, dir, rest[1]
      }' \
  > "$staging/curl.conf"
count=$(grep -c '^url' "$staging/curl.conf" || [ $? -eq 1 ])
echo "apt-prefetch: $count archives"
[ "$count" -gt 0 ] || exit 0

curl --parallel --parallel-max 16 -fsSL --retry 5 --retry-all-errors --config "$staging/curl.conf"
fetched=$(find "$staging" -name '*.deb' | wc -l)
[ "$fetched" -eq "$count" ] || { echo "apt-prefetch: fetched $fetched of $count archives" >&2; exit 1; }
find "$staging" -name '*.deb' -exec sudo mv -t "$archives" {} +

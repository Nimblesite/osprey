#!/usr/bin/env bash
# Build a loopback-only snapshot from the running folder registry. Never deploy this directory.
set -euo pipefail
site=target/registry-site
work=$(mktemp -d target/registry-site-build/preview.XXXXXX)
trap 'rm -rf "$work"' EXIT
out="$work/site"
api=http://127.0.0.1:8787/v1
render() { printf '%s\n' "$@" | wasmtime target/registry-site-build/site.wasm; }
fetch() { curl --noproxy '*' --fail --silent --show-error --max-time 120 "$api/$1"; }
mkdir -p "$out/sources"
cp -R "$site/guide" "$site/publishing" "$site/upload" "$out/"
cp "$site/registry.$(shasum -a 256 src/osprey/packages/manager/web/assets/registry.css | cut -c1-12).css" "$out/"
sed "s/form-action 'none'/script-src 'self'; form-action 'self'/" "$site/_headers" > "$out/_headers"
fetch packages > "$work/catalog"
jq -er '.packages | type == "array"' "$work/catalog" >/dev/null
jq -r '.packages[].key' "$work/catalog" > "$work/keys"
: > "$work/rows"
while IFS= read -r key; do
  [[ "$key" =~ ^@[a-z0-9][a-z0-9-]*/[a-z0-9][a-z0-9-]*$ ]]
  mkdir -p "$out/packages/$key"
  jq -er --arg key "$key" '.packages[] | select(.key == $key) | .releases[] | [.epoch,.ordinal,.digest] | @tsv' "$work/catalog" > "$work/versions"
  : > "$work/links"
  while IFS=$'\t' read -r epoch ordinal digest; do
    [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ && "$epoch.$ordinal" =~ ^[0-9]+\.[0-9]+$ ]]
    source_file="$out/sources/$digest.json"
    fetch "releases/$digest/source" > "$source_file"
    jq -er --arg key "$key" --arg digest "$digest" 'select(.key == $key and .digest == $digest) | .files[] | select(.path == "osprey.toml") | .text' "$source_file" > "$work/manifest.toml"
    summary=$(yq -p toml -o yaml -r '.package.summary // ""' "$work/manifest.toml")
    [[ "$summary" != *$'\n'* && "$summary" != *$'\r'* ]]
    render release "e$epoch.r$ordinal" "$digest" >> "$work/links"
  done < "$work/versions"
  render row "$key" "$summary" >> "$work/rows"
  render package "$key" "$summary" "$(tr -d '\n' < "$work/links")" > "$out/packages/$key/index.html"
done < "$work/keys"
render local "$(tr -d '\n' < "$work/rows")" > "$out/index.html"
filter=src/osprey/packages/manager/web/assets/filter.js
asset="filter.$(shasum -a 256 "$filter" | cut -c1-12).js"
grep -Fq "/$asset" "$out/index.html"
cp "$filter" "$out/$asset"
mkdir -p target/registry-site-preview
rsync -a --delete "$out/" target/registry-site-preview/

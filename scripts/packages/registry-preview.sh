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
cp "$site/registry-mark.svg" "$out/"
cp -R "$site/guide" "$site/publishing" "$site/upload" "$out/"
cp "$site/registry.$(shasum -a 256 src/osprey/packages/manager/web/assets/registry.css | cut -c1-12).css" "$out/"
sed "s/form-action 'none'/connect-src 'self'; script-src 'self' 'wasm-unsafe-eval'; form-action 'self'/" "$site/_headers" > "$out/_headers"
fetch packages > "$work/catalog"
jq -er '.packages | type == "array"' "$work/catalog" >/dev/null
jq -r '.packages[].key' "$work/catalog" > "$work/keys"
: > "$work/rows"
: > "$work/credits"
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
    version="e$epoch.r$ordinal"
    receipt=unrecorded
    if [[ -f target/local-registry.receipts ]] && grep -Fxq "published $key $version $digest" target/local-registry.receipts; then
      receipt=recorded
      render credit "$key" "$version" >> "$work/credits"
    fi
    render release "$version" "$digest" "$receipt" >> "$work/links"
  done < "$work/versions"
  render row "$key" "$summary" >> "$work/rows"
  metadata=$(yq -p toml -o json '.' "$work/manifest.toml")
  license=$(jq -r '.package.license // "Unspecified"' <<< "$metadata")
  keywords=$(jq -r '.package.keywords // [] | join(", ")' <<< "$metadata")
  deps=$(jq -r '.dependencies // {} | keys | join(", ")' <<< "$metadata")
  for field in "$license" "$keywords" "$deps"; do [[ "$field" != *$'\n'* && "$field" != *$'\r'* ]]; done
  render package "$key" "$summary" "$receipt" "$license" "${keywords:-None}" "${deps:-None}" "$(tr -d '\n' < "$work/links")" > "$out/packages/$key/index.html"
done < "$work/keys"
if [[ -s "$work/credits" ]]; then
  mkdir -p "$out/publishers/osprey-repository"
  render publisher "$(tr -d '\n' < "$work/credits")" > "$out/publishers/osprey-repository/index.html"
fi
asset() {
  local source="$1" name="$2" ext="$3" digest
  digest=$(shasum -a 256 "$source" | cut -c1-12)
  cp "$source" "$out/$name.$digest.$ext"
  printf '%s' "$name.$digest.$ext"
}
adapter=$(asset src/toolchain/hosts/web/src/list-filter.mjs filter mjs)
host=$(asset src/osprey/examples/wasm/wasi-shim.mjs wasi mjs)
guest=$(asset target/registry-site-build/site.wasm site wasm)
render local "$(tr -d '\n' < "$work/rows")" "$adapter" "$host" "$guest" > "$out/index.html"
mkdir -p target/registry-site-preview
rsync -a --delete "$out/" target/registry-site-preview/

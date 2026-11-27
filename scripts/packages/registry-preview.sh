#!/usr/bin/env bash
# Build a loopback-only snapshot from the running folder registry. Never deploy this directory.
set -euo pipefail
client="$PWD/target/osprey-pkg"
site=target/registry-site
work=$(mktemp -d target/registry-site-build/preview.XXXXXX)
report="$PWD/$work/client-report"
trap 'rm -rf "$work"' EXIT
out="$work/site"
api=http://127.0.0.1:8787/v1
render() { printf '%s\n' "$@" | wasmtime target/registry-site-build/site.wasm; }
fetch() { curl --noproxy '*' --fail --silent --show-error --max-time 120 "$api/$1"; }
pkg() {
  printf '%s\n' "$*" | "$client" > "$report"
  if ! grep -Fxq 'status: 0' "$report"; then cat "$report" >&2; return 1; fi
}
release_docs() {
  if [[ $(yq -o yaml -p toml '.dependencies | length' "$work/manifest.toml") -gt 0 ]]; then
    pkg lock --registry "${api%/v1}" --project "$release_source" &&
      pkg fetch --registry "${api%/v1}" --project "$release_source" &&
      pkg docs --project "$release_source" --compiler target/release/osprey --docs-dir "$docs"
  else
    target/release/osprey --docs "$release_source" --docs-dir "$docs" --docs-format html
  fi
}
mkdir -p "$out/sources" "$out/tags" "$out/publishers" "$work/tags"
cp "$site/registry-mark.svg" "$site"/form.*.mjs "$out/"
cp -R "$site/guide" "$site/publishing" "$site/upload" "$out/"
form_asset=("$out"/form.*.mjs)
render local-upload "" "$(basename "${form_asset[0]}")" > "$out/upload/index.html"
cp "$site/registry.$(shasum -a 256 src/osprey/packages/manager/web/assets/registry.css | cut -c1-12).css" "$out/"
sed "s/script-src 'self'/script-src 'self' 'wasm-unsafe-eval'/;s/form-action 'none'/form-action 'self'/" "$site/_headers" > "$out/_headers"
fetch packages > "$work/catalog"
jq -er '.packages | type == "array"' "$work/catalog" >/dev/null
jq -r '.packages[].key' "$work/catalog" > "$work/keys"
: > "$work/rows"
: > "$work/credits"
while IFS= read -r key; do
  [[ "$key" =~ ^@[a-z0-9][a-z0-9-]*/[a-z0-9][a-z0-9-]*$ ]]
  mkdir -p "$out/packages/$key"
  jq -er --arg key "$key" '.packages[] | select(.key == $key) | .releases | sort_by(.epoch,.ordinal)[] | [.epoch,.ordinal,.digest] | @tsv' "$work/catalog" > "$work/versions"
  : > "$work/links"
  while IFS=$'\t' read -r epoch ordinal digest; do
    [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ && "$epoch.$ordinal" =~ ^[0-9]+\.[0-9]+$ ]]
    source_file="$out/sources/$digest.json"
    fetch "releases/$digest/source" > "$source_file"
    install=$(mktemp -d "$work/release.XXXXXX")
    ln "$source_file" "$install/bundle.json"
    (cd "$install" && pkg fetch "$digest" --bundle bundle.json)
    release_source="$install/.osprey/packages/$key/${digest#sha256:}"
    jq -e --arg key "$key" --argjson epoch "$epoch" --argjson ordinal "$ordinal" \
      '.key == $key and .epoch == $epoch and .ordinal == $ordinal' "$source_file" >/dev/null
    cp "$release_source/osprey.toml" "$work/manifest.toml"
    summary=$(yq -p toml -o yaml -r '.package.summary // ""' "$work/manifest.toml")
    [[ "$summary" != *$'\n'* && "$summary" != *$'\r'* ]]
    version="e$epoch.r$ordinal"
    docs="$out/packages/$key/$version/docs"
    docs_status=unavailable
    if release_docs && [[ -s "$docs/api/index.html" ]]; then
      node scripts/packages/registry-docs.mjs "$site" "$docs" "$key" "$version"
      docs_status=generated
    else
      printf 'Documentation unavailable for %s %s\n' "$key" "$version" >&2
    fi
    receipt=unrecorded
    if [[ -f target/local-registry.receipts ]] && grep -Fxq "published $key $version $digest" target/local-registry.receipts; then
      receipt=recorded
      render credit "$key" "$version" "$summary" >> "$work/credits"
    fi
    license=$(yq -p toml -o yaml -r '.package.license // "Unspecified"' "$work/manifest.toml")
    [[ "$license" != *$'\n'* && "$license" != *$'\r'* ]]
    notice_version=
    notices=$(jq -c '[.files[] | select(.path | test("^(notices?|third_party_notices)([.](txt|md))?$"; "i"))]' "$source_file")
    if [[ "$notices" != "[]" ]]; then notice_version="$version"; fi
    render release "$version" "$digest" "$receipt" "$docs_status" "$license" "$notices" >> "$work/links"
  done < "$work/versions"
  row=$(render row "$key" "$summary" "$license")
  echo "$row" >> "$work/rows"
  metadata=$(yq -p toml -o json '.' "$work/manifest.toml")
  keywords=$(jq -r '.package.keywords // [] | join(", ")' <<< "$metadata")
  while IFS= read -r tag; do
    [[ "$tag" =~ ^[a-z0-9][a-z0-9-]{0,31}$ ]]
    echo "$row" >> "$work/tags/$tag"
  done < <(jq -r '.package.keywords[]?' <<< "$metadata")
  deps=$(jq -r '.dependencies // {} | keys | join(", ")' <<< "$metadata")
  for field in "$license" "$keywords" "$deps"; do [[ "$field" != *$'\n'* && "$field" != *$'\r'* ]]; done
  render package "$key" "$summary" "$receipt" "$license" "${keywords:-None}" "${deps:-None}" "$notice_version" "$(tr -d '\n' < "$work/links")" > "$out/packages/$key/index.html"
done < "$work/keys"
if [[ -s "$work/credits" ]]; then
  mkdir -p "$out/publishers/osprey-repository"
  render publisher "$(tr -d '\n' < "$work/credits")" > "$out/publishers/osprey-repository/index.html"
fi
publisher_list="No recorded publishers."
if [[ -s "$work/credits" ]]; then
  publisher_list='<a href="/publishers/osprey-repository/">Osprey repository publisher</a>'
fi
render directory Publishers "$publisher_list" > "$out/publishers/index.html"
: > "$work/tag-links"
for group in "$work/tags/"*; do
  [[ -f "$group" ]] || continue
  tag=${group##*/}
  mkdir -p "$out/tags/$tag"
  render directory "$tag" "$(tr -d '\n' < "$group")" > "$out/tags/$tag/index.html"
  printf '<a class="prose-chip" href="/tags/%s/">%s</a>' "$tag" "$tag" >> "$work/tag-links"
done
render directory Tags "$(cat "$work/tag-links")" > "$out/tags/index.html"
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

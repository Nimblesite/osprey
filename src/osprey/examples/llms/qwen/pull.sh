#!/bin/sh
# Pull the pinned Qwen3.8-27B GGUF into ~/.cache/osprey-llm/gguf (plan 0049's
# "osprey-llm pull"): the download resumes from an existing .part and is only
# renamed .gguf once its SHA-256 matches Hugging Face's LFS metadata. Every
# message goes to stderr; only the final path goes to stdout, so
# GGUF=$(sh pull.sh) captures the path.
set -eu

QWEN_REPO=${QWEN_REPO:-unsloth/Qwen3.8-27B-GGUF}
QWEN_REVISION=${QWEN_REVISION:-4ca720788d1e01f1bff70c033e0d0028fd02e502}
QWEN_QUANT=${QWEN_QUANT:-UD-Q4_K_XL}
QWEN_DIR=${QWEN_DIR:-$HOME/.cache/osprey-llm/gguf}
QWEN_HUB=${QWEN_HUB:-https://huggingface.co}
FILE="Qwen3.8-27B-$QWEN_QUANT.gguf"
GGUF="$QWEN_DIR/$FILE"
PART="$GGUF.part"

say() { printf 'qwen-pull: %s\n' "$*" >&2; }
die() { say "$1"; exit 1; }

bytes_of() { wc -c < "$1" | tr -d ' '; }

sha256_of() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | cut -d ' ' -f1
  else sha256sum "$1" | cut -d ' ' -f1
  fi
}

# "<size> <sha256>" of FILE from the tree JSON on stdin; exit 3 = no entry or no lfs.oid.
select_entry() {
  python3 -I -c '
import json, sys
entry = next((e for e in json.load(sys.stdin) if e.get("path") == sys.argv[1]), None)
lfs = (entry or {}).get("lfs") or {}
if entry is None or "oid" not in lfs or "size" not in entry:
    sys.exit(3)
print(entry["size"], lfs["oid"])' "$FILE"
}

metadata() {
  tree=$(curl -fsSL --retry 3 "$QWEN_HUB/api/models/$QWEN_REPO/tree/$QWEN_REVISION") ||
    die "cannot read the tree of $QWEN_REPO at revision $QWEN_REVISION"
  size_oid=$(printf '%s\n' "$tree" | select_entry) ||
    die "$FILE is not in $QWEN_REPO at revision $QWEN_REVISION, or carries no lfs.oid"
  SIZE=${size_oid%% *}
  OID=${size_oid#* }
}

# An existing GGUF of the right size is present: hashing 17 GB again proves
# nothing new, so it is skipped unless QWEN_VERIFY=1 forces the hash.
already_present() {
  [ -f "$GGUF" ] && [ "$(bytes_of "$GGUF")" -eq "$SIZE" ] || return 1
  if [ "${QWEN_VERIFY:-0}" = 1 ]; then
    actual=$(sha256_of "$GGUF")
    [ "$actual" = "$OID" ] || die "$GGUF is $SIZE bytes but sha256 $actual is not $OID; delete it and pull again"
    say "$GGUF verified: sha256 $OID"
  else
    say "$GGUF is present ($SIZE bytes); set QWEN_VERIFY=1 to hash it"
  fi
}

# Resume whatever .part holds; a .part already at SIZE needs no request (curl
# -C - would answer exit 33 on an already-complete range).
download() {
  if [ -f "$PART" ] && [ "$(bytes_of "$PART")" -eq "$SIZE" ]; then
    say "$PART already holds all $SIZE bytes"
    return 0
  fi
  mkdir -p "$QWEN_DIR"
  say "fetching $FILE from $QWEN_HUB/$QWEN_REPO at revision $QWEN_REVISION"
  curl -fL --retry 3 -C - -o "$PART" "$QWEN_HUB/$QWEN_REPO/resolve/$QWEN_REVISION/$FILE" ||
    die "download of $FILE failed (curl exit $?); whatever resumed stays in $PART"
}

# The .part must be complete and match the hub's digest. Nothing is deleted on
# a mismatch: a short .part resumes on the next run, and a complete but bad
# .part is the user's to remove, so the message names it.
verify_part() {
  actual_size=$(bytes_of "$PART")
  actual=$(sha256_of "$PART")
  if [ "$actual_size" -ne "$SIZE" ] || [ "$actual" != "$OID" ]; then
    say "digest mismatch for $PART; it was left in place"
    say "  expected: sha256 $OID over $SIZE bytes"
    say "  actual:   sha256 $actual over $actual_size bytes"
    die "if $PART is complete but bad, remove it by hand"
  fi
}

metadata
if ! already_present; then
  download
  verify_part
  mv "$PART" "$GGUF"
  say "$GGUF: $SIZE bytes, sha256 $OID"
fi
printf '%s\n' "$GGUF"

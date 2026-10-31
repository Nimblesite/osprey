#!/usr/bin/env bash
# Records pinned Lean exports for the shared proof kernel [PROOF-EXPORT].
# Default: IdImp. --theorem and_swap or --theorem Nat.add_comm selects the
# common-proof controls or the complete real arithmetic dependency closure.
# --check emits no stdout and requires byte-identical recorded input.
set -euo pipefail

export PATH="$HOME/.elan/bin:$PATH"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOOLS_DIR="$ROOT/benchmarks/proof/tools"
EXPORTER_REV="cfefafacc917b2d14c5f9bb092ed337f71317a4a" # lean4export, tag v4.25.0
CHECKER_REV="aa03950243dc18750c1546602ab054609ea8d57d" # lean4checker, tag v4.25.0
FORMAT_VERSION="3.1.0"
LEAN_VERSION="4.25.0"
MODE="record"
THEOREM="id_imp"

fail() { printf 'proof-export.sh: STOP — %s\n' "$*" >&2; exit 1; }

parse_args() {
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --check) MODE="--check" ;;
            --theorem)
                [ "$#" -ge 2 ] || fail "--theorem needs a name"
                THEOREM="$2"
                shift ;;
            record) ;;
            *) fail "unknown argument: $1" ;;
        esac
        shift
    done
}

select_theorem() {
    case "$THEOREM" in
        id_imp) BUILD_MODULE="IdImp"; EXPORT_ARGS=(IdImp); FIXTURE="id_imp" ;;
        and_swap) BUILD_MODULE="AndSwap"; EXPORT_ARGS=(AndSwap); FIXTURE="and_swap" ;;
        Nat.add_comm) BUILD_MODULE=""; EXPORT_ARGS=(Init.Data.Nat.Basic -- Nat.add_comm); FIXTURE="nat_add_comm" ;;
        *) fail "unknown theorem: $THEOREM" ;;
    esac
    RECORDED="$ROOT/examples/proof/kernel/exports/$FIXTURE.ndjson"
}

verify_toolchain() {
    [ "$(cat lean-toolchain)" = "leanprover/lean4:v$LEAN_VERSION" ] || fail "expected pinned Lean $LEAN_VERSION toolchain"
    version="$(lean --version)"
    case "$version" in
        "Lean (version $LEAN_VERSION,"*) ;;
        *) fail "expected Lean $LEAN_VERSION; got $version" ;;
    esac
    printf 'lean --version: %s\nlean4export rev: %s (tag v%s)\nlean4checker rev: %s (tag v%s)\n' "$version" "$EXPORTER_REV" "$LEAN_VERSION" "$CHECKER_REV" "$LEAN_VERSION" >&2
}

verify_header() {
    header="$(head -n 1 "$EXPORT_TEXT")"
    case "$header" in
        '{"meta":'*'"format":{"version":"'"$FORMAT_VERSION"'"}'*) ;;
        *) fail "lean4export $EXPORTER_REV writes '$header', not NDJSON format $FORMAT_VERSION" ;;
    esac
}

emit_export() {
    if [ "$MODE" != "--check" ]; then cat "$EXPORT_TEXT"; return; fi
    if cmp -s "$EXPORT_TEXT" "$RECORDED"; then
        printf 'proof-export.sh: the export is byte-identical to %s\n' "$RECORDED" >&2
        return
    fi
    diff -u "$RECORDED" "$EXPORT_TEXT" >&2 || true
    fail "the export differs from $RECORDED"
}

parse_args "$@"
select_theorem
cd "$TOOLS_DIR"
verify_toolchain
lake build lean4export lean4checker ${BUILD_MODULE:+"$BUILD_MODULE"} >&2
EXPORT_TEXT="$(mktemp "${TMPDIR:-/tmp}/osprey-proof-export.XXXXXX")"
trap 'rm -f "$EXPORT_TEXT"' EXIT
lake exe lean4export "${EXPORT_ARGS[@]}" > "$EXPORT_TEXT"
verify_header
emit_export

#!/usr/bin/env bash
# Records a real lean4export dump of the IdImp theorem module for the proof
# kernel's fixture ([PROOF-EXPORT]). Builds the pinned tools under
# benchmarks/proof/tools if needed, runs the exporter, writes the export text
# to stdout, and prints `lean --version` plus the pinned revs to stderr.
# With `--check` it prints nothing to stdout and instead fails unless the
# export is byte-identical to the recorded fixture, so a stale fixture or a
# drifted toolchain is caught rather than re-recorded.
#
# The kernel reader takes the NDJSON format 3.1.0 that the pinned exporter
# writes; a different format version on the meta line is a hard stop so the
# fixture is never recorded from an exporter the reader does not read.
set -euo pipefail

export PATH="$HOME/.elan/bin:$PATH"

TOOLS_DIR="$(cd "$(dirname "$0")/.." && pwd)/benchmarks/proof/tools"
EXPORTER_REV="cfefafacc917b2d14c5f9bb092ed337f71317a4a" # lean4export, tag v4.25.0
CHECKER_REV="aa03950243dc18750c1546602ab054609ea8d57d" # lean4checker, tag v4.25.0
FORMAT_VERSION="3.1.0"
RECORDED="$(cd "$(dirname "$0")/.." && pwd)/examples/proof/kernel/exports/id_imp.ndjson"
MODE="${1:-record}"

echo "lean --version: $(lean --version)" >&2
echo "lean4export rev: $EXPORTER_REV (tag v4.25.0)" >&2
echo "lean4checker rev: $CHECKER_REV (tag v4.25.0)" >&2

cd "$TOOLS_DIR"
lake build lean4export lean4checker IdImp >&2

text="$(lake exe lean4export IdImp)"

header="$(printf '%s\n' "$text" | head -n 1)"
case "$header" in
    '{"meta":'*'"format":{"version":"'"$FORMAT_VERSION"'"}'*) ;;
    *)
        echo "proof-export.sh: STOP — lean4export $EXPORTER_REV writes '$header' as its first line, not NDJSON format $FORMAT_VERSION; the kernel reader is not adapted" >&2
        exit 1
        ;;
esac

if [ "$MODE" = "--check" ]; then
    if printf '%s\n' "$text" | cmp -s - "$RECORDED"; then
        echo "proof-export.sh: the export is byte-identical to $RECORDED" >&2
        exit 0
    fi
    echo "proof-export.sh: STOP — the export differs from $RECORDED" >&2
    printf '%s\n' "$text" | diff - "$RECORDED" >&2 || true
    exit 1
fi
printf '%s\n' "$text"

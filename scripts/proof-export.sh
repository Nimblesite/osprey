#!/usr/bin/env bash
# Records a real lean4export dump of the IdImp theorem module for the proof
# kernel's fixture ([PROOF-EXPORT]). Builds the pinned tools under
# benchmarks/proof/tools if needed, runs the exporter, writes the export text
# to stdout, and prints `lean --version` plus the pinned revs to stderr.
#
# The kernel reader takes the 0.1.2 text format only. The verification below
# is a hard stop: if the pinned exporter writes anything else — a newer text
# version or the NDJSON format — this script exits 1 and the reader is never
# adapted to match. The last text-format lean4export commits (semver 2.0.0)
# are pinned to v4.22–v4.27 toolchains; the 0.1.2 text format was last
# written by commits pinned to v4.20.0-rc5. The v4.25.0 tags write NDJSON
# 3.1.0. See docs/reports/proof-export-fixture-2026-10-29.md.
set -euo pipefail

export PATH="$HOME/.elan/bin:$PATH"

TOOLS_DIR="$(cd "$(dirname "$0")/.." && pwd)/benchmarks/proof/tools"
EXPORTER_REV="cfefafacc917b2d14c5f9bb092ed337f71317a4a" # lean4export, tag v4.25.0
CHECKER_REV="aa03950243dc18750c1546602ab054609ea8d57d" # lean4checker, tag v4.25.0
TEXT_HEADER="0.1.2"

echo "lean --version: $(lean --version)" >&2
echo "lean4export rev: $EXPORTER_REV (tag v4.25.0)" >&2
echo "lean4checker rev: $CHECKER_REV (tag v4.25.0)" >&2

cd "$TOOLS_DIR"
lake build lean4export lean4checker IdImp >&2

text="$(lake exe lean4export IdImp)"
printf '%s\n' "$text"

header="$(printf '%s\n' "$text" | head -n 1)"
if [ "$header" != "$TEXT_HEADER" ]; then
    echo "proof-export.sh: STOP — lean4export $EXPORTER_REV writes '$header' as its first line, not the '$TEXT_HEADER' text header; the kernel reader is not adapted (docs/reports/proof-export-fixture-2026-10-29.md)" >&2
    exit 1
fi

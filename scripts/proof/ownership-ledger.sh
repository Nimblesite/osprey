#!/usr/bin/env bash
# [MEM-OWNERSHIP-STATIC-VERIFICATION] Check the ledger model in Lean, replay
# every theorem in Osprey's proof kernel under each backend, and require the
# proofs to fail when the model's guards are weakened.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out="$root/target/proof/ownership"
tools="$root/src/toolchain/benchmarks/proof/tools"
mkdir -p "$out/mutations"
cd "$tools"
lake build lean4export
lake env lean -DwarningAsError=true --root="$tools" -o "$out/OwnershipLedger.olean" OwnershipLedger.lean
exporter=$(lake env which lean4export)
lean_path=$(lake env printenv LEAN_PATH)
# Each theorem with the axioms its proof term may use: Lean's three standard
# ones at most. A new axiom, `sorryAx` included, fails the comparison.
while read -r name axioms; do
  theorem="OwnershipLedger.$name"
  LEAN_PATH="$out:$lean_path" "$exporter" OwnershipLedger -- "$theorem" > "$out/ledger-$name.ndjson"
  for backend in gc arc wasm; do
    if [[ $backend == wasm ]]; then
      result=$(printf 'ledger-%s.ndjson\t%s\n' "$name" "$theorem" | node "$root/scripts/proof/proof-batch-wasm.mjs" "$root/target/proof/openai/check-batch.wasm" "$out")
    else
      result=$(printf '%s\t%s\n' "$out/ledger-$name.ndjson" "$theorem" | "$root/target/proof/openai/check-batch.$backend")
    fi
    echo "$backend $result"
    test "$result" = "PASS $theorem | axioms: $axioms"
  done
done <<'THEOREMS'
step_preserves propext, Quot.sound
from_empty propext, Quot.sound
lent_pins_owner propext
lent_field_not_freed propext
lent_then_released propext, Quot.sound, Classical.choice
lent_twice_released propext, Quot.sound, Classical.choice
take_moves_owner propext
taken_field_not_freed propext
take_needs_no_loan propext
taken_freed_once propext, Quot.sound
dead_rejects propext, Quot.sound
edge_frames propext
only_free_kills propext
given_is_gone propext, Quot.sound, Classical.choice
moved_once propext, Quot.sound
join_needs_one_owner propext
borrow_keeps_owner none
read_needs_live propext
one_owner_always propext, Quot.sound
part_outlives_holder propext
take_on_one_path propext, Quot.sound
literal_needs_no_owner propext, Quot.sound
shared_element_refused propext, Quot.sound
capture_leaves_frame propext, Quot.sound
mixed_join_frees_once propext, Quot.sound, Classical.choice
THEOREMS
# A model that frees or moves a value while it is lent, or frees through a
# slot that does not own it, must lose its proofs.
mutate() {
  awk -v from="$2" -v to="$3" 'index($0, from) && !done {sub(/if c = owned [a-z]+ then/, to); done = 1} {print} END {exit !done}' \
    OwnershipLedger.lean > "$out/mutations/ledger-$1.lean"
  if lake env lean "$out/mutations/ledger-$1.lean" > "$out/mutations/ledger-$1.log" 2>&1; then
    echo "accepted ledger mutation $1" >&2
    exit 1
  fi
  # The failure must be in the proof of the theorem the mutation violates.
  local first
  first=$(grep -n "^theorem $4 " "$out/mutations/ledger-$1.lean" | cut -d: -f1)
  awk -F: -v first="$first" '/\.lean:[0-9]+:[0-9]+: error:/ && $2 > first && $2 < first + 12 {hit = 1} END {exit !hit}' \
    "$out/mutations/ledger-$1.log"
  echo "REJECT ledger $1 at $4"
}
mutate free-while-lent 'if c = owned owner then some { c with live' \
  'if c.live \&\& c.owners == [owner] then' lent_pins_owner
mutate move-while-lent 'if c = owned source then some (owned destination)' \
  'if c.live \&\& c.owners == [source] then' lent_pins_owner
mutate free-by-any-slot 'if c = owned owner then some { c with live' \
  'if c.live then' taken_field_not_freed

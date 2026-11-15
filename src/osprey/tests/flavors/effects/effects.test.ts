// Portable cross-contract oracle for [EFFECTS-OP-TYPING],
// [EFFECTS-STATIC-DISCHARGE], [EFFECTS-HANDLER-STATE], [TYPE-LIST],
// [TYPE-MAP], [TYPE-MAP-LOOKUP], [TYPE-MAP-OPS], and
// [BUILTIN-STRING-UFCS]. The TypeScript twin of effects.test.osp, sharing its
// golden. [FLAVOR-TS-EFFECTS]

/** Counter exposes mutation only through its handled tick operation. */
interface FlavorCounter extends Effect {
  /** Add `amount` to the counter the handler owns. */
  tick(amount: int): void;
}

/** The success value of `found`, or `fallback`. */
function or(found, fallback) {
  switch (found.kind) {
    case "success":
      return found.value;
    case "error":
      return fallback;
  }
}

function runBatchAssertions() {
  let total = 10;
  const empty = [99];
  const base = mapSet(mapSet(new Map<string, int[]>(), "first", [2, 3]), "second", [5, 9]);
  const grown = mapSet(base, "third", [7, 11, 13]);
  const replaced = mapSet(grown, "first", [20]);
  const removed = mapRemove(replaced, "second");
  const first = or(mapGet(base, "first"), empty);
  const second = or(mapGet(base, "second"), empty);
  const missing = or(mapGet(base, "missing"), [99]);

  checkAll("effect batch collection preconditions", [
    total === 10,
    mapLength(base) === 2,
    mapLength(grown) === 3,
    mapLength(replaced) === 3,
    mapLength(removed) === 2,
    listLength(first) === 2,
    listLength(second) === 2,
    listLength(missing) === 1,
    or(listGet(first, 0), 99) === 2,
    or(listGet(first, 1), 99) === 3,
    or(listGet(second, 0), 99) === 5,
    or(listGet(second, 1), 99) === 9,
    or(listGet(missing, 0), 0) === 99,
    !mapContains(base, "third"),
    mapContains(grown, "third"),
    !mapContains(removed, "second"),
  ]);

  return handle<FlavorCounter>({ tick: (amount) => { total = total + amount; } }, () => {
    forEachList(first, (amount) => perform<FlavorCounter>().tick(amount));
    checkAll("effectful callback applies the first generic list", [
      total === 15,
      `first total=${total}` === "first total=15",
    ]);
    perform<FlavorCounter>().tick(0 - 4);
    checkAll("negative handled transition remains checked", [
      total === 11,
      total < 15,
    ]);
    forEachList(second, (amount) => perform<FlavorCounter>().tick(amount));
    perform<FlavorCounter>().tick(0);
    checkAll("second list and identity transition compose", [
      total === 25,
      total > 11,
      `second total=${total}` === "second total=25",
    ]);

    return checkAll("handler exit preserves state and persistent collection versions", [
      total === 25,
      mapLength(base) === 2,
      mapLength(grown) === 3,
      listLength(or(mapGet(grown, "third"), empty)) === 3,
      or(listGet(or(mapGet(grown, "third"), empty), 2), 99) === 13,
      listLength(or(mapGet(replaced, "first"), empty)) === 1,
      or(listGet(or(mapGet(replaced, "first"), empty), 0), 99) === 20,
      listLength(or(mapGet(base, "first"), empty)) === 2,
      listLength(mapKeys(grown)) === 3,
      listLength(mapValues(grown)) === 3,
    ]);
  });
}

/** This case proves the initial, intermediate, final, and rendered counter state. */
function mutationCase() {
  // Keep mutable state local so each execution starts independently at zero.
  let flavorCounter = 0;

  // The migrated test starts from the same zero-valued state as the example.
  checkAll("counter initial state", [flavorCounter === 0]);

  return handle<FlavorCounter>({ tick: (amount) => { flavorCounter = flavorCounter + amount; } }, () => {
    perform<FlavorCounter>().tick(1);

    // The first effect is observable independently instead of only in the sum.
    checkAll("counter first transition", [flavorCounter === 1]);

    perform<FlavorCounter>().tick(41);

    // The second effect reaches the legacy final value inside the handler.
    checkAll("counter second transition", [flavorCounter === 42]);

    // The final value and exact old output line both survive handler exit.
    checkAll("counter legacy final state and rendering", [
      flavorCounter === 42,
      `counter = ${flavorCounter}` === "counter = 42",
    ]);

    return runBatchAssertions();
  });
}

// Run the complete state-transition scenario as one named test.

test("handled mutation retains every counter transition", exactly(mutationCase));

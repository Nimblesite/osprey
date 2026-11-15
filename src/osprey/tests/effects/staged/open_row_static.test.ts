/**
 * One open-row helper can be specialized through a static handler. Its result
 * must be identical on native and wasm32. The TypeScript twin of
 * open_row_static.test.osp: TypeScript has no open-row spelling, so `relay`'s
 * row is inferred. [FLAVOR-TS-EFFECTS]
 */
interface Read extends Effect {
  value(): int;
}

function relay(callback) {
  return callback();
}

function work(): Requires<int, [Read]> {
  return perform<Read>().value();
}

const answer = handleStatic<Read>({ value: () => 41 }, () => relay(work));
console.log(answer);

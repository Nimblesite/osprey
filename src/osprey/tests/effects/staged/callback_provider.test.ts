/**
 * A static region owned by a higher-order provider discharges the operations
 * of the callback it receives as a parameter, at the provider's own
 * instantiation (#274). The TypeScript twin of callback_provider.test.osp.
 * Implements [STAGE-POLY], [STAGE-POLY-ERASURE], [STAGE-SIGNALS-CACHE] and
 * [FLAVOR-TS-EFFECTS].
 */
interface Signal<T> extends StaticEffect {
  read(): T;
}

interface Query {
  readonly text: string;
}

interface Case {
  readonly name: string;
  readonly work: () => void;
}

function listView() {
  return `list:${perform<Signal<Query>>().read().text}`;
}

function snap() {
  return signalSnapshot(listView);
}

function provide(store, work) {
  return handleStatic<Signal<Query>>({ read: () => store }, () => work());
}

function signalProviderCase() {
  const result = provide({ text: "x" } satisfies Query, snap);
  console.log(result.value);
  checkAll("a provider discharges a snapshot callback", [
    result.value === "list:x",
    result.revision === 1,
    join(result.dependencies, ";") === join(signalDependencies(listView), ";"),
  ]);
}

/** The same shape without generics or signals: the static stage alone decides whether the callback's requirements are discharged. */
interface Store extends StaticEffect {
  read(): string;
}

function shelf() {
  return `shelf:${perform<Store>().read()}`;
}

function stocked(store, work) {
  return handleStatic<Store>({ read: () => store }, () => work());
}

function staticMonoCase() {
  const label = stocked("jam", shelf);
  console.log(label);
  checkAll("a non-generic provider works the same way", [label === "shelf:jam"]);
}

/** A lambda written at the instantiation is as resolvable as a named function. */
function lambdaCallbackCase() {
  const label = stocked("pear", () => `crated:${perform<Store>().read()}`);
  console.log(label);
  checkAll("an inline lambda callback is discharged too", [label === "crated:pear"]);
}

/** A wrapper that only forwards the callback still reaches the provider's region, so the framework may add layers without repeating the handler. */
function restock(work) {
  return stocked("tin", work);
}

function forwardedCallbackCase() {
  const label = restock(shelf);
  console.log(label);
  checkAll("a forwarded callback reaches the provider's region", [label === "shelf:tin"]);
}

testCases([
  { name: "a provider discharges a snapshot callback", work: signalProviderCase } satisfies Case,
  { name: "a non-generic provider works the same way", work: staticMonoCase } satisfies Case,
  { name: "an inline lambda callback is discharged too", work: lambdaCallbackCase } satisfies Case,
  { name: "a forwarded callback reaches the provider's region", work: forwardedCallbackCase } satisfies Case,
]);

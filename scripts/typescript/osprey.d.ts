// Declarations for the OPTIONAL `tsc` compatibility lint of TypeScript-flavor
// sources ([FLAVOR-TS-LINT]). The Osprey compiler never reads this file: it
// parses TypeScript-flavor source itself and compiles the AST. This file only
// lets `tsc` confirm that the same source is also valid TypeScript.
//
// It declares what an Osprey program has in scope, with `noLib` set, so the
// JavaScript standard library is absent here as it is in a compiled program.
// lint.mjs appends a `declare function` for each builtin a checked source
// calls, translated from the compiler's own `osprey --hover` signature.

// ---- Global types the TypeScript compiler requires under `noLib` ----------

interface Array<T> {
  readonly length: int;
  readonly [index: number]: T;
}
interface ReadonlyArray<T> {
  readonly length: int;
  readonly [index: number]: T;
}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface RegExp {}
// tsc requires a global `Promise` type while inferring nested generic calls.
// No Osprey value has this type: a compiled program has no promises.
interface Promise<T> {
  readonly __never: never;
}
// A string has no `length` here: TypeScript counts UTF-16 code units and
// Osprey strings are UTF-8 ([FLAVOR-TS-EXPRESSIONS]).
interface String {}

// ---- Types ([FLAVOR-TS-TYPES]) --------------------------------------------

// `int` and the sized numeric types are aliases of `number` for `tsc`. Osprey
// keeps each distinct: an `int` is a 64-bit integer whose arithmetic follows
// the `Arith` policy, not binary64.
type int = number;
type int8 = number;
type int16 = number;
type int32 = number;
type uint8 = number;
type uint16 = number;
type uint32 = number;
type float16 = number;
type float32 = number;

/** A list of `T`; the same type as `T[]` and `readonly T[]`. */
type List<T> = readonly T[];
/** A map from string keys to `V`. `new Map<string, V>()` is the empty map. */
interface Map<K, V> {
  readonly __osprey: "Map";
  readonly __key: K;
  readonly __value: V;
}
interface MapConstructor {
  new <K, V>(): Map<K, V>;
  <V>(): Map<string, V>;
}
declare const Map: MapConstructor;

/** The failure of a builtin operation. */
interface Error {
  readonly __osprey: "Error";
}

/** An expected failure is a value: `success` with its value, or `error` with its message. */
type Result<T, E = Error> =
  | { readonly kind: "success"; readonly value: T }
  | { readonly kind: "error"; readonly message: string };
/** Construct the `success` variant of a `Result`. */
declare function Success<T>(value: T): { readonly kind: "success"; readonly value: T };
/** Construct the `error` variant of a `Result`. */
declare function Error(message: string): { readonly kind: "error"; readonly message: string };

/** `Requires<T, [Log]>` is `T`; the frontend reads the closed effect row. [FLAVOR-TS-EFFECTS] */
type Requires<T, Effects extends readonly unknown[]> = T;

/** The record an HTTP request handler returns to the native listener. */
interface HttpResponse {
  readonly status: int;
  readonly headers: string;
  readonly contentType: string;
  readonly streamFd: int;
  readonly isComplete: boolean;
  readonly partialBody: string;
}

/** A JSON value: Osprey's built-in `Json` union, built with the constructors below. */
interface Json {
  readonly __osprey: "Json";
}
/** One named member of a JSON object. */
interface JsonMember {
  readonly __osprey: "JsonMember";
}
declare const JsonNull: Json;
declare function JsonBool(value: boolean): Json;
declare function JsonInt(value: int): Json;
declare function JsonFloat(value: number): Json;
declare function JsonString(value: string): Json;
declare function JsonArray(items: List<Json>): Json;
declare function JsonObject(members: List<JsonMember>): Json;
declare function JsonMember(name: string, value: Json): JsonMember;
/** `value` as compact JSON text. */
declare function jsonEncode(value: Json): Result<string>;
/** `text` as a JSON document. */
declare function jsonDecode(text: string): Result<Json>;
/** The member `key` of an object. */
declare function jsonField(doc: Json, key: string): Result<Json>;
/** The element at `index` of an array. */
declare function jsonIndex(doc: Json, index: int): Result<Json>;
/** A string value's text. */
declare function jsonText(doc: Json): Result<string>;
/** An integer value. */
declare function jsonInt(doc: Json): Result<int>;
/** A float value. */
declare function jsonFloat(doc: Json): Result<number>;
/** A boolean value. */
declare function jsonBool(doc: Json): Result<boolean>;

/** `console.log(value)` prints one value and a newline. */
declare const console: { log(value: unknown): void };

// ---- Prelude functions ----------------------------------------------------

/** Continue with `next` on success; pass an error through unchanged. */
declare function andThen<T, R>(result: Result<T>, next: (value: T) => R): R | { readonly kind: "error"; readonly message: string };

/** Default `result ?: fallback`: the fallback is evaluated only for an error. */
declare function unwrapOr<T = any>(result: Result<T>, fallback: T): T;

// ---- Effects and fibers [FLAVOR-TS-EFFECTS] [FLAVOR-TS-INTEROP] -------------

/** An interface that extends `Effect` declares a dynamic Osprey effect. */
interface Effect {}
/** An interface that extends `StaticEffect` declares a static Osprey effect. */
interface StaticEffect {}
/** Operation result wrappers: the frontend reads the mode and multiplicity. */
type Control<T> = T;
type Abort<T> = T;
type Once<T> = T;
type Many<T> = T;
type Replayable<T> = T;
/** `perform<E>().op(args)` requests operation `op` of effect `E`. */
declare function perform<E>(): E;
/** The arms of a handler for `E`: one function per operation, and the clauses. */
type Arms<E> = { [K in keyof E]?: (...args: any[]) => unknown } & { return?: (value: any) => unknown; finally?: () => void };
/** A callable handler: it runs `work` with `E` handled. */
declare function handler<E>(arms: Arms<E>): <T>(work: () => T) => T;
declare function staticHandler<E>(arms: Arms<E>): <T>(work: () => T) => T;
/** `handle<E>(arms, () => body)` runs `body` with `E` handled. */
declare function handle<E, T = any>(arms: Arms<E>, body: () => T): T;
declare function handleStatic<E, T = any>(arms: Arms<E>, body: () => T): T;
/** Resume the performer of a control operation with `value`. */
declare function resume(value?: unknown): any;
/** The integer and float arithmetic policy every program has. */
interface Arith extends Effect {
  overflow(op: string, lhs: int, rhs: int, wrapped: int): int;
  divideByZero(op: string, lhs: number): number;
  remainderByZero(lhs: int): int;
}
/** Fibers are Osprey fibers, not promises. */
declare function spawn<T>(work: () => T): Fiber<T>;
declare function awaitFiber<T>(fiber: Fiber<T>): T;
declare function yieldFiber(value?: unknown): void;
declare function send<T>(channel: Channel<T>, value: T): Result<int>;
declare function recv<T>(channel: Channel<T>): Result<T>;

// ---- Opaque builtin types ------------------------------------------------

interface Iterator<T> { readonly __osprey: "Iterator"; readonly __item: T; }
interface Fiber<T> { readonly __osprey: "Fiber"; readonly __result: T; }
interface Channel<T> { readonly __osprey: "Channel"; readonly __item: T; }

// ---- Builtins: appended by lint.mjs ---------------------------------------

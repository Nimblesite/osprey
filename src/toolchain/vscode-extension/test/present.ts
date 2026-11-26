import * as assert from "assert";

/** `value`, asserted present: the caller's wait, length check or index proved it. */
export function present<T>(value: T | undefined, what: string): T {
  assert.ok(value !== undefined, `${what} is present`);
  return value;
}

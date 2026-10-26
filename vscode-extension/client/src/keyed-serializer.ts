/**
 * Mutations on one project root run strictly one at a time: every task is
 * chained onto the previous one's promise, so overlapping palette invocations
 * queue instead of racing the CLI's atomic relock.
 */
export function createProjectSerializer() {
  const tails = new Map<string, Promise<unknown>>();
  return function serialize<T>(root: string, task: () => Promise<T>): Promise<T> {
    const tail = tails.get(root) ?? Promise.resolve();
    const next = tail.then(task, task);
    const settled = next.catch(() => undefined);
    tails.set(root, settled);
    settled.then(() => { if (tails.get(root) === settled) tails.delete(root); });
    return next;
  };
}

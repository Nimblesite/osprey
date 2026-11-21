// The browser's WASI Preview 1 host for an Osprey Native application ([NATIVE-SHM-WASM]). The
// application needs no files, arguments or environment; wasi-libc still imports the clock, random
// bytes and the standard streams, which go to the console. Every other call answers ENOSYS.

import type { GuestHost } from "../../../vscode-extension/client/src/osprey-ui/instance";

const ESUCCESS = 0;
const ENOSYS = 52;

type Call = (...args: number[]) => number;

function written(memory: ArrayBuffer, iovs: number, count: number, out: number): string {
  const view = new DataView(memory);
  let text = "";
  let total = 0;
  for (let index = 0; index < count; index += 1) {
    const pointer = view.getUint32(iovs + index * 8, true);
    const length = view.getUint32(iovs + index * 8 + 4, true);
    text += new TextDecoder().decode(new Uint8Array(memory, pointer, length));
    total += length;
  }
  view.setUint32(out, total, true);
  return text;
}

function calls(memory: () => ArrayBuffer | undefined): Record<string, Call> {
  const view = (): DataView => new DataView(memory() ?? new ArrayBuffer(0));
  const zeroPair: Call = (count, size) => { view().setUint32(count, 0, true); view().setUint32(size, 0, true); return ESUCCESS; };
  return {
    fd_write: (fd, iovs, count, out) => {
      const text = written(memory() ?? new ArrayBuffer(0), iovs, count, out);
      if (fd === 1 || fd === 2) console.debug(text.trimEnd());
      return ESUCCESS;
    },
    environ_sizes_get: zeroPair,
    args_sizes_get: zeroPair,
    environ_get: () => ESUCCESS,
    args_get: () => ESUCCESS,
    clock_time_get: (_clock, _precision, out) => { view().setBigUint64(out, BigInt(Date.now()) * 1_000_000n, true); return ESUCCESS; },
    random_get: (pointer, length) => { crypto.getRandomValues(new Uint8Array(memory() ?? new ArrayBuffer(0), pointer, length)); return ESUCCESS; },
    proc_exit: () => ESUCCESS,
    sched_yield: () => ESUCCESS,
    fd_close: () => ESUCCESS,
    fd_seek: () => ESUCCESS,
    fd_fdstat_get: () => ESUCCESS,
  };
}

/** The browser as the instance's guest host. */
export const browserGuest: GuestHost = {
  imports: (memory) => {
    const known = calls(memory);
    return { wasi_snapshot_preview1: new Proxy(known, { get: (target, name: string) => target[name] ?? (() => ENOSYS) }) };
  },
  start: (instance) => {
    (instance.exports._start as () => void)();
    return 0;
  },
  random: (bytes) => { crypto.getRandomValues(bytes); },
};

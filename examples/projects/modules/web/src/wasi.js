// Small WASI Preview 1 host for Osprey command modules. Browser applications
// do not need a virtual filesystem, but wasi-libc still imports a handful of
// process, clock, random and stdio functions.

const ESUCCESS = 0;
const ENOSYS = 52;
const textDecoder = new TextDecoder("utf-8");

function dataView(state) {
  if (!state.memory) throw new Error("WASI memory is not attached");
  return new DataView(state.memory.buffer);
}

function bytes(state) {
  if (!state.memory) throw new Error("WASI memory is not attached");
  return new Uint8Array(state.memory.buffer);
}

function flushPending(state, write) {
  if (!state.pending) return;
  write(state.pending);
  state.pending = "";
}

function fdWrite(state, flush, fd, iovs, iovsLength, writtenPointer) {
  const view = dataView(state);
  let count = 0;
  let output = "";
  for (let index = 0; index < iovsLength; index += 1) {
    const pointer = view.getUint32(iovs + index * 8, true);
    const length = view.getUint32(iovs + index * 8 + 4, true);
    output += textDecoder.decode(bytes(state).subarray(pointer, pointer + length));
    count += length;
  }
  view.setUint32(writtenPointer, count, true);
  if (fd === 1 || fd === 2) {
    state.pending += output;
    if (output.includes("\n")) flush();
  }
  return ESUCCESS;
}

function zeroPair(state, countPointer, sizePointer) {
  const view = dataView(state);
  view.setUint32(countPointer, 0, true);
  view.setUint32(sizePointer, 0, true);
  return ESUCCESS;
}

function stdioCalls(state, flush) {
  return {
    fd_write: (...args) => fdWrite(state, flush, ...args),
    proc_exit: () => {
      flush();
      return ESUCCESS;
    },
    fd_close: () => ESUCCESS,
    fd_seek: () => ESUCCESS,
    fd_fdstat_get: () => ESUCCESS,
    fd_fdstat_set_flags: () => ESUCCESS,
  };
}

function platformCalls(state) {
  return {
    environ_sizes_get: (...args) => zeroPair(state, ...args),
    environ_get: () => ESUCCESS,
    args_sizes_get: (...args) => zeroPair(state, ...args),
    args_get: () => ESUCCESS,
    clock_time_get: (_clock, _precision, outputPointer) => {
      dataView(state).setBigUint64(outputPointer, BigInt(Date.now()) * 1_000_000n, true);
      return ESUCCESS;
    },
    random_get: (pointer, length) => {
      globalThis.crypto.getRandomValues(bytes(state).subarray(pointer, pointer + length));
      return ESUCCESS;
    },
    sched_yield: () => ESUCCESS,
    poll_oneoff: () => ENOSYS,
  };
}

function importsFor(implementations) {
  return new Proxy(implementations, {
    get(target, property) {
      return target[property] ?? (() => ENOSYS);
    },
  });
}

export function createWasi(write = (text) => console.debug(text.trimEnd())) {
  const state = { memory: null, pending: "" };
  const flush = () => flushPending(state, write);
  const implementations = { ...stdioCalls(state, flush), ...platformCalls(state) };
  return {
    imports: importsFor(implementations),
    setMemory(nextMemory) {
      state.memory = nextMemory;
    },
    flush,
  };
}

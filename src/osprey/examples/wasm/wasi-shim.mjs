// Shared WASI preview1 shim for Osprey-compiled command modules. [WASM-TARGET]
//
// Single source of truth used by BOTH the browser loader (src/osprey/examples/wasm/
// index.html, via `<script type="module">`) and the headless smoke test
// (scripts/wasm-browser-smoke.mjs, under Node) — so the exact code that runs in
// the browser is the code CI exercises. It implements only the preview1 calls a
// print-only command module touches; any other import degrades to ENOSYS rather
// than trapping. Output is gathered by `fd_write` and handed to a caller-supplied
// `write(text)` sink (the page + console in a browser, stdout under Node), which
// keeps the shim DOM-agnostic and therefore testable without a browser.

const ESUCCESS = 0;
const ENOSYS = 52;

/// Build a WASI shim that routes stdout/stderr `fd_write` to `write`.
export function makeWasi(write) {
  let memory = null;
  let pending = "";
  const view = () => new DataView(memory.buffer);
  const bytes = () => new Uint8Array(memory.buffer);
  const flush = () => {
    if (pending) {
      write(pending);
      pending = "";
    }
  };
  // fd_write(fd, iovs, iovs_len, nwritten) — gather the iovecs, decode UTF-8,
  // buffer until a newline so partial writes coalesce into clean lines.
  const fdWrite = (fd, iovs, iovsLen, nwrittenPtr) => {
    const dv = view();
    let written = 0;
    let text = "";
    for (let i = 0; i < iovsLen; i++) {
      const ptr = dv.getUint32(iovs + i * 8, true);
      const len = dv.getUint32(iovs + i * 8 + 4, true);
      text += new TextDecoder("utf-8").decode(bytes().subarray(ptr, ptr + len));
      written += len;
    }
    dv.setUint32(nwrittenPtr, written, true);
    if (fd === 1 || fd === 2) {
      pending += text;
      if (text.includes("\n")) flush();
    }
    return ESUCCESS;
  };
  const zeroPair = (countPtr, sizePtr) => {
    const dv = view();
    dv.setUint32(countPtr, 0, true);
    dv.setUint32(sizePtr, 0, true);
    return ESUCCESS;
  };
  const base = {
    fd_write: fdWrite,
    proc_exit: () => flush(),
    fd_close: () => ESUCCESS,
    fd_seek: () => ESUCCESS,
    fd_fdstat_get: () => ESUCCESS,
    fd_fdstat_set_flags: () => ESUCCESS,
    environ_sizes_get: zeroPair,
    environ_get: () => ESUCCESS,
    args_sizes_get: zeroPair,
    args_get: () => ESUCCESS,
    clock_time_get: (_id, _prec, outPtr) => {
      view().setBigUint64(outPtr, 0n, true);
      return ESUCCESS;
    },
    random_get: (ptr, len) => {
      globalThis.crypto.getRandomValues(bytes().subarray(ptr, ptr + len));
      return ESUCCESS;
    },
    poll_oneoff: () => ENOSYS,
  };
  return {
    setMemory: (m) => {
      memory = m;
    },
    flush,
    // Unknown preview1 calls return ENOSYS instead of trapping the whole run.
    imports: new Proxy(base, { get: (t, k) => t[k] ?? (() => ENOSYS) }),
  };
}

/// Instantiate `wasmBytes` as a WASI command module under the shim, run `_start`,
/// and return the raw wasm exports. `write` receives decoded stdout/stderr text.
export async function runModule(wasmBytes, write) {
  const wasi = makeWasi(write);
  const { instance } = await WebAssembly.instantiate(wasmBytes, {
    wasi_snapshot_preview1: wasi.imports,
  });
  wasi.setMemory(instance.exports.memory);
  instance.exports._start();
  wasi.flush();
  return instance.exports;
}

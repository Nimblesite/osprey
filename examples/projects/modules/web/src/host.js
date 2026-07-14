import React from "react";
import { createRoot } from "react-dom/client";
import { commandKind, nodeToReact, normalizeEnvelope } from "./protocol.js";
import { createWasi } from "./wasi.js";

const EMBEDDED_WASM = "__OSPREY_WASM_BASE64__";
const HTTP_TRACE_LIMIT = 64;
const decoder = new TextDecoder("utf-8");
const encoder = new TextEncoder();

function textBytes(value) {
  return encoder.encode(String(value ?? "")).byteLength;
}

function elapsedMs(startedAt) {
  return Math.max(0, Math.round((performance.now() - startedAt) * 100) / 100);
}

function transportError(context, error) {
  const detail = String(error?.message ?? error ?? "Unknown network error");
  return JSON.stringify({
    error: `${context.method} ${context.url} could not reach the ledger: ${detail}`,
  });
}

function runtimeDispatch(exports) {
  const dispatch =
    exports?.osprey_web_dispatch ?? exports?.dispatch ?? exports?.osprey_dispatch;
  if (typeof dispatch !== "function") {
    throw new Error("Osprey wasm must export osprey_web_dispatch");
  }
  return dispatch;
}

function scalarMessage(message, model) {
  const flat = { ...message, model };
  for (const [name, value] of Object.entries(flat)) {
    if (value === undefined || value === null) delete flat[name];
    else if (typeof value === "object") flat[name] = JSON.stringify(value);
    else if (!["string", "number", "boolean"].includes(typeof value)) {
      flat[name] = String(value);
    }
  }
  return flat;
}

function requestInit(command) {
  const method = String(command.method || "GET").toUpperCase();
  const headers = { ...(command.headers ?? {}) };
  const init = { method, headers };
  const hasBody = command.body !== undefined && command.body !== "";
  if (!hasBody || method === "GET" || method === "HEAD") return init;
  init.body = typeof command.body === "string" ? command.body : JSON.stringify(command.body);
  const contentType = Object.keys(headers).some((name) => name.toLowerCase() === "content-type");
  if (!contentType) headers["Content-Type"] = "application/json";
  return init;
}

function httpEvent(command, status, data) {
  return { kind: "http", id: String(command.id ?? command.commandId ?? ""), status, data };
}

function wasmImports(host, wasi) {
  return {
    wasi_snapshot_preview1: wasi.imports,
    osprey_web: {
      render: (pointer) => host.renderPointer(pointer),
      command: (pointer) => host.commandPointer(pointer),
    },
  };
}

function invokeWasmStart(exports) {
  if (typeof exports._start === "function") exports._start();
  else if (typeof exports.main === "function") exports.main();
  else throw new Error("Osprey wasm must export _start");
}

function bridgeTelemetry() {
  return {
    ready: false,
    renders: 0,
    events: 0,
    lastPayloadBytes: 0,
    lastDecodeMs: 0,
    trace: [],
  };
}

function decodeBase64(source) {
  if (!source || source.startsWith("__OSPREY_WASM_")) {
    throw new Error("The Osprey WebAssembly payload was not embedded");
  }
  const binary = atob(source);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}

function ensureRootElement() {
  let element = document.getElementById("app");
  if (!element) {
    element = document.createElement("div");
    element.id = "app";
    document.body.appendChild(element);
  }
  return element;
}

function ErrorScreen({ error }) {
  return React.createElement(
    "main",
    { className: "host-error", role: "alert" },
    React.createElement("span", { className: "host-error-mark" }, "T"),
    React.createElement("p", { className: "eyebrow" }, "Talon client did not start"),
    React.createElement("h1", null, "The browser engine hit turbulence."),
    React.createElement(
      "p",
      null,
      "The server is still safe. Reload the page, then inspect the browser console if this persists.",
    ),
    React.createElement("pre", null, String(error?.message ?? error)),
  );
}

export class OspreyReactHost {
  constructor(rootElement) {
    this.root = createRoot(rootElement);
    this.exports = null;
    this.memory = null;
    this.model = "{}";
    this.renderSequence = 0;
    this.pendingRenders = [];
    this.httpSequence = 0;
    this.logger = console;
    this.bridge = bridgeTelemetry();
    window.__TALON_BRIDGE__ = this.bridge;
    this.onPopState = () => {
      if (this.exports) this.dispatch({ kind: "location", value: window.location.hash });
    };
    window.addEventListener("popstate", this.onPopState);
  }

  readCString(pointer) {
    const start = Number(pointer);
    if (!this.memory || !Number.isSafeInteger(start) || start <= 0) {
      throw new Error(`Invalid Osprey string pointer: ${String(pointer)}`);
    }
    const bytes = new Uint8Array(this.memory.buffer);
    let end = start;
    while (end < bytes.length && bytes[end] !== 0) end += 1;
    if (end === bytes.length) throw new Error("Unterminated string from Osprey wasm");
    this.bridge.lastPayloadBytes = end - start;
    return decoder.decode(bytes.subarray(start, end));
  }

  readJson(pointer) {
    const started = performance.now();
    const source = this.readCString(pointer);
    try {
      const value = JSON.parse(source);
      this.bridge.lastDecodeMs = performance.now() - started;
      return value;
    } catch (error) {
      this.bridge.lastDecodeMs = performance.now() - started;
      throw new Error(`Invalid JSON from Osprey: ${error.message}`);
    }
  }

  renderPointer(pointer) {
    const value = this.readJson(pointer);
    this.bridge.renders += 1;
    this.commitEnvelope(value);
    return 0;
  }

  commandPointer(pointer) {
    const value = this.readJson(pointer);
    const commands = Array.isArray(value) ? value : [value];
    queueMicrotask(() => this.runCommands(commands));
    return 0;
  }

  commitEnvelope(value) {
    const envelope = normalizeEnvelope(value);
    this.model = envelope.model;
    const sequence = ++this.renderSequence;
    this.root.render(nodeToReact(envelope.view, (event) => this.dispatch(event)));
    if (envelope.commands.length) {
      // Let React commit first so focus commands can see newly rendered fields.
      queueMicrotask(() => {
        if (sequence <= this.renderSequence) this.runCommands(envelope.commands);
      });
    }
  }

  allocate(length) {
    const allocate =
      this.exports?.osp_alloc ??
      this.exports?.osprey_web_alloc ??
      this.exports?.malloc ??
      this.exports?.alloc;
    if (typeof allocate !== "function") {
      throw new Error("Osprey wasm must export osp_alloc");
    }
    // osp_alloc is `i64 -> pointer` in Osprey's runtime; aliases used by other
    // wasm toolchains often accept i32, so retain a narrow compatibility path.
    try {
      return Number(allocate(BigInt(length)));
    } catch (error) {
      if (!(error instanceof TypeError)) throw error;
      return Number(allocate(length));
    }
  }

  dispatch(message) {
    if (!this.exports) return;
    const dispatch = runtimeDispatch(this.exports);
    const pointer = this.writeDispatch(message);
    this.acceptDispatchResult(dispatch(pointer));
  }

  writeDispatch(message) {
    const payload = encoder.encode(`${JSON.stringify(scalarMessage(message, this.model))}\0`);
    this.bridge.events += 1;
    this.bridge.lastPayloadBytes = payload.byteLength - 1;
    const pointer = this.allocate(payload.byteLength);
    new Uint8Array(this.memory.buffer, pointer, payload.byteLength).set(payload);
    return pointer;
  }

  acceptDispatchResult(result) {
    // The canonical ABI renders through the import callback. A returned C
    // string is also accepted, which is convenient for tiny embedders/tests.
    const resultPointer = Number(result ?? 0);
    if (Number.isSafeInteger(resultPointer) && resultPointer > 0) {
      this.commitEnvelope(this.readJson(resultPointer));
    }
  }

  httpContext(command, init) {
    const commandId = String(command.id || "http");
    this.httpSequence += 1;
    return {
      correlationId: `${commandId}-${this.httpSequence}`,
      commandId,
      method: init.method,
      url: String(command.url ?? ""),
      requestBytes: textBytes(init.body),
    };
  }

  recordHttp(entry, level = "info") {
    this.bridge.trace.push(entry);
    const excess = this.bridge.trace.length - HTTP_TRACE_LIMIT;
    if (excess > 0) this.bridge.trace.splice(0, excess);
    const log = this.logger[level] ?? this.logger.info ?? this.logger.log;
    log?.call(this.logger, "[talon:bridge]", entry);
  }

  completeHttp(context, startedAt, status, data) {
    const entry = {
      ...context, phase: "response", status,
      responseBytes: textBytes(data), durationMs: elapsedMs(startedAt),
    };
    this.recordHttp(entry, status >= 400 ? "warn" : "info");
    this.dispatch(httpEvent(context, status, data));
  }

  failHttp(context, startedAt, error) {
    const entry = {
      ...context, phase: "response", status: 0, responseBytes: 0,
      durationMs: elapsedMs(startedAt), error: String(error?.message ?? error),
    };
    this.recordHttp(entry, "warn");
    this.dispatch(httpEvent(context, 0, transportError(context, error)));
  }

  async runHttp(command) {
    const startedAt = performance.now();
    const init = requestInit(command);
    const context = this.httpContext(command, init);
    this.recordHttp({ ...context, phase: "request" });
    try {
      const response = await fetch(command.url, init);
      const data = await response.text();
      this.completeHttp(context, startedAt, response.status, data);
    } catch (error) {
      this.failHttp(context, startedAt, error);
    }
  }

  runFocus(command) {
    requestAnimationFrame(() => {
      const element = document.getElementById(String(command.id ?? ""));
      element?.focus({ preventScroll: false });
    });
  }

  runNavigate(command) {
    const url = String(command.url ?? "");
    if (!url) return;
    if (command.replace === true || command.replace === "true") {
      window.history.replaceState(null, "", url);
    } else {
      window.history.pushState(null, "", url);
    }
  }

  runCommands(commands) {
    for (const command of commands) {
      if (!command || typeof command !== "object") continue;
      switch (commandKind(command)) {
        case "http":
          void this.runHttp(command);
          break;
        case "focus":
          this.runFocus(command);
          break;
        case "navigate":
          this.runNavigate(command);
          break;
        default:
          console.warn("Unknown Osprey web command", command);
      }
    }
  }

  async start(wasmBytes) {
    const wasi = createWasi((text) => console.debug("[osprey]", text.trimEnd()));
    const { instance } = await WebAssembly.instantiate(wasmBytes, wasmImports(this, wasi));
    this.attachInstance(instance, wasi);
    invokeWasmStart(instance.exports);
    wasi.flush();
    this.bridge.ready = true;
    if (window.location.hash) {
      this.dispatch({ kind: "location", value: window.location.hash });
    }
    return this;
  }

  attachInstance(instance, wasi) {
    this.exports = instance.exports;
    this.memory = instance.exports.memory;
    if (!(this.memory instanceof WebAssembly.Memory)) {
      throw new Error("Osprey wasm must export memory");
    }
    wasi.setMemory(this.memory);
  }
}

export async function boot() {
  const host = new OspreyReactHost(ensureRootElement());
  window.__OSPREY_WEB__ = host;
  try {
    await host.start(decodeBase64(EMBEDDED_WASM));
    document.documentElement.dataset.ospreyReady = "true";
  } catch (error) {
    console.error("Osprey web host failed", error);
    document.documentElement.dataset.ospreyReady = "false";
    host.root.render(React.createElement(ErrorScreen, { error }));
  }
}

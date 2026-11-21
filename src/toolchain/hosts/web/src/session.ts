// The page's one application session ([NATIVE-SHM-DELIVERY]): the wasm instance runs on the page,
// one call at a time, the VS Code host's session loop reads each frame, the shared renderer draws
// it, and the frame reaches the webview client through this window's own messages.

import { startInstance } from "../../../vscode-extension/client/src/osprey-ui/instance";
import { renderWorkspace } from "../../../vscode-extension/client/src/osprey-ui/render";
import { createWorkspaceSession, type WorkspaceSession } from "../../../vscode-extension/client/src/osprey-ui/session";
import type { NativeRuntime, SliceDelta } from "../../../vscode-extension/client/src/osprey-ui/wasi-runner";
import { fetched } from "./http";
import { browserGuest } from "./wasi";

/** The instance as the session's runtime: each call answers at once, in order. */
function runtime(module: WebAssembly.Module): NativeRuntime {
  const call = startInstance(module, browserGuest);
  let held: (readonly [string, string])[] = [];
  let id = 0;
  const delta = (): SliceDelta => held.splice(0);
  const now = (): number => Math.floor(Date.now() / 1000);
  return {
    open: async (options) => call({ id: (id += 1), call: "open", ...options, slices: delta() }),
    event: async (touch) => call({ id: (id += 1), call: "event", now: now(), ...touch, slices: delta() }),
    hold: (key, text) => { held = [...held, [key, text] as const]; },
    dispose: () => { held = []; },
  };
}

/** Start the application in `module`, drawing into this page. */
export function pageSession(module: WebAssembly.Module): WorkspaceSession {
  return createWorkspaceSession({
    runtime: runtime(module),
    send: (message) => window.postMessage(message, location.origin),
    execute: (ask) => fetched(ask),
    draw: (frame, regions, slices) => renderWorkspace({ regions, slices, theme: frame.theme, brand: false, loadingText: "Loading…" }),
    report: (error) => console.error("Osprey Native application:", error),
  });
}

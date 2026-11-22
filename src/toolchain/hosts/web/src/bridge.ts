// What the webview client calls `acquireVsCodeApi`, answered by the page itself: `ready` opens the
// application, `external` opens an https page in a new tab and every other message is a typed
// host event for the session. Imported before the client, so the client finds it at load.

import type { WorkspaceSession } from "../../../vscode-extension/client/src/osprey-ui/session";
import { pageSession } from "./session";

declare const OSPREY_APP_WASM: string;

function bytes(base64: string): Uint8Array {
  return Uint8Array.from(atob(base64), (char) => char.charCodeAt(0));
}

const session: Promise<WorkspaceSession> = WebAssembly.compile(bytes(OSPREY_APP_WASM)).then(pageSession);
const dark = matchMedia("(prefers-color-scheme: dark)");

function opened(live: WorkspaceSession): void {
  const argument = document.documentElement.dataset.ospreyArgument ?? location.origin;
  void live.start({ now: Math.floor(Date.now() / 1000), offset: -new Date().getTimezoneOffset() * 60, platform: "web",
    home: "", dark: dark.matches, arguments: [argument] });
  dark.addEventListener("change", () => live.appearance(dark.matches));
}

function received(message: unknown): void {
  const raw = message as { type?: unknown; url?: unknown };
  if (raw.type === "external" && typeof raw.url === "string" && raw.url.startsWith("https://")) {
    window.open(raw.url, "_blank", "noopener");
    return;
  }
  void session.then((live) => (raw.type === "ready" ? opened(live) : live.message(message)));
}

(globalThis as { acquireVsCodeApi?: () => { postMessage(message: unknown): void } }).acquireVsCodeApi =
  () => ({ postMessage: received });

// The webview's whole program: route the host's messages, draw each sealed frame's rows or the
// component markup the Node renderer told, paint scene canvases and drawing boards, and post
// typed events back. It holds no application state and sends no capability request.

import { applyRender, clientDiagnostics, clientNotice, installClient, ordinalOf, type Client } from "./client.js";
import { installEditors, refreshEditors, type Caret } from "./editor-client.js";
import { refreshBoards } from "./board-client.js";
import { keyEvent, pointerEvent, scenePoint, sceneTransform, viewedEvent } from "./events.js";
import { readFrame, type Scene, type Shown } from "./rows.js";
import { renderShown } from "./render.js";
import { accessBounds, paintScene } from "./scene.js";

declare function acquireVsCodeApi(): { postMessage(message: unknown): void };

interface FrameMessage {
  readonly type: "frame";
  readonly shapes: ArrayBuffer;
  readonly views: ArrayBuffer;
  readonly text: ArrayBuffer;
  readonly assets: Readonly<Record<string, string>>;
  readonly diagnostics: readonly string[];
}

interface RenderMessage extends Omit<FrameMessage, "type"> {
  readonly type: "render";
  readonly html: string;
}

interface BusyMessage { readonly type: "busy"; readonly active: boolean }
interface FatalMessage { readonly type: "fatal"; readonly message: string }
type HostMessage = RenderMessage | FrameMessage | BusyMessage | FatalMessage;

const vscode = acquireVsCodeApi();
const client: Client = {
  action: { pressed: null, pressedPointerId: null, cancelled: null, pendingPatch: null },
  edits: new Map(),
  timers: new Map(),
  savedFocus: null,
  hostDiagnostics: [],
  pendingLoss: undefined,
  runtimeBusy: false,
  runtimeTracked: false,
  post: {
    control: (ordinal, words) => vscode.postMessage({ type: "event", kind: 0, ordinal, words }),
    event: (event) => vscode.postMessage(event),
    external: (url) => vscode.postMessage({ type: "external", url }),
  },
};

let shown: Shown | undefined;
let assetMap: Readonly<Record<string, string>> = {};
let cleanup: (() => void) | undefined;

/** Sprites are loaded once per approved URI and every scene is repainted when one arrives. */
const sprites = new Map<string, HTMLImageElement>();

function loadImage(uri: string): HTMLImageElement | undefined {
  const held = sprites.get(uri);
  if (held !== undefined) return held.complete ? held : undefined;
  const image = new Image();
  image.decoding = "async";
  image.onload = () => redraw();
  image.src = uri;
  sprites.set(uri, image);
  return undefined;
}

function spriteFor(scene: Scene, atlas: number): HTMLImageElement | undefined {
  const source = scene.atlas[atlas];
  if (source === undefined || !Object.prototype.hasOwnProperty.call(assetMap, source)) return undefined;
  return loadImage(assetMap[source]!);
}

function draw(canvas: HTMLCanvasElement): void {
  const scene = shown?.scenes.get(Number(canvas.dataset.scene));
  if (scene === undefined) return;
  paintScene(canvas, scene, sceneTransform(scene, canvas.clientWidth, canvas.clientHeight), {
    image: (atlas) => spriteFor(scene, atlas),
  });
}

function redraw(): void {
  for (const canvas of Array.from(document.querySelectorAll("canvas.osp-canvas"))) {
    if (canvas instanceof HTMLCanvasElement) draw(canvas);
  }
}

/** Paint one canvas, describe its scene to assistive technology, and report its region. */
function prepare(canvas: HTMLCanvasElement): () => void {
  const scene = shown?.scenes.get(Number(canvas.dataset.scene));
  if (scene === undefined) return () => undefined;
  draw(canvas);
  const parent = canvas.parentElement;
  const overlay: HTMLElement[] = [];
  if (parent !== null) {
    for (const access of scene.access) {
      const box = accessBounds(access, sceneTransform(scene, canvas.clientWidth, canvas.clientHeight));
      const node = document.createElement("button");
      node.type = "button";
      node.className = "osp-access";
      node.textContent = access.label;
      node.setAttribute("aria-label", access.label);
      node.setAttribute("role", access.role === "menuitem" ? "menuitem" : access.role);
      if (access.selected) node.setAttribute("aria-current", "true");
      Object.assign(node.style, { position: "absolute", left: `${box.x}px`, top: `${box.y}px`,
        width: `${Math.max(box.w, 1)}px`, height: `${Math.max(box.h, 1)}px`, opacity: "0", cursor: "pointer" });
      node.addEventListener("click", () => {
        const ordinal = ordinalOf(canvas);
        if (ordinal === null) return;
        const x = access.x + access.w / 2;
        const y = access.y + access.h / 2;
        client.post.event(pointerEvent(ordinal, x, y, true, false));
        client.post.event(pointerEvent(ordinal, x, y, false, false));
      });
      parent.appendChild(node);
      overlay.push(node);
    }
  }
  let viewed = { width: -1, height: -1 };
  const observer = new ResizeObserver(() => {
    const measured = sceneTransform(scene, canvas.clientWidth, canvas.clientHeight);
    const ordinal = ordinalOf(canvas);
    if (ordinal !== null && scene.width > 0 && (measured.viewedWidth !== viewed.width || measured.viewedHeight !== viewed.height)) {
      viewed = { width: measured.viewedWidth, height: measured.viewedHeight };
      client.post.event(viewedEvent(ordinal, measured.viewedWidth, measured.viewedHeight));
    }
    draw(canvas);
  });
  observer.observe(canvas);
  return () => {
    observer.disconnect();
    overlay.forEach((node) => node.remove());
  };
}

/** Scenes, boards and editors after either kind of frame has replaced the markup. */
function rendered(): void {
  const stops = Array.from(document.querySelectorAll("canvas.osp-canvas"))
    .flatMap((canvas) => (canvas instanceof HTMLCanvasElement ? [prepare(canvas)] : []));
  cleanup?.();
  cleanup = () => stops.forEach((stop) => stop());
  refreshBoards(client);
  refreshEditors(caret);
}

const caret: Caret = { post: client.post, edits: client.edits };
client.rendered = rendered;

/** Read one frame's runs, keeping the last good view when a frame is refused. */
function readRuns(message: FrameMessage | RenderMessage): boolean {
  try {
    shown = readFrame({ shapes: message.shapes, views: message.views, text: message.text });
  } catch (error) {
    client.pendingLoss = `The frame was refused: ${error instanceof Error ? error.message : String(error)}`;
    return false;
  }
  assetMap = message.assets;
  client.pendingLoss = undefined;
  return true;
}

function render(message: RenderMessage | FrameMessage): void {
  const ready = readRuns(message);
  clientDiagnostics(client, message.diagnostics);
  if (!ready && message.type === "frame") return;
  applyRender(client, message.type === "render" ? message.html
    : renderShown(shown ?? { controls: [], roots: [], scenes: new Map() }, { assets: new Map(Object.entries(assetMap)) }));
}

function closest(target: EventTarget | null, selector: string): Element | null {
  return target instanceof Element ? target.closest(selector) : null;
}

function pointerAt(canvas: HTMLCanvasElement, clientX: number, clientY: number): { ordinal: number; x: number; y: number } | undefined {
  const scene = shown?.scenes.get(Number(canvas.dataset.scene));
  if (scene === undefined) return undefined;
  const box = canvas.getBoundingClientRect();
  const point = scenePoint(sceneTransform(scene, canvas.clientWidth, canvas.clientHeight), clientX - box.left, clientY - box.top);
  return { ordinal: ordinalOf(canvas) ?? -1, x: point.x, y: point.y };
}

function installSceneInput(): void {
  document.addEventListener("keydown", (event) => {
    const field = event.target;
    const commit = field instanceof Element ? field.getAttribute("data-commit") : null;
    if (field instanceof HTMLInputElement && (commit === "submit" || commit === "enter") && event.key === "Enter") {
      event.preventDefault();
      return;
    }
    const canvas = closest(event.target, "canvas.osp-canvas");
    if (canvas === null) return;
    const key = keyEvent(ordinalOf(canvas) ?? -1, event.key, true);
    if (key === undefined) return;
    if (event.key === " " || event.key.startsWith("Arrow") || event.key === "Tab") event.preventDefault();
    client.post.event(key);
  });
  document.addEventListener("keyup", (event) => {
    const canvas = closest(event.target, "canvas.osp-canvas");
    if (canvas === null) return;
    const key = keyEvent(ordinalOf(canvas) ?? -1, event.key, false);
    if (key !== undefined) client.post.event(key);
  });
}

let pressing: { canvas: HTMLCanvasElement; pointer: number } | undefined;
let hoverQueued = false;

function installScenePointer(): void {
  document.addEventListener("pointerdown", (event) => {
    const canvas = closest(event.target, "canvas.osp-canvas");
    if (!(canvas instanceof HTMLCanvasElement) || event.button !== 0 || !event.isPrimary) return;
    if (canvas.hasAttribute("data-board-slice")) return;
    event.preventDefault();
    canvas.focus();
    const at = pointerAt(canvas, event.clientX, event.clientY);
    if (at === undefined) return;
    pressing = { canvas, pointer: event.pointerId };
    canvas.setPointerCapture(event.pointerId);
    client.post.event(pointerEvent(at.ordinal, at.x, at.y, true, event.shiftKey));
  });
  document.addEventListener("pointermove", (event) => {
    const canvas = closest(event.target, "canvas.osp-canvas");
    if (!(canvas instanceof HTMLCanvasElement) || canvas.hasAttribute("data-board-slice")) return;
    if (pressing !== undefined && pressing.canvas !== canvas) return;
    const dragging = pressing !== undefined && pressing.pointer === event.pointerId;
    if (!dragging && hoverQueued) return;
    if (!dragging) hoverQueued = true;
    requestAnimationFrame(() => {
      hoverQueued = false;
      const at = pointerAt(canvas, event.clientX, event.clientY);
      if (at !== undefined) client.post.event(pointerEvent(at.ordinal, at.x, at.y, dragging, event.shiftKey));
    });
  });
  document.addEventListener("pointerup", (event) => {
    const finished = pressing;
    if (finished === undefined || finished.pointer !== event.pointerId) return;
    pressing = undefined;
    const at = pointerAt(finished.canvas, event.clientX, event.clientY);
    if (at !== undefined) client.post.event(pointerEvent(at.ordinal, at.x, at.y, false, event.shiftKey));
  });
}

window.addEventListener("message", (event: MessageEvent<HostMessage>) => {
  const message = event.data;
  if (message === null || typeof message !== "object") return;
  if (message.type === "busy") {
    clientNotice(client, "busy", message.active);
    return;
  }
  if (message.type === "fatal") {
    clientNotice(client, "fatal", message.message);
    return;
  }
  render(message);
});

installClient(client);
installEditors(caret);
installSceneInput();
installScenePointer();
vscode.postMessage({ type: "ready" });

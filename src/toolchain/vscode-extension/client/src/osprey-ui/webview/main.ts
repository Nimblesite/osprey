// The webview's whole program: decode each sealed frame's rows, check them as the host does,
// draw the checked controls as DOM and their canvases as scenes, and post typed events back.
// It holds no application state and sends no capability request.

import { controlEvent, keyEvent, pointerEvent, scenePoint, sceneTransform, toggledWords, viewedEvent } from "./events.js";
import { renderShown } from "./render.js";
import { readFrame, type Scene, type Shown } from "./rows.js";
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

interface BusyMessage { readonly type: "busy"; readonly active: boolean }
interface FatalMessage { readonly type: "fatal"; readonly message: string }
type HostMessage = FrameMessage | BusyMessage | FatalMessage;

const vscode = acquireVsCodeApi();
const root = document.getElementById("osprey-root");
const status = document.getElementById("osprey-runtime-status");
const diagnostics = document.getElementById("osprey-diagnostics");

let shown: Shown | undefined;
let assetMap: Readonly<Record<string, string>> = {};
let cleanup: (() => void) | undefined;
let runtimeBusy = false;
let pendingLoss: string | undefined;
let hostDiagnostics: readonly string[] = [];
let savedFocus: string | undefined;
let savedField: { ordinal: string; value: string } | undefined;

/** Sprites are loaded once per approved URI and every scene is repainted when one arrives. */
const sprites = new Map<string, HTMLImageElement>();

function notice(): void {
  const combined = pendingLoss === undefined ? [...hostDiagnostics] : [...hostDiagnostics, pendingLoss];
  if (diagnostics) {
    diagnostics.textContent = [...new Set(combined)].join("\n");
    diagnostics.hidden = combined.length === 0;
  }
  if (status) status.hidden = !runtimeBusy;
  if (root) root.setAttribute("aria-busy", String(runtimeBusy));
}

function saveFocus(): void {
  const name = document.hasFocus() ? document.activeElement?.getAttribute("data-focus") : undefined;
  savedFocus = name === null || name === undefined ? undefined : name;
  const active = document.activeElement;
  savedField = active instanceof HTMLInputElement && active.dataset.ordinal !== undefined && document.hasFocus()
    ? { ordinal: active.dataset.ordinal, value: active.value } : undefined;
}

/** A focused field keeps what the person typed while its frame is re-drawn. */
function restoreField(): void {
  const field = savedField;
  savedField = undefined;
  if (field === undefined) return;
  const element = Array.from(document.querySelectorAll("input[data-ordinal]"))
    .find((candidate) => candidate.getAttribute("data-ordinal") === field.ordinal);
  if (element instanceof HTMLInputElement && element.value !== field.value) element.value = field.value;
}

function restoreFocus(): void {
  if (savedFocus === undefined) return;
  const element = Array.from(document.querySelectorAll("[data-focus]"))
    .find((candidate) => candidate.getAttribute("data-focus") === savedFocus);
  if (element instanceof HTMLElement) element.focus();
  savedFocus = undefined;
}

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
function prepare(canvas: HTMLCanvasElement, frame: Shown): () => void {
  const scene = frame.scenes.get(Number(canvas.dataset.scene));
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
        const ordinal = Number(canvas.dataset.ordinal ?? "-1");
        if (ordinal < 0) return;
        const x = access.x + access.w / 2, y = access.y + access.h / 2;
        vscode.postMessage(pointerEvent(ordinal, x, y, true, false));
        vscode.postMessage(pointerEvent(ordinal, x, y, false, false));
      });
      parent.appendChild(node);
      overlay.push(node);
    }
  }
  let viewed = { width: -1, height: -1 };
  const observer = new ResizeObserver(() => {
    const measured = sceneTransform(scene, canvas.clientWidth, canvas.clientHeight);
    const ordinal = Number(canvas.dataset.ordinal ?? "-1");
    if (ordinal >= 0 && scene.width > 0 && (measured.viewedWidth !== viewed.width || measured.viewedHeight !== viewed.height)) {
      viewed = { width: measured.viewedWidth, height: measured.viewedHeight };
      vscode.postMessage(viewedEvent(ordinal, measured.viewedWidth, measured.viewedHeight));
    }
    draw(canvas);
  });
  observer.observe(canvas);
  return () => {
    observer.disconnect();
    overlay.forEach((node) => node.remove());
  };
}

function render(message: FrameMessage): void {
  let checked: Shown;
  try {
    checked = readFrame({ shapes: message.shapes, views: message.views, text: message.text });
  } catch (error) {
    pendingLoss = `The frame was refused: ${error instanceof Error ? error.message : String(error)}`;
    notice();
    return;
  }
  saveFocus();
  shown = checked;
  assetMap = message.assets;
  hostDiagnostics = message.diagnostics;
  pendingLoss = undefined;
  cleanup?.();
  cleanup = undefined;
  if (root !== null) {
    root.innerHTML = renderShown(checked, { assets: new Map(Object.entries(assetMap)) });
    const stops = Array.from(root.querySelectorAll("canvas.osp-canvas"))
      .flatMap((canvas) => (canvas instanceof HTMLCanvasElement ? [prepare(canvas, checked)] : []));
    cleanup = () => stops.forEach((stop) => stop());
  }
  restoreField();
  restoreFocus();
  notice();
}

function closest(target: EventTarget | null, selector: string): Element | null {
  return target instanceof Element ? target.closest(selector) : null;
}

document.addEventListener("click", (event) => {
  const control = closest(event.target, "[data-ordinal]");
  if (control === null || control instanceof HTMLInputElement || control instanceof HTMLCanvasElement) return;
  const ordinal = Number(control.getAttribute("data-ordinal"));
  if (!Number.isSafeInteger(ordinal) || ordinal < 0) return;
  const words = control.getAttribute("role") === "switch"
    ? toggledWords(control.getAttribute("aria-checked") === "true")
    : control.getAttribute("data-words") ?? "";
  vscode.postMessage(controlEvent(ordinal, words));
});

/** A field that delivers as it changes posts its text once the typist pauses. */
const pendingFields = new WeakMap<HTMLInputElement, number>();

function postField(field: HTMLInputElement): void {
  const ordinal = Number(field.dataset.ordinal ?? "-1");
  if (Number.isSafeInteger(ordinal) && ordinal >= 0) vscode.postMessage(controlEvent(ordinal, field.value));
}

document.addEventListener("input", (event) => {
  const field = event.target;
  if (!(field instanceof HTMLInputElement) || field.dataset.commit !== "change") return;
  const previous = pendingFields.get(field);
  if (previous !== undefined) window.clearTimeout(previous);
  pendingFields.set(field, window.setTimeout(() => {
    pendingFields.delete(field);
    postField(field);
  }, 160));
});

document.addEventListener("change", (event) => {
  const field = event.target;
  if (!(field instanceof HTMLInputElement) || field.dataset.commit !== "change") return;
  const previous = pendingFields.get(field);
  if (previous !== undefined) {
    window.clearTimeout(previous);
    pendingFields.delete(field);
  }
  postField(field);
});

document.addEventListener("keydown", (event) => {
  const field = event.target;
  if (field instanceof HTMLInputElement && field.dataset.commit === "enter" && event.key === "Enter") {
    const ordinal = Number(field.dataset.ordinal ?? "-1");
    if (Number.isSafeInteger(ordinal) && ordinal >= 0) vscode.postMessage(controlEvent(ordinal, field.value));
    event.preventDefault();
    return;
  }
  const canvas = closest(event.target, "canvas.osp-canvas");
  if (canvas === null) return;
  const key = keyEvent(Number(canvas.getAttribute("data-ordinal") ?? "-1"), event.key, true);
  if (key === undefined) return;
  if (event.key === " " || event.key.startsWith("Arrow") || event.key === "Tab") event.preventDefault();
  vscode.postMessage(key);
});

document.addEventListener("keyup", (event) => {
  const canvas = closest(event.target, "canvas.osp-canvas");
  if (canvas === null) return;
  const key = keyEvent(Number(canvas.getAttribute("data-ordinal") ?? "-1"), event.key, false);
  if (key !== undefined) vscode.postMessage(key);
});

let pressing: { canvas: HTMLCanvasElement; pointer: number; ordinal: number } | undefined;
let hoverQueued = false;

function pointerAt(canvas: HTMLCanvasElement, clientX: number, clientY: number): { ordinal: number; x: number; y: number } | undefined {
  const scene = shown?.scenes.get(Number(canvas.dataset.scene));
  if (scene === undefined) return undefined;
  const box = canvas.getBoundingClientRect();
  const point = scenePoint(sceneTransform(scene, canvas.clientWidth, canvas.clientHeight), clientX - box.left, clientY - box.top);
  return { ordinal: Number(canvas.dataset.ordinal ?? "-1"), x: point.x, y: point.y };
}

document.addEventListener("pointerdown", (event) => {
  const canvas = closest(event.target, "canvas.osp-canvas");
  if (!(canvas instanceof HTMLCanvasElement) || event.button !== 0 || !event.isPrimary) return;
  event.preventDefault();
  canvas.focus();
  const at = pointerAt(canvas, event.clientX, event.clientY);
  if (at === undefined) return;
  pressing = { canvas, pointer: event.pointerId, ordinal: at.ordinal };
  canvas.setPointerCapture(event.pointerId);
  vscode.postMessage(pointerEvent(at.ordinal, at.x, at.y, true, event.shiftKey));
});

document.addEventListener("pointermove", (event) => {
  const canvas = closest(event.target, "canvas.osp-canvas");
  if (!(canvas instanceof HTMLCanvasElement)) return;
  if (pressing !== undefined && pressing.canvas !== canvas) return;
  const dragging = pressing !== undefined && pressing.pointer === event.pointerId;
  if (!dragging && hoverQueued) return;
  if (!dragging) hoverQueued = true;
  requestAnimationFrame(() => {
    hoverQueued = false;
    const at = pointerAt(canvas, event.clientX, event.clientY);
    if (at !== undefined) vscode.postMessage(pointerEvent(at.ordinal, at.x, at.y, dragging, event.shiftKey));
  });
});

document.addEventListener("pointerup", (event) => {
  const finished = pressing;
  if (finished === undefined || finished.pointer !== event.pointerId) return;
  pressing = undefined;
  const at = pointerAt(finished.canvas, event.clientX, event.clientY);
  if (at !== undefined) vscode.postMessage(pointerEvent(finished.ordinal, at.x, at.y, false, event.shiftKey));
});

window.addEventListener("message", (event: MessageEvent<HostMessage>) => {
  const message = event.data;
  if (message === null || typeof message !== "object") return;
  if (message.type === "busy") {
    runtimeBusy = message.active;
    notice();
    return;
  }
  if (message.type === "frame") {
    runtimeBusy = false;
    render(message);
    return;
  }
  pendingLoss = message.message;
  runtimeBusy = false;
  notice();
});

vscode.postMessage({ type: "ready" });

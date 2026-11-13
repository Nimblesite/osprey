// [WORKBENCH-BOARD] Host drawing and hit testing only, restored from the JSON-protocol host and
// retargeted at the typed event boundary: each pointer point crosses as a point event, the
// finished stroke as its width and clock, a closed lasso as the strokes it enclosed, and a camera
// drag as its final `x,y` offsets in words ([NATIVE-SHM-EVENT]). Osprey owns strokes, framing,
// undo, clear, recognition and every proof decision.

import { encloseEvent, lassoEvent, pointEvent, strokeEvent } from "./events.js";
import type { Client } from "./client.js";

interface BoardStroke {
  readonly points: readonly (readonly [number, number])[];
  readonly width: number;
  readonly at: number;
}

interface BoardState {
  readonly size: { readonly width: number; readonly height: number };
  readonly view: { x: number; y: number; scale: number };
  readonly strokes: readonly BoardStroke[];
}

interface BoardScene {
  readonly canvas: HTMLCanvasElement;
  readonly base: HTMLCanvasElement;
  state: BoardState;
  readonly texture: HTMLCanvasElement;
  readonly colour: string;
  ratio: number;
  frame: { scale: number; x: number; y: number };
}

interface Drawing {
  scene: BoardScene;
  readonly pointer: number;
  points: [number, number][];
  readonly pan: boolean;
  readonly origin: { readonly x: number; readonly y: number; readonly scale: number };
  readonly start: readonly [number, number];
  readonly lasso: boolean;
  readonly began: number;
}

let drawing: Drawing | null = null;

function contextOf(canvas: HTMLCanvasElement): CanvasRenderingContext2D {
  const context = canvas.getContext("2d");
  if (context === null) throw new Error("A drawing board requires a canvas context");
  return context;
}

function chalkTexture(colour: string): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = 32;
  canvas.height = 32;
  const context = contextOf(canvas);
  context.fillStyle = colour;
  context.fillRect(0, 0, 32, 32);
  const pixels = context.getImageData(0, 0, 32, 32);
  for (let at = 3; at < pixels.data.length; at += 4) {
    pixels.data[at] = 85 + ((Math.imul(at, 1103515245) >>> 10) % 171);
  }
  context.putImageData(pixels, 0, 0);
  return canvas;
}

function chalkStroke(context: CanvasRenderingContext2D, stroke: { points: readonly (readonly [number, number])[]; width: number }, scene: BoardScene): void {
  const pattern = context.createPattern(scene.texture, "repeat");
  if (pattern === null) throw new Error("The drawing board cannot create its chalk texture");
  context.strokeStyle = pattern;
  context.fillStyle = pattern;
  context.lineWidth = stroke.width;
  context.lineCap = "round";
  context.lineJoin = "round";
  context.beginPath();
  stroke.points.forEach((point, index) => {
    if (index === 0) context.moveTo(point[0], point[1]);
    else context.lineTo(point[0], point[1]);
  });
  context.stroke();
  const first = stroke.points[0];
  if (first !== undefined) {
    context.beginPath();
    context.arc(first[0], first[1], stroke.width / 2, 0, Math.PI * 2);
    context.fill();
  }
}

function transform(context: CanvasRenderingContext2D, scene: BoardScene): void {
  const frame = scene.frame;
  context.setTransform(scene.ratio * frame.scale, 0, 0, scene.ratio * frame.scale, scene.ratio * frame.x, scene.ratio * frame.y);
}

function frameOf(canvas: HTMLCanvasElement, state: BoardState): { scale: number; x: number; y: number } {
  const scale = Math.min(canvas.clientWidth / state.size.width, canvas.clientHeight / state.size.height) * state.view.scale / 100;
  return { scale, x: (canvas.clientWidth - state.size.width * scale) / 2 + state.view.x * scale,
    y: (canvas.clientHeight - state.size.height * scale) / 2 + state.view.y * scale };
}

/** Redraw the retained strokes onto the board's offscreen base at its current view. */
function resizeBoard(scene: BoardScene): void {
  scene.ratio = window.devicePixelRatio || 1;
  scene.canvas.width = scene.base.width = Math.max(1, Math.round(scene.canvas.clientWidth * scene.ratio));
  scene.canvas.height = scene.base.height = Math.max(1, Math.round(scene.canvas.clientHeight * scene.ratio));
  scene.frame = frameOf(scene.canvas, scene.state);
  const backdrop = scene.canvas.parentElement?.querySelector("img") ?? null;
  if (backdrop instanceof HTMLImageElement) {
    Object.assign(backdrop.style, { width: `${scene.state.size.width * scene.frame.scale}px`,
      height: `${scene.state.size.height * scene.frame.scale}px`, left: `${scene.frame.x}px`,
      top: `${scene.frame.y}px`, objectFit: "fill" });
  }
  const context = contextOf(scene.base);
  context.setTransform(1, 0, 0, 1, 0, 0);
  context.clearRect(0, 0, scene.base.width, scene.base.height);
  transform(context, scene);
  for (const stroke of scene.state.strokes) chalkStroke(context, stroke, scene);
}

function blit(scene: BoardScene): void {
  const context = contextOf(scene.canvas);
  context.setTransform(1, 0, 0, 1, 0, 0);
  context.clearRect(0, 0, scene.canvas.width, scene.canvas.height);
  context.drawImage(scene.base, 0, 0);
}

function paintLasso(context: CanvasRenderingContext2D, points: readonly [number, number][], scene: BoardScene): void {
  context.strokeStyle = scene.colour;
  context.lineWidth = 1 / scene.frame.scale;
  context.setLineDash([6 / scene.frame.scale, 5 / scene.frame.scale]);
  context.beginPath();
  points.forEach((point, index) => {
    if (index === 0) context.moveTo(point[0], point[1]);
    else context.lineTo(point[0], point[1]);
  });
  context.closePath();
  context.stroke();
  context.setLineDash([]);
}

/** The base plus the stroke or lasso still being drawn, which no slice carries yet. */
function paintPending(held: Drawing | null): void {
  if (held === null) return;
  const scene = held.scene;
  blit(scene);
  const context = contextOf(scene.canvas);
  transform(context, scene);
  if (held.pan) return;
  if (held.lasso) paintLasso(context, held.points, scene);
  else chalkStroke(context, { points: held.points, width: 3 }, scene);
}

function boardPoint(event: PointerEvent, scene: BoardScene): [number, number] {
  const box = scene.canvas.getBoundingClientRect();
  return [
    Math.round(Math.max(0, Math.min(scene.state.size.width, (event.clientX - box.left - scene.frame.x) / scene.frame.scale))),
    Math.round(Math.max(0, Math.min(scene.state.size.height, (event.clientY - box.top - scene.frame.y) / scene.frame.scale))),
  ];
}

function insidePolygon(point: readonly [number, number], polygon: readonly (readonly [number, number])[]): boolean {
  let inside = false;
  for (let first = 0, previous = polygon.length - 1; first < polygon.length; previous = first++) {
    const a = polygon[first];
    const b = polygon[previous];
    if (a === undefined || b === undefined) continue;
    if ((a[1] > point[1]) !== (b[1] > point[1]) && point[0] < ((b[0] - a[0]) * (point[1] - a[1])) / (b[1] - a[1]) + a[0]) inside = !inside;
  }
  return inside;
}

function selectedStrokes(held: Drawing): number[] {
  return held.scene.state.strokes.reduce<number[]>((indices, stroke, index) => {
    if (stroke.points.some((point) => insidePolygon(point, held.points))) indices.push(index);
    return indices;
  }, []);
}

function ordinalOf(canvas: HTMLCanvasElement, attribute: string): number {
  return Number(canvas.dataset[attribute] ?? "-1");
}

function startBoard(client: Client, event: PointerEvent, scene: BoardScene): void {
  if (!event.isPrimary || event.button !== 0 || drawing !== null) return;
  event.preventDefault();
  scene.canvas.focus();
  const first = boardPoint(event, scene);
  drawing = { scene, pointer: event.pointerId, points: [first],
    pan: scene.canvas.dataset.panEnabled === "true", origin: { x: scene.state.view.x, y: scene.state.view.y, scale: scene.state.view.scale },
    start: [event.clientX, event.clientY], lasso: event.shiftKey && scene.canvas.dataset.lassoEnabled === "true",
    began: Date.now() };
  scene.canvas.setPointerCapture(event.pointerId);
  if (!drawing.pan) client.post.event(pointEvent(first[0], first[1]));
  paintPending(drawing);
}

function moveCamera(event: PointerEvent, held: Drawing): void {
  const scene = held.scene;
  scene.state = { ...scene.state, view: { x: Math.round(held.origin.x + (event.clientX - held.start[0]) / scene.frame.scale),
    y: Math.round(held.origin.y + (event.clientY - held.start[1]) / scene.frame.scale), scale: scene.state.view.scale } };
  resizeBoard(scene);
  paintPending(held);
}

function moveBoard(client: Client, event: PointerEvent, scene: BoardScene): void {
  const held = drawing;
  if (held === null || held.scene !== scene || held.pointer !== event.pointerId) return;
  if (held.pan) {
    moveCamera(event, held);
    return;
  }
  const point = boardPoint(event, scene);
  const last = held.points[held.points.length - 1];
  if (last !== undefined && point[0] === last[0] && point[1] === last[1]) return;
  held.points.push(point);
  if (held.points.length >= 8192) held.points = held.points.filter((_, index) => index % 2 === 0 || index === held.points.length - 1);
  client.post.event(pointEvent(point[0], point[1]));
  paintPending(held);
}

function finishBoard(client: Client, event: PointerEvent, scene: BoardScene, cancelled: boolean): void {
  if (drawing === null || drawing.scene !== scene || drawing.pointer !== event.pointerId) return;
  const held = drawing;
  drawing = null;
  if (held.pan) {
    if (cancelled) {
      scene.state = { ...scene.state, view: held.origin };
      resizeBoard(scene);
    } else {
      const view = scene.state.view;
      client.post.control(ordinalOf(scene.canvas, "panOrdinal"), `${view.x},${view.y}`);
    }
    blit(scene);
    return;
  }
  if (!cancelled && held.lasso) {
    for (const index of selectedStrokes(held)) client.post.event(encloseEvent(index));
    client.post.event(lassoEvent(ordinalOf(scene.canvas, "lassoOrdinal")));
  }
  if (!cancelled && !held.lasso) {
    client.post.event(strokeEvent(ordinalOf(scene.canvas, "eventOrdinal"), 3, Date.now() - held.began));
  }
  blit(scene);
}

/** Refresh every board after a render: rebuild its scene from the frame's slice, keeping a
 * stroke still being drawn on the canvas that now carries its slice. */
export function refreshBoards(client: Client): void {
  for (const canvas of Array.from(document.querySelectorAll("canvas[data-board-slice]"))) {
    if (!(canvas instanceof HTMLCanvasElement)) continue;
    const state = JSON.parse(canvas.dataset.boardState ?? "null") as BoardState;
    const colour = getComputedStyle(canvas).color;
    const scene: BoardScene = { canvas, base: document.createElement("canvas"), state,
      texture: chalkTexture(colour), colour, ratio: 1, frame: frameOf(canvas, state) };
    const held = drawing;
    resizeBoard(scene);
    if (held !== null && held.scene.canvas.dataset.boardSlice === canvas.dataset.boardSlice) {
      drawing = { ...held, scene };
      paintPending(drawing);
    } else {
      blit(scene);
    }
    const observer = new ResizeObserver(() => {
      if (canvas.isConnected) resizeBoard(scene);
      else observer.disconnect();
      const active = drawing;
      if (active !== null && active.scene === scene) paintPending(active);
      else blit(scene);
    });
    observer.observe(canvas);
    canvas.addEventListener("pointerdown", (event) => startBoard(client, event, scene));
    canvas.addEventListener("pointermove", (event) => moveBoard(client, event, scene));
    canvas.addEventListener("pointerup", (event) => finishBoard(client, event, scene, false));
    canvas.addEventListener("pointercancel", (event) => finishBoard(client, event, scene, true));
  }
}

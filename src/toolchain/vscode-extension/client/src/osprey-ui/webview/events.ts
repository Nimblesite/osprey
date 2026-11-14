// [NATIVE-SHM-EVENT]: one host event as the typed call the application exports. This module
// builds those calls from DOM input and maps a scene's region and pointer positions back into
// scene units; it holds no application state (mirrors hosts/gpui/src/events.rs and scene/input.rs).

/** The event kinds `ui::Hosting` declares. */
export const EVENT = {
  control: 0,
  key: 1,
  pointer: 2,
  viewed: 3,
  point: 4,
  stroke: 5,
  enclose: 6,
  lasso: 7,
  completed: 8,
  phase: 9,
  frame: 10,
  appearance: 11,
} as const;

export interface HostEvent {
  readonly type: "event";
  readonly kind: number;
  readonly ordinal: number;
  readonly words: string;
  readonly detail: string;
  readonly x: number;
  readonly y: number;
  readonly down: boolean;
  readonly shift: boolean;
}

function event(kind: number): HostEvent {
  return { type: "event", kind, ordinal: 0, words: "", detail: "", x: 0, y: 0, down: false, shift: false };
}

/** The control numbered `ordinal` delivering `words`. */
export function controlEvent(ordinal: number, words: string): HostEvent {
  return { ...event(EVENT.control), ordinal, words };
}

/** The words a control value carries: nothing, text, a flag or a whole number. */
export function wordsOf(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "boolean") return value ? "true" : "false";
  if (typeof value === "number" && Number.isFinite(value)) return String(value);
  return "";
}

/** One more point, in whole board points, of the stroke being drawn ([NATIVE-SHM-EVENT]). */
export function pointEvent(x: number, y: number): HostEvent {
  return { ...event(EVENT.point), x, y };
}

/** The stroke on board `ordinal` finished: `width` points wide at `at` milliseconds. */
export function strokeEvent(ordinal: number, width: number, at: number): HostEvent {
  return { ...event(EVENT.stroke), ordinal, x: width, y: at };
}

/** One more stroke, numbered `ordinal`, inside the lasso being drawn. */
export function encloseEvent(ordinal: number): HostEvent {
  return { ...event(EVENT.enclose), ordinal };
}

/** The lasso on board `ordinal` closed. */
export function lassoEvent(ordinal: number): HostEvent {
  return { ...event(EVENT.lasso), ordinal };
}

/** A key going down or up over the scene numbered `ordinal`. */
export function keyEvent(ordinal: number, name: string, down: boolean): HostEvent | undefined {
  const admitted = keyName(name);
  return admitted === undefined ? undefined : { ...event(EVENT.key), ordinal, words: admitted, down };
}

/** The pointer over the scene numbered `ordinal`, in scene units. */
export function pointerEvent(ordinal: number, x: number, y: number, down: boolean, shift: boolean): HostEvent {
  return { ...event(EVENT.pointer), ordinal, x: Number.isFinite(x) ? x : 0, y: Number.isFinite(y) ? y : 0, down, shift };
}

/** The scene numbered `ordinal` measured in scene units. */
export function viewedEvent(ordinal: number, width: number, height: number): HostEvent {
  return { ...event(EVENT.viewed), ordinal, x: width, y: height };
}

/** Admits only the host-neutral key vocabulary in the scene contract. */
export function keyName(key: string): string | undefined {
  const named: Readonly<Record<string, string>> = {
    ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down",
    " ": "space", Enter: "enter", Escape: "escape", Tab: "tab", Backspace: "backspace",
  };
  if (key in named) return named[key];
  const lowered = key.length === 1 ? key.toLowerCase() : "";
  return /^[a-z0-9]$/.test(lowered) ? lowered : undefined;
}

export interface SceneExtent {
  readonly width: number;
  readonly height: number;
  readonly zoom: number;
}

/** A canvas region's scene-to-pixel scale and the region measured in scene units. */
export interface SceneTransform {
  readonly scale: number;
  readonly viewedWidth: number;
  readonly viewedHeight: number;
}

/** The scene's zoom and top-left origin, shared by painting and pointer input. */
export function sceneTransform(scene: SceneExtent, regionWidth: number, regionHeight: number): SceneTransform {
  if (scene.width === 0) return { scale: 1, viewedWidth: regionWidth, viewedHeight: regionHeight };
  const whole = Math.min(regionWidth / scene.width, regionHeight / scene.height);
  const scale = scene.zoom === 0 ? whole : Math.min(scene.zoom, whole);
  if (!(scale > 0) || !Number.isFinite(scale)) return { scale: 1, viewedWidth: regionWidth, viewedHeight: regionHeight };
  return { scale, viewedWidth: regionWidth / scale, viewedHeight: regionHeight / scale };
}

/** One pointer position in scene units, relative to the canvas box. */
export function scenePoint(transform: SceneTransform, clientX: number, clientY: number): { x: number; y: number } {
  return { x: clientX / transform.scale, y: clientY / transform.scale };
}

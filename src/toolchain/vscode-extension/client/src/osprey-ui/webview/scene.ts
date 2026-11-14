// [GPUI-HOST-SCENE]: paints a checked scene's primitives in their stable depth order onto a
// 2D canvas. Every colour, size and point comes from the scene rows; this painter decides nothing.

import type { Access, Paint, Point, Scene, Shape, Tint } from "./rows.js";
import type { SceneTransform } from "./events.js";

function colour(tint: Tint): string {
  const [a, b, c, d] = tint.channels;
  return tint.space === "rgb"
    ? `rgba(${Math.round(a * 255)},${Math.round(b * 255)},${Math.round(c * 255)},${d})`
    : `hsla(${a * 360},${b * 100}%,${c * 100}%,${d})`;
}

function ink(value: number): string {
  return `#${value.toString(16).padStart(8, "0")}`;
}

function style(context: CanvasRenderingContext2D, paint: Paint): string | CanvasGradient {
  if (paint.paint === "solid") return colour(paint.tint);
  const radians = (paint.angle * Math.PI) / 180;
  const made = context.createLinearGradient(-Math.cos(radians) * 512, -Math.sin(radians) * 512,
    Math.cos(radians) * 512, Math.sin(radians) * 512);
  made.addColorStop(Math.max(0, Math.min(1, paint.from[1])), colour(paint.from[0]));
  made.addColorStop(Math.max(0, Math.min(1, paint.to[1])), colour(paint.to[0]));
  return made;
}

/** One SVG elliptical arc from `start` to `to`, in scene units. */
function arc(context: CanvasRenderingContext2D, start: Point, radii: Point, rotation: number, large: boolean, sweep: boolean, to: Point): void {
  const phi = (rotation * Math.PI) / 180;
  const cos = Math.cos(phi), sin = Math.sin(phi);
  const dx = (start.x - to.x) / 2, dy = (start.y - to.y) / 2;
  const x1 = cos * dx + sin * dy, y1 = -sin * dx + cos * dy;
  const rx = Math.abs(radii.x), ry = Math.abs(radii.y);
  if (rx === 0 || ry === 0) {
    context.lineTo(to.x, to.y);
    return;
  }
  const scaled = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
  const grow = scaled > 1 ? Math.sqrt(scaled) : 1;
  const rX = rx * grow, rY = ry * grow;
  const factor = (large === sweep ? -1 : 1) * Math.sqrt(Math.max(0, (rX * rX * rY * rY - rX * rX * y1 * y1 - rY * rY * x1 * x1)
    / (rX * rX * y1 * y1 + rY * rY * x1 * x1)));
  const cx1 = (factor * rX * y1) / rY, cy1 = (-factor * rY * x1) / rX;
  const cx = cos * cx1 - sin * cy1 + (start.x + to.x) / 2;
  const cy = sin * cx1 + cos * cy1 + (start.y + to.y) / 2;
  const angle = (ux: number, uy: number, vx: number, vy: number): number => {
    const value = (ux * vx + uy * vy) / (Math.hypot(ux, uy) * Math.hypot(vx, vy));
    return (ux * vy - uy * vx < 0 ? -1 : 1) * Math.acos(Math.max(-1, Math.min(1, value)));
  };
  const startAngle = angle(1, 0, (x1 - cx1) / rX, (y1 - cy1) / rY);
  const sweepAngle = angle((x1 - cx1) / rX, (y1 - cy1) / rY, (-x1 - cx1) / rX, (-y1 - cy1) / rY);
  context.ellipse(cx, cy, rX, rY, phi, startAngle, startAngle + (sweep ? sweepAngle : sweepAngle - Math.PI * 2), !sweep);
}

function steps(context: CanvasRenderingContext2D, shape: Extract<Shape, { shape: "path" }>): void {
  let current: Point = { x: 0, y: 0 };
  context.beginPath();
  for (const step of shape.steps) {
    switch (step.step) {
      case "move":
        context.moveTo(step.to.x, step.to.y);
        current = step.to;
        break;
      case "line":
        context.lineTo(step.to.x, step.to.y);
        current = step.to;
        break;
      case "curve":
        context.quadraticCurveTo(step.control.x, step.control.y, step.to.x, step.to.y);
        current = step.to;
        break;
      case "cubic":
        context.bezierCurveTo(step.first.x, step.first.y, step.second.x, step.second.y, step.to.x, step.to.y);
        current = step.to;
        break;
      case "arc":
        arc(context, current, step.radii, step.rotation, step.large, step.sweep, step.to);
        current = step.to;
        break;
      case "polygon":
        if (step.corners.length > 0) {
          context.moveTo(step.corners[0]!.x, step.corners[0]!.y);
          for (const corner of step.corners.slice(1)) context.lineTo(corner.x, corner.y);
          if (step.closed) context.closePath();
          current = step.corners[step.corners.length - 1]!;
        }
        break;
      case "close":
        context.closePath();
        break;
    }
  }
}

function outline(context: CanvasRenderingContext2D, shape: Extract<Shape, { shape: "path" }>): void {
  context.fillStyle = style(context, shape.paint);
  if (shape.outline.outline === "fill") {
    context.fill();
    return;
  }
  context.strokeStyle = context.fillStyle;
  context.lineWidth = shape.outline.width;
  context.lineJoin = shape.outline.bevel ? "bevel" : "miter";
  context.setLineDash([...shape.outline.dashes]);
  context.stroke();
  context.setLineDash([]);
}

/** Loaded sprite sources, by atlas index. */
export interface SceneImages {
  image(atlas: number): HTMLImageElement | undefined;
}

function painterOf(shape: Shape, sources: SceneImages): (context: CanvasRenderingContext2D) => void {
  switch (shape.shape) {
    case "rect":
      return (context) => {
        context.save();
        context.translate(shape.x, shape.y);
        context.rotate(shape.angle);
        context.fillStyle = ink(shape.colour);
        context.fillRect(-shape.w / 2, -shape.h / 2, shape.w, shape.h);
        context.restore();
      };
    case "disc":
      return (context) => {
        context.fillStyle = ink(shape.colour);
        context.beginPath();
        context.arc(shape.x, shape.y, shape.radius, 0, Math.PI * 2);
        context.fill();
      };
    case "strip":
      return (context) => {
        if (shape.points.length < 2) return;
        context.strokeStyle = ink(shape.colour);
        context.lineWidth = shape.stroke;
        context.lineCap = "round";
        context.lineJoin = "round";
        context.beginPath();
        context.moveTo(shape.points[0]!.x, shape.points[0]!.y);
        for (const at of shape.points.slice(1)) context.lineTo(at.x, at.y);
        context.stroke();
      };
    case "sprite":
      return (context) => {
        const image = sources.image(shape.atlas);
        context.save();
        context.translate(shape.x, shape.y);
        context.rotate(shape.angle);
        if (image) context.drawImage(image, -shape.w / 2, -shape.h / 2, shape.w, shape.h);
        else {
          context.fillStyle = shape.tint === 0xffffffff ? ["#87ceeb", "#ffa500", "#55aa55"][shape.atlas] ?? "#888888" : ink(shape.tint);
          context.fillRect(-shape.w / 2, -shape.h / 2, shape.w, shape.h);
        }
        context.restore();
      };
    case "glyphs":
      return (context) => {
        context.fillStyle = ink(shape.colour);
        context.font = `${shape.size}px system-ui, sans-serif`;
        context.textBaseline = "top";
        context.textAlign = "left";
        context.fillText(shape.text, shape.x, shape.y);
      };
    case "lettering":
      return (context) => {
        context.fillStyle = ink(shape.colour);
        context.font = `${shape.weight} ${shape.size}px system-ui, sans-serif`;
        context.textBaseline = "top";
        context.textAlign = shape.anchor === 1 ? "center" : shape.anchor === 2 ? "right" : "left";
        context.fillText(shape.text, shape.x, shape.y);
      };
    case "path":
      return (context) => {
        context.save();
        if (shape.transform) {
          context.translate(shape.transform.x, shape.transform.y);
          context.scale(shape.transform.scale, shape.transform.scale);
        }
        steps(context, shape);
        outline(context, shape);
        context.restore();
      };
  }
}

/** Draws one scene at `transform`; sprites come from `sources`. */
export function paintScene(canvas: HTMLCanvasElement, scene: Scene, transform: SceneTransform, sources: SceneImages): void {
  const context = canvas.getContext("2d");
  if (!context) return;
  const ratio = window.devicePixelRatio || 1;
  canvas.width = Math.max(1, Math.round(canvas.clientWidth * ratio));
  canvas.height = Math.max(1, Math.round(canvas.clientHeight * ratio));
  context.setTransform(1, 0, 0, 1, 0, 0);
  context.clearRect(0, 0, canvas.width, canvas.height);
  context.save();
  context.setTransform(ratio * transform.scale, 0, 0, ratio * transform.scale, 0, 0);
  if (scene.width > 0) {
    context.beginPath();
    context.rect(0, 0, scene.width, scene.height);
    context.clip();
  }
  const ordered = scene.shapes.map((shape, at) => ({ shape, at }))
    .sort((left, right) => left.shape.depth - right.shape.depth || left.at - right.at);
  for (const { shape } of ordered) painterOf(shape, sources)(context);
  context.restore();
}

/** A described element's bounds in pixels, for the invisible controls over a scene. */
export function accessBounds(access: Access, transform: SceneTransform): { x: number; y: number; w: number; h: number } {
  return { x: access.x * transform.scale, y: access.y * transform.scale, w: access.w * transform.scale, h: access.h * transform.scale };
}

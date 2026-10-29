// [LESSONS-FRAMEWORK] Native media is application-authored content. Hosts
// resolve approved assets; media never creates a control binding or autoplays.
import { escapeHtml } from "./html";

type Node = Record<string, unknown>;
type Resolve = ((source: string) => string) | undefined;

function text(node: Node, key: string): string {
  const value = node[key];
  if (typeof value !== "string" || value.trim() === "") throw new Error(`Media ${key} must be non-empty text`);
  return value;
}

function positive(node: Node, key: string): number {
  const value = node[key];
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value <= 0) throw new Error(`Media ${key} must be a positive integer`);
  return value;
}

function asset(node: Node, key: string, resolve: Resolve): string {
  if (!resolve) throw new Error("Media requires a host asset resolver");
  const value = resolve(text(node, key));
  if (!/^(https?:|file:|blob:|data:(image|video|text\/vtt)[/;,]|vscode-resource:|vscode-webview-resource:)/i.test(value)) {
    throw new Error("The host returned an unsupported media asset URI");
  }
  return escapeHtml(value);
}

/** An accessible still picture, with intrinsic dimensions to reserve its layout before loading. */
export function renderImage(node: Node, resolve: Resolve): string {
  return `<img class="osp-media-image" src="${asset(node, "source", resolve)}" alt="${escapeHtml(text(node, "alt"))}" width="${positive(node, "width")}" height="${positive(node, "height")}" loading="lazy" decoding="async" />`;
}

/** A user-started film with a poster and an enabled caption track. */
export function renderVideo(node: Node, resolve: Resolve): string {
  const source = asset(node, "source", resolve), poster = asset(node, "poster", resolve), captions = asset(node, "captions", resolve);
  const label = escapeHtml(text(node, "alt"));
  const identity = escapeHtml(JSON.stringify([source, poster, captions]));
  return `<video class="osp-media-video" controls playsinline preload="none" src="${source}" poster="${poster}" aria-label="${label}" data-media-key="${identity}" data-duration="${positive(node, "seconds")}"><track kind="captions" label="Captions" src="${captions}" default />${label}</video>`;
}

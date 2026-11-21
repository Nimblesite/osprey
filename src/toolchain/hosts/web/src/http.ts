// The `http` capability in the browser ([UI-EFFECTS]): HTTPS, or HTTP to this page's own origin or
// to this machine. No credentials, no cache, no redirects followed, a deadline and a response
// limit. The receipt carries the status; the body lands in the slice the request named.

import type { Ask, Completion } from "../../../vscode-extension/client/src/osprey-ui/asks";

function allowed(url: URL): boolean {
  const loopback = ["localhost", "127.0.0.1", "[::1]"].includes(url.hostname);
  return url.protocol === "https:" || url.origin === location.origin || (url.protocol === "http:" && loopback);
}

async function bounded(response: Response, limit: number): Promise<string> {
  const bytes = new Uint8Array(await response.arrayBuffer());
  if (bytes.length > limit) throw new Error(`${bytes.length} bytes exceeds the ${limit} byte limit`);
  return new TextDecoder().decode(bytes);
}

/** Perform one `http` ask and complete it with its status, holding the body in its slice. */
export async function fetched(ask: Ask): Promise<Completion> {
  if (ask.kind !== "http") throw new Error(`This host answers no ${ask.kind} requests`);
  const url = new URL(ask.text(2), location.href);
  if (!allowed(url)) throw new Error("Only https URLs, or http on this machine, are allowed");
  const method = ask.text(1) === "" ? "GET" : ask.text(1);
  const body = ask.text(4);
  const headers = Object.fromEntries(ask.pairs(3).map((pair) => [pair.name, pair.value]));
  const signal = AbortSignal.timeout(Math.max(ask.number(6), 1));
  const response = await fetch(url, { method, headers, body: body === "" ? undefined : body, cache: "no-store",
    credentials: "omit", redirect: "manual", signal });
  const text = await bounded(response, Math.max(ask.number(5), 0));
  return { detail: JSON.stringify({ status: response.status, headers: [] }), slices: [{ key: ask.text(7), text }] };
}

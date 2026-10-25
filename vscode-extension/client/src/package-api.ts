// Implements [PACKAGE-API]: bounded, read-only registry discovery for the editor.
import * as http from "http";
import * as https from "https";

export function registryUrl(origin: string): URL {
  const url = new URL(origin);
  const loopback = ["127.0.0.1", "localhost", "[::1]"].includes(url.hostname);
  if ((url.protocol !== "https:" && !(url.protocol === "http:" && loopback)) ||
      url.username || url.password || url.pathname !== "/" || url.search || url.hash) {
    throw new Error("Package registry must be an HTTPS origin (HTTP is allowed only on loopback).");
  }
  url.pathname = "/v1/packages";
  return url;
}

export function fetchListingText(origin: string, limits?: { timeoutMs?: number; maxBytes?: number }): Promise<string>;
export function fetchListingText(origin: string, limits: { timeoutMs?: number; maxBytes?: number } = {}): Promise<string> {
  const url = registryUrl(origin);
  return new Promise((resolve, reject) => {
    const request = (url.protocol === "https:" ? https : http).get(url, { headers: { Accept: "application/json" } }, (response) => {
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`Package registry answered HTTP ${response.statusCode}.`));
      } else readBody(response, limits.maxBytes ?? 1024 * 1024).then(resolve, reject);
    });
    const timer = setTimeout(() => request.destroy(new Error("Package registry request timed out.")), limits.timeoutMs ?? 15_000);
    request.once("close", () => clearTimeout(timer));
    request.once("error", reject);
  });
}

function readBody(response: http.IncomingMessage, maxBytes: number): Promise<string> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    let bytes = 0;
    response.on("data", (chunk: Buffer) => {
      bytes += chunk.length;
      if (bytes > maxBytes) response.destroy(new Error("Package registry response is too large."));
      else chunks.push(chunk);
    });
    response.once("end", () => resolve(Buffer.concat(chunks).toString("utf8")));
    response.once("error", reject);
    response.once("aborted", () => reject(new Error("Package registry response was interrupted.")));
  });
}

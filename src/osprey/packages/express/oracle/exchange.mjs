// One request over real loopback HTTP, and the parts of its reply that the
// port is compared on. Shared by the recorder and the end-to-end test, so both
// servers are asked and read in exactly the same way. [TS-PORT-EXPRESS-ORACLE]
import http from "node:http";

/** The reply headers compared: content type and the security-relevant set. */
export const COMPARED_HEADERS = [
  "content-type",
  "content-length",
  "x-powered-by",
  "content-security-policy",
  "x-content-type-options",
];

/** Send `request` to 127.0.0.1:`port`; resolve with status, compared headers and body. */
export function exchange(port, request) {
  return new Promise((resolve, reject) => {
    const options = { host: "127.0.0.1", port, method: request.method, path: request.path, agent: false };
    const sent = http.request(options, (reply) => {
      const chunks = [];
      reply.on("data", (chunk) => chunks.push(chunk));
      reply.on("end", () => resolve(summary(reply, Buffer.concat(chunks))));
    });
    sent.on("error", reject);
    sent.end(request.body ?? "");
  });
}

function summary(reply, body) {
  const headers = Object.fromEntries(
    COMPARED_HEADERS.filter((name) => reply.headers[name] !== undefined).map((name) => [name, reply.headers[name]]),
  );
  return { status: reply.statusCode, headers, etag: reply.headers.etag !== undefined, body: body.toString("utf8") };
}

/** Resolve once something accepts connections on `port`, or reject after `attempts` tries. */
export async function listening(port, attempts = 200) {
  for (let attempt = 0; attempt < attempts; attempt += 1) {
    const open = await new Promise((resolve) => {
      const probe = http.request({ host: "127.0.0.1", port, method: "OPTIONS", path: "/", agent: false }, (reply) => {
        reply.resume();
        resolve(true);
      });
      probe.on("error", () => resolve(false));
      probe.end();
    });
    if (open) return;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  throw new Error(`nothing listened on port ${port}`);
}

import * as assert from "assert";
import * as http from "http";
import { fetchListingText, registryUrl } from "../../client/src/package-api";

suite("package API transport", () => {
  test("requires an HTTPS origin except for loopback development", () => {
    assert.strictEqual(registryUrl("https://packages.example").href, "https://packages.example/v1/packages");
    for (const url of ["http://packages.example", "https://user:secret@example.com", "https://example.com/path", "file:///tmp", "https://example.com/?token=x"]) {
      assert.throws(() => registryUrl(url), /registry/i, url);
    }
  });
  test("reads the actual API listing over loopback HTTP", async () => {
    const body = JSON.stringify({ packages: [{ key: "@my-org/helper", releases: [] }] });
    await withServer((_req, response) => response.end(body), async (origin) => {
      assert.strictEqual(await fetchListingText(origin), body);
    });
  });
  test("HTTP errors and redirects are never interpreted as package lists", async () => {
    for (const status of [302, 401, 403, 500]) {
      await withServer((_req, response) => { response.statusCode = status; response.end("denied"); },
        async (origin) => assert.rejects(fetchListingText(origin), new RegExp(String(status))));
    }
  });
  test("oversized responses and stalled requests terminate", async () => {
    await withServer((_req, response) => response.end("x".repeat(1025)),
      async (origin) => assert.rejects(fetchListingText(origin, { maxBytes: 1024 }), /large/));
    await withServer(() => undefined,
      async (origin) => assert.rejects(fetchListingText(origin, { timeoutMs: 30 }), /timed out/));
  });
});
async function withServer(handler: http.RequestListener, run: (origin: string) => Promise<unknown>) {
  const server = http.createServer(handler);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address() as { port: number };
  try { await run(`http://127.0.0.1:${address.port}`); }
  finally { server.closeAllConnections(); await new Promise<void>((resolve) => server.close(() => resolve())); }
}

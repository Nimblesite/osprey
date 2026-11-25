import * as assert from "assert";
import { registryUrl } from "../../client/src/package-api";

suite("package registry origin validation", () => {
  test("requires an HTTPS origin except for loopback development", () => {
    assert.strictEqual(registryUrl("https://packages.example").href, "https://packages.example/v1/packages");
    for (const url of ["http://packages.example", "https://user:secret@example.com", "https://example.com/path", "file:///tmp", "https://example.com/?token=x"]) {
      assert.throws(() => registryUrl(url), /registry/i, url);
    }
  });
});

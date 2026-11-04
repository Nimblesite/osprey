// The listing and manifest surfaces the QuickPicks are built from
// ([PACKAGE-FOLDER-REGISTRY] `GET /v1/packages`, [PACKAGE-MANIFEST]
// dependency lines). The registry listing carries NO security-audit evidence,
// so every entry must be presented as unverified — an entry without evidence
// must never read as safe.

import * as assert from "assert";
import {
  dependencyKeys,
  isOrgQualifiedName,
  listingOf,
  newestRelease,
  parseListing,
  securityDetail,
} from "../../client/src/package-listing";

const listing = JSON.stringify({
  packages: [
    { key: "@demo/pyadd", releases: [{ digest: "aa", epoch: 1, ordinal: 2 }, { digest: "ab", epoch: 2, ordinal: 0 }] },
    { key: "@other/util", releases: [{ digest: "cc", epoch: 1, ordinal: 5 }] },
    { key: "@demo/util", releases: [{ digest: "dd", epoch: 3, ordinal: 1 }] },
  ],
});

suite("package listing", () => {
  test("parses org-qualified keys with their releases", () => {
    const entries = parseListing(listing) ?? [];
    assert.deepStrictEqual(entries.map((e) => e.key), ["@demo/pyadd", "@other/util", "@demo/util"]);
    assert.strictEqual(newestRelease(entries[0]).epoch, 2);
  });

  test("duplicate leaf names under distinct orgs stay distinct entries", () => {
    const entries = listingOf(parseListing(listing));
    const leaves = entries.filter((e) => e.key.endsWith("/util"));
    assert.deepStrictEqual(leaves.map((e) => e.key).sort(), ["@demo/util", "@other/util"]);
  });

  test("a listing whose only entry is unscoped yields nothing to offer", () => {
    for (const text of [
      '{"packages":[{"key":"util","releases":[{ "digest":"d","epoch":1,"ordinal":1}]}]}',
      '{"packages":[{"key":"@a/b"}]}',
      '{"packages":[{"key":"@a/b","releases":[{"epoch":1}]}]}',
    ]) {
      assert.deepStrictEqual(parseListing(text), [], text);
    }
  });

  test("unscoped, malformed or wrong-shape listings yield nothing to show", () => {
    for (const text of [
      "",
      "not json",
      "{}",
      '{"packages": {}}',
    ]) {
      assert.strictEqual(parseListing(text), undefined, text);
    }
  });

  test("an unscoped name can never be an org-qualified identity", () => {
    assert.strictEqual(isOrgQualifiedName("@demo/pyadd"), true);
    for (const bad of ["util", "@demo", "demo/util", "@demo/util/extra", "@/util", "@/", "@Demo/util", "@demo/a b", "@demo/--", "@-demo/util", "@demo/util-", `@${"a".repeat(40)}/util`, `@demo/${"a".repeat(64)}`]) {
      assert.strictEqual(isOrgQualifiedName(bad), false, bad);
    }
  });

  test("every entry warns that security evidence is unavailable", () => {
    const entries = parseListing(listing) ?? [];
    for (const entry of entries) {
      const detail = securityDetail(entry);
      assert.ok(detail.includes("No security audit evidence"), detail);
      assert.ok(!/safe|verified clean|trusted/i.test(detail), `reads as safe: ${detail}`);
    }
  });
});

suite("direct dependency reports", () => {
  test("direct dependencies come from the canonical client report", () => {
    assert.deepStrictEqual(dependencyKeys({ status: 0, lines: ["@demo/pyadd", "@other/util", "@demo/util"] }),
      ["@demo/pyadd", "@other/util", "@demo/util"]);
  });
  test("a manifest without dependencies reports an empty list", () => {
    assert.deepStrictEqual(dependencyKeys({ status: 0, lines: [] }), []);
  });
  test("an unscoped identity or client failure cannot become a removal choice", () => {
    assert.strictEqual(dependencyKeys({ status: 0, lines: ["util", "@ok/one"] }), undefined);
    assert.strictEqual(dependencyKeys({ status: 1, lines: ["manifest.invalid"] }), undefined);
  });
});

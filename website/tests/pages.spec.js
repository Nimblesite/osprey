// Page-health checks for every template at every breakpoint:
// no JS errors, no failed requests, no horizontal overflow, dark theme intact,
// site chrome present, the logo image resolving, and Osprey code highlighted.
const fs = require("node:fs");
const path = require("node:path");
const { test, expect } = require("@playwright/test");
const { PAGES, VIEWPORTS, collectProblems, paintedColours } = require("./helpers");

const SITE = path.resolve(__dirname, "../_site");
const OSPREY_CODE = /<code([^>]*)class="language-osprey(?:-ml)?"[^>]*>([\s\S]*?)<\/code>/g;
const COMMENT_TOKEN = /<span class="token comment">[\s\S]*?<\/span>/g;
const OSPREY_BLOCKS = "pre > code.language-osprey, pre > code.language-osprey-ml";

const brokenSpecLinks = (sources) => {
  const origin = "https://spec.test";
  const pages = new Map(sources.map(([url, html]) => [url, new DOMParser().parseFromString(html, "text/html")]));
  return [...pages].flatMap(([from, document]) => [...document.querySelectorAll("a[href]")].flatMap((link) => {
    const url = new URL(link.getAttribute("href"), origin + from);
    if (url.origin !== origin || !url.pathname.startsWith("/spec/")) return [];
    const target = pages.get(url.pathname);
    const found = target && (!url.hash || target.getElementById(decodeURIComponent(url.hash.slice(1))));
    return found ? [] : [`${from} -> ${url.pathname}${url.hash}`];
  }));
};

test("every specification link and section anchor resolves", async ({ page }) => {
  const subjects = fs.readdirSync(path.join(SITE, "spec"), { withFileTypes: true }).filter((entry) => entry.isDirectory());
  const sources = ["", ...subjects.map((entry) => entry.name)].map((slug) => [
    `/spec/${slug ? `${slug}/` : ""}`, fs.readFileSync(path.join(SITE, "spec", slug, "index.html"), "utf8"),
  ]);
  expect(sources.length).toBeGreaterThan(40);
  expect(await page.evaluate(brokenSpecLinks, sources)).toEqual([]);
});

test("specification links resolve across subject folders and duplicate URLs are rejected", () => {
  const { resolveLink, pagesFrom } = require("../scripts/copy-spec.js");
  const root = path.resolve("../docs/specs");
  const file = path.join(root, "platforms/0038-iOSTarget.md");
  const compiler = path.join(root, "compiler/iOSCodegen.md");
  const pages = new Map([[compiler, { permalink: "/spec/ioscodegen/" }]]);
  expect(resolveLink("../compiler/iOSCodegen.md#ios-host-abi", file, pages)).toBe("/spec/ioscodegen/#ios-host-abi");
  expect(resolveLink("../README.md", file, pages)).toBe("/spec/");
  expect(resolveLink("../../../crates/osprey-cli/src/ios.rs", file, pages)).toBe("https://github.com/Nimblesite/osprey/blob/main/crates/osprey-cli/src/ios.rs");
  expect(resolveLink("https://example.com/#host", file, pages)).toBe("https://example.com/#host");
  expect(() => resolveLink("../../../../outside", file, pages)).toThrow("Link escapes repository");
  const spec = { file, section: { slug: "platforms", title: "Platform integration" } };
  expect(() => pagesFrom([spec, spec])).toThrow("Duplicate specification URL");
});

test("specifications separate the language from hosts, tools and applications", async ({ page }) => {
  await page.goto("/spec/");
  for (const category of ["language", "compiler", "runtime", "platforms", "tooling", "ecosystem", "applications"]) {
    await expect(page.locator(`main a[href="/spec/${category}/"]`).first()).toBeVisible();
  }
  await page.goto("/spec/language/");
  await expect(page.locator('main a[href="/spec/0017-algebraiceffects/"]')).toBeVisible();
  await expect(page.locator('main a[href="/spec/0033-ospreygithubaction/"], main a[href="/spec/0038-iostarget/"], main a[href="/spec/0039-androidtarget/"]')).toHaveCount(0);
  await page.goto("/spec/platforms/");
  await expect(page.locator('main a[href="/spec/0038-iostarget/"]')).toBeVisible();
  await expect(page.locator('main a[href="/spec/0039-androidtarget/"]')).toBeVisible();
  await page.goto("/spec/0038-iostarget/");
  await expect(page.locator('main a[href="/spec/ioscodegen/"]')).toBeVisible();
});

// A block is unhighlighted when it has no tokens or an ML `(* … *)` comment
// outside a comment token. The /wasm/ studio fills #src-code at runtime; the
// browser check below waits for its tokens.
const unhighlightedBlocks = (file) =>
  [...fs.readFileSync(path.join(SITE, file), "utf8").matchAll(OSPREY_CODE)]
    .filter(([, attributes, body]) => !attributes.includes('id="src-code"'))
    .filter(([, , body]) => !body.includes('class="token') || body.replace(COMMENT_TOKEN, "").replace(/<[^>]+>/g, "").includes("(*"))
    .map(([, , body]) => `${file}: ${body.slice(0, 60)}`);

test("every built page ships its Osprey code highlighted", () => {
  const pages = fs.readdirSync(SITE, { recursive: true }).filter((file) => file.endsWith(".html"));
  expect(pages).toContain("index.html");
  expect(pages.flatMap(unhighlightedBlocks)).toEqual([]);
});

const DARK_BACKGROUNDS = ["rgb(12, 19, 37)", "rgb(7, 13, 31)", "rgb(25, 31, 50)"];

for (const pg of PAGES) {
  test.describe(`page: ${pg.name}`, () => {
    test(`loads cleanly with no errors (${pg.path})`, async ({ page }) => {
      const problems = collectProblems(page, pg.name);
      const resp = await page.goto(pg.path, { waitUntil: "networkidle" });
      expect(resp.status(), "HTTP status").toBeLessThan(400);
      await page.waitForTimeout(300);

      expect(problems.pageErrors, "uncaught page errors").toEqual([]);
      expect(problems.consoleErrors, "console errors").toEqual([]);
      expect(problems.failedRequests, "failed requests").toEqual([]);
    });

    test(`has dark theme + header + footer (${pg.path})`, async ({ page }) => {
      await page.goto(pg.path, { waitUntil: "domcontentloaded" });
      const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
      expect(DARK_BACKGROUNDS, `body background was ${bg}`).toContain(bg);

      await expect(page.locator(".site-header")).toBeVisible();
      // The theme toggle must stay hidden — the design is dark-only.
      await expect(page.locator(".theme-toggle")).toBeHidden();
      // Full-screen app-like pages (playground, wasm studio) hide the footer.
      if (pg.kind !== "app" && !pg.fullscreen) {
        await expect(page.locator(".site-footer")).toBeVisible();
      }
    });

    test(`logo image resolves (${pg.path})`, async ({ page }) => {
      await page.goto(pg.path, { waitUntil: "domcontentloaded" });
      const bgImage = await page.evaluate(() =>
        getComputedStyle(document.querySelector(".logo"), "::before").backgroundImage
      );
      const match = bgImage.match(/url\(["']?([^"')]+)["']?\)/);
      expect(match, "logo ::before background-image url").toBeTruthy();
      const res = await page.request.get(match[1]);
      expect(res.status(), "logo request status").toBe(200);
    });

    test(`Osprey code paints its tokens (${pg.path})`, async ({ page }) => {
      await page.goto(pg.path, { waitUntil: "domcontentloaded" });
      for (const block of await page.locator(OSPREY_BLOCKS).all()) {
        await expect(block.locator(".token").first()).toBeAttached();
        expect(await paintedColours(block.or(block.locator(".token"))), await block.textContent()).toBeGreaterThan(1);
      }
    });

    // The playground is a fixed-height Monaco app — horizontal overflow there
    // is owned by the editor, not the site layout.
    if (pg.kind !== "app") {
      for (const vp of VIEWPORTS) {
        test(`no horizontal overflow @ ${vp.name} (${pg.path})`, async ({ page }) => {
          await page.setViewportSize({ width: vp.width, height: vp.height });
          await page.goto(pg.path, { waitUntil: "networkidle" });
          await page.waitForTimeout(200);
          const overflow = await page.evaluate(
            () => document.documentElement.scrollWidth - window.innerWidth
          );
          expect(overflow, `document overflows by ${overflow}px`).toBeLessThanOrEqual(2);
        });
      }
    }
  });
}

const proseOffsets = (elements) => elements.map((element) => {
  const { x, width } = element.getBoundingClientRect();
  return Math.abs(x + width / 2 - window.innerWidth / 2);
});

for (const width of [1280, 1440, 1920]) {
  test(`benchmark columns fit desktop landscape at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/benchmarks/");
    await page.evaluate(() => document.fonts.ready);
    const prose = page.locator(".prose > :is(h1, h2, h3, p, ul, ol, pre)");
    const centers = await prose.evaluateAll(proseOffsets);
    expect(Math.max(...centers), "benchmark prose must be centered in the viewport").toBeLessThanOrEqual(2);
    const tables = page.locator(".comparison-table table");
    await expect(tables).toHaveCount(2);
    expect(await tables.first().evaluate((table) => table.clientWidth / table.closest("article").clientWidth)).toBeGreaterThan(0.95);
    const overflow = await tables.evaluateAll((elements) => elements.map((element) => element.scrollWidth - element.clientWidth));
    expect(overflow, "every benchmark column must fit without horizontal scrolling").toEqual([0, 0]);
    for (const table of await tables.all()) {
      const bounds = await table.locator("th:last-child").boundingBox();
      expect(bounds.x).toBeGreaterThanOrEqual(0);
      expect(bounds.x + bounds.width).toBeLessThanOrEqual(width);
    }
  });
}

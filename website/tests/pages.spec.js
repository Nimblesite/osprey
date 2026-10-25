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

async function expectNavigationFits(links) {
  for (const link of await links.all()) await expect(link).toBeInViewport({ ratio: 1 });
  const overflowing = await links.evaluateAll((elements) => elements
    .filter((element) => element.scrollWidth > element.clientWidth)
    .map((element) => element.textContent.trim()));
  expect(overflowing, "navigation text must fit inside each link's target").toEqual([]);
}

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

test.describe("mobile interactions", () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test("mobile menu opens and closes", async ({ page }) => {
    await page.goto("/");
    const toggle = page.locator("#mobile-menu-toggle");
    const links = page.locator(".nav-links");
    await expect(toggle).toBeVisible();
    await expect(links).toBeHidden();
    await toggle.click();
    await expect(links).toBeVisible();
    await expect(links.locator("summary", { hasText: "Docs" })).toBeVisible();
    await toggle.click();
    await expect(links).toBeHidden();
  });

  test("mobile menu link navigates", async ({ page }) => {
    await page.goto("/");
    await page.locator("#mobile-menu-toggle").click();
    await page.locator(".nav-link", { hasText: "Blog" }).click();
    await expect(page).toHaveURL(/\/blog\/$/);
  });

  for (const viewport of [{ width: 320, height: 568 }, { width: 768, height: 320 }, { width: 768, height: 320, font: "monospace" }]) {
    test(`mobile menu links fit within ${viewport.width}x${viewport.height}${viewport.font ? " with fallback font" : ""}`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.goto("/docs/");
      if (viewport.font) await page.addStyleTag({ content: `:root { --font-sans: ${viewport.font}; }` });
      await page.locator("#mobile-menu-toggle").click();
      const controls = page.locator("#site-navigation > li > a, #site-navigation > li > details > summary");
      await expect(controls).toHaveCount(5);
      await expectNavigationFits(controls);
      for (const group of await page.locator(".nav-group").all()) {
        await group.locator("summary").click();
        await expectNavigationFits(group.getByRole("link"));
        await expectNavigationFits(controls);
      }
    });
  }

  test("hero actions stack full-width on mobile", async ({ page }) => {
    await page.goto("/");
    const widths = await page.evaluate(() => {
      const btns = [...document.querySelectorAll(".hero-actions .btn")];
      return { btn: btns[0].getBoundingClientRect().width, actions: document.querySelector(".hero-actions").getBoundingClientRect().width };
    });
    // Each button should span (nearly) the full actions row.
    expect(widths.btn).toBeGreaterThan(widths.actions * 0.9);
  });

  test("TOC sidebar is hidden on mobile", async ({ page }) => {
    await page.goto("/spec/0001-introduction/");
    await expect(page.locator(".toc-aside")).toBeHidden();
  });

  test("mobile menu supports keyboard dismissal and restores focus", async ({ page }) => {
    await page.goto("/");
    const toggle = page.locator("#mobile-menu-toggle");
    const links = page.locator(".nav-links");
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(toggle).toHaveAttribute("aria-controls", await links.getAttribute("id"));
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    await expect(links.locator("summary").first()).toBeFocused();
    await page.keyboard.press("Escape");
    await expect(links).toBeHidden();
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(toggle).toBeFocused();
  });

  test("mobile menu closes outside the header and resets on resize", async ({ page }) => {
    await page.goto("/docs/");
    const toggle = page.locator("#mobile-menu-toggle");
    const links = page.locator(".nav-links");
    await toggle.click();
    await page.mouse.click(10, 800);
    await expect(links).toBeHidden();
    await toggle.click();
    await page.setViewportSize({ width: 1440, height: 900 });
    await expect(links).toBeVisible();
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(links).toBeHidden();
  });
});

const DOC_SECTIONS = ["Start here", "Tutorials", "Build applications", "Packages", "Language", "Reference"];

test.describe("documentation sub-page navigation", () => {
  test("groups subpages and identifies the current desktop page", async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto("/docs/web-apps/");
    await expect(page.locator(".prose-layout .prose")).toBeVisible();
    const navigation = page.locator(".toc-aside .docs-nav-desktop");
    await expect(navigation).toBeVisible();
    await expect(navigation.locator(".docs-nav-section-title")).toHaveText(DOC_SECTIONS);
    await expect(navigation.getByRole("link", { name: "Web apps", exact: true })).toHaveClass(/active/);
  });

  for (const height of [600, 900]) {
    test(`desktop docs navigation scrolls with the document at ${height}px`, async ({ page }) => {
      await page.setViewportSize({ width: 1440, height });
      await page.goto("/docs/web-apps/");
      const navigation = page.locator(".docs-nav-desktop");
      await navigation.getByRole("link").first().hover();
      await page.mouse.wheel(0, 400);
      await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(100);
      expect(await navigation.evaluate((nav) => nav.scrollTop)).toBe(0);
      const lastLink = navigation.getByRole("link", { name: "Operators", exact: true });
      await lastLink.scrollIntoViewIfNeeded();
      await expect(lastLink).toBeInViewport();
      expect(await navigation.evaluate((nav) => nav.scrollHeight - nav.clientHeight)).toBe(0);
    });
  }

  test("mobile docs groups expose the current section and expand on demand", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await page.goto("/docs/web-apps/");
    const navigation = page.locator(".docs-nav-mobile");
    await expect(navigation).toBeVisible();
    await navigation.locator(":scope > summary").click();
    await expect(navigation.locator(".docs-nav-section-title")).toHaveText(DOC_SECTIONS);
    await expect(navigation.getByRole("link", { name: "Web apps", exact: true })).toBeVisible();
    const firstApp = navigation.getByRole("link", { name: "My First App", exact: true });
    await expect(firstApp).toBeHidden();
    await navigation.locator("summary", { hasText: "Start here" }).click();
    await expect(firstApp).toBeVisible();
    await firstApp.click();
    await expect(page).toHaveURL(/\/docs\/my-first-app\/$/);
  });

  test("mobile reference navigation works without JavaScript", async ({ browser }) => {
    const context = await browser.newContext({ javaScriptEnabled: false, viewport: { width: 390, height: 844 } });
    const page = await context.newPage();
    await page.goto("/docs/functions/map/");
    const navigation = page.locator(".docs-nav-mobile");
    await navigation.locator(":scope > summary").click();
    const functions = navigation.getByRole("link", { name: "Functions", exact: true });
    await expect(functions).toBeVisible();
    await expect(functions).toHaveAttribute("aria-current", "location");
    await navigation.locator("summary", { hasText: "Packages" }).click();
    await navigation.getByRole("link", { name: "Package manager", exact: true }).click();
    await expect(page).toHaveURL(/\/docs\/package-manager\/$/);
    await context.close();
  });

  test("keeps the single-file first app tutorial in Default flavor", async ({ page }) => {
    await page.goto("/docs/my-first-app/");
    await expect(page.locator('.prose pre[data-flavor="default"]')).not.toHaveCount(0);
    await expect(page.locator('.prose pre[data-flavor="ml"]')).toHaveCount(0);
  });
});

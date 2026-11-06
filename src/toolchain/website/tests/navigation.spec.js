// The one documentation side panel: grouped sections, the current page, the
// page's own headings, independent desktop scrolling and the mobile menu.
const { test, expect } = require("@playwright/test");

const DOC_SECTIONS = ["Start here", "Python", "Tutorials", "Proofs", "Build applications", "Packages", "Language", "Reference"];

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

  test("the site Docs link opens the documentation index, whose sidebar holds every section", async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto("/");
    await page.locator("#site-navigation").getByRole("link", { name: "Docs", exact: true }).click();
    await expect(page).toHaveURL(/\/docs\/$/);
    await expect(page.locator(".docs-nav-desktop .docs-nav-section-title")).toHaveText(DOC_SECTIONS);
  });

  test("Reference lists benchmarks, releases and credits", async ({ page }) => {
    await page.setViewportSize({ width: 1440, height: 900 });
    await page.goto("/docs/releases/");
    const reference = page.locator(".docs-nav-desktop .docs-nav-section", { hasText: "Reference" });
    await expect(reference.getByRole("link", { name: "Benchmarks", exact: true })).toHaveAttribute("href", "/benchmarks/");
    await expect(reference.getByRole("link", { name: "Releases", exact: true })).toHaveAttribute("aria-current", "page");
    await expect(reference.getByRole("link", { name: "Credits", exact: true })).toHaveAttribute("href", "/credits/");
    await expect(page.locator("h1")).toHaveText("Releases");
    await expect(page.locator(".prose a[href='https://github.com/Nimblesite/osprey/releases']")).toHaveCount(1);
    await expect(page.locator("#site-navigation a.active")).toHaveText("Docs");
  });

  for (const height of [600, 900]) {
    test(`the one side panel stays in view and scrolls on its own at ${height}px`, async ({ page }) => {
      await page.setViewportSize({ width: 1440, height });
      await page.goto("/docs/effects/");
      const navigation = page.locator(".docs-nav-desktop");
      await expect(navigation.getByRole("link", { name: "Algebraic effects", exact: true })).toBeInViewport();
      await page.locator(".prose").hover();
      await page.mouse.wheel(0, 4000);
      await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(2000);
      await expect(navigation).toBeInViewport();
      const active = navigation.locator(".toc-link.active");
      await expect(active).toHaveCount(1);
      await expect(active).toBeInViewport();
      const pageScroll = await page.evaluate(() => window.scrollY);
      await navigation.hover();
      await page.mouse.wheel(0, 5000);
      await expect.poll(() => navigation.evaluate((nav) => nav.scrollTop + nav.clientHeight >= nav.scrollHeight - 1)).toBe(true);
      expect(await page.evaluate(() => window.scrollY)).toBe(pageScroll);
      const lastLink = navigation.getByRole("link", { name: "Credits", exact: true });
      await expect(lastLink).toBeInViewport();
      await navigation.getByRole("link", { name: "Operators", exact: true }).click();
      await expect(page).toHaveURL(/\/docs\/operators\/$/);
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

  test("publishes the @osprey/json package API generated by the compiler", async ({ page }) => {
    await page.goto("/docs/package-manager/");
    const sidebar = page.locator(".docs-nav").first();
    await sidebar.getByRole("link", { name: "@osprey/json API", exact: true }).click();
    await expect(page).toHaveURL(/\/docs\/packages\/json\/$/);
    await expect(page.locator("h1")).toHaveText("@osprey/json API Reference");
    await expect(sidebar.getByRole("link", { name: "@osprey/json API", exact: true })).toHaveAttribute("aria-current", /.+/);
    await expect(page.locator(".prose h2")).toContainText(["ui", "ui::Doc"]);
    const parseRow = page.locator(".prose tr", { has: page.getByRole("link", { name: "parse", exact: true }) });
    await expect(parseRow).toContainText("Function");
    await expect(parseRow).toContainText("as JSON, or JsonNull when it is not JSON.");
    await expect(page.locator(".prose")).toContainText("Total readers and member builders");
    const links = page.locator(".prose a[href]");
    expect(await links.count()).toBeGreaterThan(20);
    for (const href of await links.evaluateAll((all) => all.map((a) => a.getAttribute("href")))) {
      expect(href).not.toMatch(/\.md$/);
      const response = await page.request.get(new URL(href, page.url()).href);
      expect(response.status(), href).toBe(200);
    }
    await page.locator(".prose h2").getByRole("link", { name: "ui::Doc", exact: true }).click();
    await expect(page).toHaveURL(/\/docs\/packages\/json\/ui-doc\/$/);
    await expect(page.locator("h1")).toHaveText("ui::Doc");
    await page.locator(".prose").getByRole("link", { name: "parse", exact: true }).first().click();
    await expect(page).toHaveURL(/\/docs\/packages\/json\/ui-doc-parse\/$/);
    await expect(page.locator(".prose pre").first()).toContainText("ui::Doc::parse : string -> Json");
    await expect(page.locator(".prose")).toContainText("Defined in");
  });

  test("keeps the single-file first app tutorial in Default flavor", async ({ page }) => {
    await page.goto("/docs/my-first-app/");
    await expect(page.locator('.prose pre[data-flavor="default"]')).not.toHaveCount(0);
    await expect(page.locator('.prose pre[data-flavor="ml"]')).toHaveCount(0);
  });
});

test("every long-form page carries the same single side panel; only documentation pages list the documentation", async ({ page }) => {
  for (const [width, panel] of [[1440, ".docs-nav-desktop"], [390, ".docs-nav-mobile"]]) {
    await page.setViewportSize({ width, height: 900 });
    for (const url of ["/docs/", "/docs/effects/", "/spec/0001-introduction/", "/credits/", "/status/", "/docs/packages/json/"]) {
      await page.goto(url);
      await expect(page.locator(".toc-aside"), url).toHaveCount(1);
      await expect(page.locator(".toc, .toc-title"), url).toHaveCount(0);
      await expect(page.locator(`${panel} .docs-nav-section-title`).filter({ hasText: "Reference" }), url).toHaveCount(1);
    }
    for (const url of ["/blog/2026-11-03-osprey-agent-and-pydantic-ai-port/", "/blog/2026-10-05-effect-handlers-you-can-call/", "/blog/2026-10-25-python-packages-osprey-uv/"]) {
      await page.goto(url);
      await expect(page.locator(".toc-aside"), url).toHaveCount(1);
      await expect(page.locator(".toc-aside .docs-nav-section-title"), url).toHaveText(["On this page", "On this page"]);
      await expect(page.locator(".toc-aside").getByText(/Start here|Documentation|Browse documentation|Mathlib/), url).toHaveCount(0);
      if (width < 1024) await page.locator(".docs-nav-mobile > summary").click();
      const first = page.locator(`${panel} .toc-link`).first();
      await expect(first, url).toBeVisible();
      const target = (await first.getAttribute("href")).slice(1);
      await first.click();
      await expect(page.locator(`${panel} .toc-link.active`), url).toHaveAttribute("href", `#${target}`);
    }
  }
});

for (const [width, height, panel] of [[1440, 900, ".docs-nav-desktop"], [390, 844, ".docs-nav-mobile"]]) {
  test(`choosing a heading in the side panel selects it at ${width}px`, async ({ browser }) => {
    const context = await browser.newContext({ viewport: { width, height }, hasTouch: width < 1024 });
    const page = await context.newPage();
    await page.goto("/docs/my-first-app/");
    const names = ["Model JSON as data", "Encode every case", "Decode at the boundary", "Run it twice", "Create the app", "Update and save"];
    for (const name of names) {
      if (panel === ".docs-nav-mobile") await page.locator(".docs-nav-mobile > summary").click();
      const link = page.locator(`${panel} .toc-link`, { hasText: name });
      await (width < 1024 ? link.tap() : link.click());
      await expect(page.locator(`${panel} .toc-link.active`), `${name} is selected`).toHaveText(name);
      const id = (await link.getAttribute("href")).slice(1);
      // The heading settles in view, below the sticky header and (on mobile) the sticky menu bar.
      const clear = await page.evaluate(() => Math.max(...[".site-header", ".toc-aside"].map((s) => {
        const r = document.querySelector(s).getBoundingClientRect();
        return ["sticky", "fixed"].includes(getComputedStyle(document.querySelector(s)).position) && r.bottom < 200 ? r.bottom : 0;
      })));
      await expect.poll(() => page.locator(`#${id}`).evaluate((h, [top, bottom]) => {
        const r = h.getBoundingClientRect();
        return Math.round(r.top) >= top && r.bottom <= bottom;
      }, [Math.floor(clear), height]), `${name} settles in view`).toBe(true);
    }
    // Scrolling by hand after a choice hands selection back to the scroll position.
    await page.mouse.move(width / 2, height / 2);
    await page.mouse.wheel(0, -400);
    await page.keyboard.press("Home");
    await expect(page.locator(`${panel} .toc-link.active`)).toHaveText("Create the app");
    await context.close();
  });
}

test("the docs index shows the Prism Synth app under Build applications, not under proofs", async ({ page }) => {
  await page.goto("/docs/");
  const section = (img) => img.evaluate((el) => {
    let node = el.closest("figure") || el;
    while (node && node.tagName !== "H2") node = node.previousElementSibling || node.parentElement;
    return node && node.textContent.trim();
  });
  const prism = page.locator('article img[src*="macos-synth"]');
  await expect(prism).toHaveCount(1);
  expect(await section(prism)).toBe("Build applications");
});

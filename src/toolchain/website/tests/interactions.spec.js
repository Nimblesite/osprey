const { test, expect } = require("@playwright/test");
const { paintedColours, openInstallation } = require("./helpers");

const hero = require("../src/_data/heroExample.json");

test.describe("desktop interactions", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("nav links navigate to the right pages", async ({ page }) => {
    await page.goto("/");
    // Docs, Playground, Blog, GitHub; Discord is the button beside them.
    await expect(page.locator("#site-navigation > li")).toHaveCount(4);
    await page.locator("#site-navigation").getByRole("link", { name: "Docs", exact: true }).click();
    await expect(page).toHaveURL(/\/docs\/$/);
    await page.locator("#site-navigation").getByRole("link", { name: "Blog", exact: true }).click();
    await expect(page).toHaveURL(/\/blog\/$/);
  });

  test("Docs is one link, active on every documentation page, and Discord is the header's button", async ({ page }) => {
    for (const url of ["/docs/", "/spec/language/", "/docs/releases/", "/credits/", "/benchmarks/"]) {
      await page.goto(url);
      await expect(page.locator(".nav-group, #site-navigation summary")).toHaveCount(0);
      const docs = page.locator("#site-navigation").getByRole("link", { name: "Docs", exact: true });
      await expect(docs).toHaveAttribute("href", "/docs/");
      await expect(docs, url).toHaveClass(/active/);
    }
    for (const [url, label] of [["/playground/", "Playground"], ["/blog/", "Blog"], ["/", null]]) {
      await page.goto(url);
      const active = page.locator("#site-navigation a.active");
      if (label) await expect(active, url).toHaveText(label);
      else await expect(active, url).toHaveCount(0);
    }
    const discord = page.locator(".nav-actions .nav-cta");
    await expect(discord).toHaveText("Discord");
    await expect(discord).toHaveAttribute("href", "https://discord.gg/EGk3XhcGyy");
    const [cta, links] = await Promise.all([discord.boundingBox(), page.locator("#site-navigation").boundingBox()]);
    expect(cta.x, "Discord sits right of the menu").toBeGreaterThan(links.x + links.width);
  });

  test("header links work without JavaScript", async ({ browser }) => {
    for (const width of [1440, 390]) {
      const context = await browser.newContext({ javaScriptEnabled: false, viewport: { width, height: 900 } });
      const page = await context.newPage();
      await page.goto("/");
      await page.locator("#site-navigation").getByRole("link", { name: "Docs", exact: true }).click();
      await expect(page).toHaveURL(/\/docs\/$/);
      await context.close();
    }
  });

  test("logo returns to home", async ({ page }) => {
    await page.goto("/spec/0001-introduction/");
    await page.locator(".logo").click();
    await expect(page).toHaveURL(/\/$/);
    await expect(page.locator(".hero")).toBeVisible();
  });

  test("hero fills the viewport before scroll", async ({ page }) => {
    await page.goto("/");
    const { heroH, winH } = await page.evaluate(() => ({
      heroH: document.querySelector(".hero").getBoundingClientRect().height,
      winH: window.innerHeight,
    }));
    // Hero + the in-flow sticky header should cover the viewport.
    expect(heroH).toBeGreaterThanOrEqual(winH - 70);
    // The next section must start below the fold.
    const nextTop = await page.evaluate(
      () => document.querySelector("#showcase").getBoundingClientRect().top
    );
    expect(nextTop).toBeGreaterThanOrEqual(winH - 5);
  });

  test("hero has a title and two working CTAs", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".hero-title")).toContainText("Safe, elegant, fast.");
    await expect(page.locator(".hero-title")).not.toContainText("Logic");
    await expect(page.locator(".hero-actions .btn")).toHaveCount(2);
    await expect(page.locator(".home > section").nth(1)).toHaveId("showcase");
    await expect(page.locator(".home > section").nth(2)).toHaveClass(/home-install/);
    await openInstallation(page);
    await page.goto("/");
    await page.locator(".hero-actions .btn", { hasText: "Try Osprey Online" }).click();
    await expect(page).toHaveURL(/\/playground\/$/);
  });

  test("install is a compact strip that leads to the getting started guide", async ({ page }) => {
    await page.goto("/");
    const install = page.locator("#install");
    const recommended = install.locator('[data-os="vscode"]');
    await expect(recommended).toContainText("Recommended");
    await expect(install.locator("code").first()).toHaveText("code --install-extension nimblesite.osprey");
    await expect(install.locator('[data-os="unix"] code')).toHaveText("brew install nimblesite/tap/osprey");
    const windows = install.locator('[data-os="windows"]');
    await expect(windows).toBeVisible();
    await expect(windows).toContainText("Windows");
    await expect(windows.locator("code")).toContainText("scoop bucket add nimblesite https://github.com/Nimblesite/scoop-bucket");
    await expect(windows.locator("code")).toContainText("scoop install osprey");
    for (const block of await install.locator(".install-option pre").all()) {
      expect(await block.evaluate((el) => el.scrollWidth <= el.clientWidth), "an install command is cut off").toBe(true);
    }
    await page.setViewportSize({ width: 390, height: 900 });
    for (const block of await install.locator(".install-option pre").all()) {
      expect(await block.evaluate((el) => el.scrollWidth <= el.clientWidth), "an install command is cut off on a phone").toBe(true);
    }
    await page.setViewportSize({ width: 1280, height: 900 });
    await expect(install.locator('[data-os="unix"]')).toContainText("macOS and Linux");
    await expect(install.locator(".install-grid")).toHaveCount(0);
    expect((await install.boundingBox()).height).toBeLessThan(260);
    await install.getByRole("link", { name: "Getting started guide" }).click();
    await expect(page).toHaveURL(/\/docs\/installation\/$/);
  });

  test("wasm demo seeds a browser SQLite db and runs queries", async ({ page }) => {
    await page.route("https://cdn.jsdelivr.net/**", (route) => route.abort());
    for (const asset of ["/wasm/wasi-shim.mjs", "/wasm/studio.osp", "/wasm/build/studio.osp.wasm"]) {
      const res = await page.request.get(asset);
      expect(res.status(), `${asset} status`).toBe(200);
    }

    await page.goto("/wasm/");
    // The Osprey module ran, sql.js loaded, and the DB is seeded.
    await expect(page.locator("#banner.ok")).toContainText("Database ready", { timeout: 20_000 });

    // The "Write a query" tab is active by default: the query auto-ran, rendered
    // a result table, and the SQL editor overlay is highlighted.
    await expect(page.locator("#tab-query")).toHaveClass(/is-active/);
    await expect(page.locator("#sql-result table.data")).toBeVisible();
    await expect(page.locator("#sql-hl .token.keyword").first()).toBeVisible();

    // The Source tab reveals the highlighted Osprey program.
    await page.click("#tab-source");
    await expect(page.locator("#panel-source")).toBeVisible();
    await expect(page.locator("#src-code .token.keyword").first()).toBeVisible();

    // The Add-data tab: inserting a row and re-querying reflects the new data.
    await page.click("#tab-add");
    await expect(page.locator("#panel-add")).toBeVisible();
    await page.fill('#add-form input[name="product"]', "TestBrew");
    await page.click("#add-form button[type=submit]");
    await expect(page.locator("#add-status.ok")).toContainText("TestBrew");

    await page.click("#tab-query");
    await page.fill("#sql", "SELECT product FROM sales WHERE product = 'TestBrew';");
    await page.click("#run-sql");
    await expect(page.locator("#sql-result")).toContainText("TestBrew");
  });

  test("playground flavor toggle swaps the sample between .osp and .ospml", async ({ page }) => {
    await page.goto("/playground/");
    // Wait for Monaco to seed the editor with the Default-flavor sample.
    const editorValue = () =>
      page.evaluate(() => window.monaco?.editor?.getModels?.()[0]?.getValue() ?? "");
    await expect.poll(editorValue, { timeout: 20_000 }).toContain("fn install(manifest, lock)");
    // Both samples render highlighted, not as one plain colour.
    const editorColours = () => paintedColours(page.locator('.monaco-editor .view-line span[class*="mtk"]'));

    // Default flavor is active on load.
    await expect(page.locator("#flavor-osp")).toHaveClass(/active/);

    // Switching to ML swaps in the offside-rule twin (no `fn`, `handle … in`).
    await page.locator("#flavor-ospml").click();
    await expect(page.locator("#flavor-ospml")).toHaveClass(/active/);
    await expect.poll(editorValue).toContain("install (manifest, lock) =");
    expect(await editorValue()).not.toContain("fn install(manifest, lock)");

    await expect.poll(editorColours).toBeGreaterThan(1);

    // Switching back restores the Default sample.
    await page.locator("#flavor-osp").click();
    await expect.poll(editorValue).toContain("fn install(manifest, lock)");
    await expect.poll(editorColours).toBeGreaterThan(1);
  });

  test("playground compiles ML flavor: Run posts flavor=ml so .ospml reaches the ML frontend", async ({
    page,
  }) => {
    // No live compiler backend in the static E2E build, so intercept the API
    // and assert the request the playground posts. The fix: selecting ML must
    // send flavor="ml" with the offside-rule twin, so the backend writes
    // main.ospml and the ML frontend parses it (a .osp file rejects it).
    let runBody = null;
    await page.route("**/api/run", async (route) => {
      runBody = route.request().postDataJSON();
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          success: true,
          programOutput: "OSPREY FEATURE TOUR\n",
        }),
      });
    });

    await page.goto("/playground/");
    const editorValue = () =>
      page.evaluate(() => window.monaco?.editor?.getModels?.()[0]?.getValue() ?? "");
    await expect.poll(editorValue, { timeout: 20_000 }).toContain("fn install(manifest, lock)");

    // Select ML, then Run.
    await page.locator("#flavor-ospml").click();
    await expect.poll(editorValue).toContain("install (manifest, lock) =");
    await page.getByRole("button", { name: "Run", exact: true }).click();

    // The posted request selects ML and carries the offside-rule source.
    await expect.poll(() => runBody).not.toBeNull();
    expect(runBody.flavor).toBe("ml");
    expect(runBody.code).toContain("install (manifest, lock) =");
    expect(runBody.code).not.toContain("fn install(manifest, lock)");

    // Default flavor still posts flavor=default (regression guard).
    await page.locator("#flavor-osp").click();
    await expect.poll(editorValue).toContain("fn install(manifest, lock)");
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await expect.poll(() => runBody?.flavor).toBe("default");
    expect(runBody.code).toContain("fn install(manifest, lock)");
  });

  test("prose page headings join the one side panel with scroll-spy", async ({ page }) => {
    await page.goto("/spec/0001-introduction/");
    await expect(page.locator(".toc, .toc-aside nav:not(.docs-nav)")).toHaveCount(0);
    const links = page.locator(".docs-nav-desktop .toc-link");
    expect(await links.count()).toBeGreaterThan(0);

    // Clicking a TOC link jumps to that section.
    const second = links.nth(1);
    const href = await second.getAttribute("href");
    await second.click();
    await expect(page).toHaveURL(new RegExp(href.replace(/[.*+?^${}()|[\]\\]/g, "\\$&") + "$"));
    const targetVisible = await page.locator(href).isVisible();
    expect(targetVisible).toBeTruthy();

    // Scroll spy marks an active link.
    await page.evaluate(() => window.scrollBy(0, 1200));
    await page.waitForTimeout(400);
    await expect(page.locator(".docs-nav-desktop .toc-link.active")).toHaveCount(1);
  });

  test("prose headings render as plain text, not links", async ({ page }) => {
    await page.goto("/spec/0001-introduction/");
    const h2 = page.locator(".prose h2").first();
    const color = await h2.evaluate((el) => getComputedStyle(el).color);
    // on-surface (#dce1fb) — NOT the cyan link colour (#77d7f4 / #bdeeff).
    expect(color).toBe("rgb(220, 225, 251)");
  });
});

test.describe("blog search and social metadata", () => {
  const path = "/blog/2026-07-25-semver-is-all-lies-please-stop/";
  const image = "https://www.ospreylang.dev/assets/images/blog/semver-is-all-lies-please-stop.png";

  test("SemVer article exposes its editorial image and complete BlogPosting data", async ({ page }) => {
    await page.goto(path, { waitUntil: "networkidle" });
    await expect(page).toHaveTitle("SemVer Is All Lies. Please Stop");
    await expect(page.locator('meta[property="og:image"]')).toHaveAttribute("content", image);
    await expect(page.locator('meta[name="twitter:image"]')).toHaveAttribute("content", image);
    await expect(page.locator('meta[property="article:author"]')).toHaveAttribute(
      "content",
      "https://www.christianfindlay.com/"
    );
    const renderedImage = await page
      .locator(".prose-hero")
      .evaluate((element) => new URL(element.currentSrc).pathname);
    expect(renderedImage).toBe("/assets/images/blog/semver-is-all-lies-please-stop.webp");

    const data = JSON.parse(await page.locator('script[type="application/ld+json"]').textContent());
    const post = data["@graph"].find((item) => item["@type"] === "BlogPosting");
    expect(post).toMatchObject({
      headline: "SemVer Is All Lies. Please Stop",
      image: { url: image, width: 1600, height: 840 },
      mainEntityOfPage: { "@type": "WebPage", "@id": `https://www.ospreylang.dev${path}` },
      publisher: { "@id": "https://www.ospreylang.dev/#organization" },
      author: { name: "Christian Findlay", url: "https://www.christianfindlay.com/" },
    });
    expect(post.description).toContain("Osprey's package manager");
    expect(post.dateModified).toBe(post.datePublished);

    const breadcrumb = data["@graph"].find((item) => item["@type"] === "BreadcrumbList");
    expect(breadcrumb.itemListElement.map((item) => item.name)).toEqual([
      "Home",
      "Blog",
      "SemVer Is All Lies. Please Stop",
    ]);
  });
});

// Diagrams. Prose uses ```mermaid (rendered in the browser from the vendored
// runtime) and ```typediagram (rendered to inline SVG at build time) — never
// ASCII art. A silently-unrendered diagram still LOOKS like a code block, so
// these assert the SVG actually exists.
test.describe("diagrams", () => {
  test("mermaid blocks render to SVG in dark theme", async ({ page }) => {
    await page.goto("/spec/0029-packagemanagement/", { waitUntil: "networkidle" });
    const blocks = page.locator("figure.diagram pre.mermaid");
    await expect(blocks.first().locator("svg")).toBeVisible();
    // Mermaid renders the blocks one at a time, so wait until every block has its SVG.
    await expect(page.locator("pre.mermaid svg")).toHaveCount(await blocks.count());
    // Mermaid's own dark palette, not the light default on a dark page.
    const fill = await page
      .locator("pre.mermaid svg .node rect, pre.mermaid svg rect.actor")
      .first()
      .evaluate((el) => getComputedStyle(el).fill);
    const [r, g, b] = fill.match(/[\d.]+/g).map(Number);
    expect(0.299 * r + 0.587 * g + 0.114 * b, `node fill was ${fill}`).toBeLessThan(128);
  });

  // `domcontentloaded`, deliberately: the SVG is in the served HTML, so it must
  // be there before any script runs.
  test("typediagram blocks are inline SVG with no client JS", async ({ page }) => {
    await page.goto("/spec/0003-syntax/", { waitUntil: "domcontentloaded" });
    await expect(page.locator("figure.diagram-type > svg").first()).toBeVisible();
  });

  test("no diagram ships as an unrendered code block", async ({ page }) => {
    for (const path of ["/spec/0029-packagemanagement/", "/docs/web-apps/"]) {
      await page.goto(path, { waitUntil: "networkidle" });
      await expect(page.locator("pre.language-mermaid, pre.language-typediagram")).toHaveCount(0);
    }
  });
});

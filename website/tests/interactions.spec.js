// User-interaction tests with explicit assertions: navigation, the hero,
// the mobile menu drawer, and the prose Table-of-Contents.
const { test, expect } = require("@playwright/test");

test.describe("desktop interactions", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("nav links navigate to the right pages", async ({ page }) => {
    await page.goto("/");
    await page.locator(".nav-link", { hasText: "Spec" }).click();
    await expect(page).toHaveURL(/\/spec\/$/);
    await page.locator(".nav-link", { hasText: "Docs" }).click();
    await expect(page).toHaveURL(/\/docs\/$/);
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
      () => document.querySelector(".blue-box-installation").getBoundingClientRect().top
    );
    expect(nextTop).toBeGreaterThanOrEqual(winH - 5);
  });

  test("hero has a title and two working CTAs", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator(".hero-title")).toContainText("Osprey");
    await expect(page.locator(".hero-actions .btn")).toHaveCount(2);
    await page.locator(".hero-actions .btn", { hasText: "Try Osprey Online" }).click();
    await expect(page).toHaveURL(/\/playground\/$/);
  });

  test("wasm demo seeds a browser SQLite db and runs queries", async ({ page }) => {
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
    await expect.poll(editorValue, { timeout: 20_000 }).toContain("fn account()");

    // Default flavor is active on load.
    await expect(page.locator("#flavor-osp")).toHaveClass(/active/);

    // Switching to ML swaps in the offside-rule twin (no `fn`, `handle … in`).
    await page.locator("#flavor-ospml").click();
    await expect(page.locator("#flavor-ospml")).toHaveClass(/active/);
    await expect.poll(editorValue).toContain("account () =");
    expect(await editorValue()).not.toContain("fn account()");

    // Switching back restores the Default sample.
    await page.locator("#flavor-osp").click();
    await expect.poll(editorValue).toContain("fn account()");
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
          programOutput: "🦅 OSPREY FEATURE TOUR\n",
        }),
      });
    });

    await page.goto("/playground/");
    const editorValue = () =>
      page.evaluate(() => window.monaco?.editor?.getModels?.()[0]?.getValue() ?? "");
    await expect.poll(editorValue, { timeout: 20_000 }).toContain("fn account()");

    // Select ML, then Run.
    await page.locator("#flavor-ospml").click();
    await expect.poll(editorValue).toContain("account () =");
    await page.getByRole("button", { name: "Run", exact: true }).click();

    // The posted request selects ML and carries the offside-rule source.
    await expect.poll(() => runBody).not.toBeNull();
    expect(runBody.flavor).toBe("ml");
    expect(runBody.code).toContain("account () =");
    expect(runBody.code).not.toContain("fn account()");

    // Default flavor still posts flavor=default (regression guard).
    await page.locator("#flavor-osp").click();
    await expect.poll(editorValue).toContain("fn account()");
    await page.getByRole("button", { name: "Run", exact: true }).click();
    await expect.poll(() => runBody?.flavor).toBe("default");
    expect(runBody.code).toContain("fn account()");
  });

  test("real-world example code is not clipped", async ({ page }) => {
    await page.goto("/");
    const clips = await page.evaluate(() =>
      [...document.querySelectorAll(".showcase-grid .card-code pre")].map(
        (pre) => pre.scrollWidth - pre.clientWidth
      )
    );
    expect(clips.length).toBeGreaterThan(0);
    for (const c of clips) expect(c, "code block horizontal clip (px)").toBeLessThanOrEqual(2);
  });

  test("prose page shows a TOC with scroll-spy", async ({ page }) => {
    await page.goto("/spec/0001-introduction/");
    const toc = page.locator(".toc");
    await expect(toc).toBeVisible();
    const links = page.locator(".toc-link");
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
    await expect(page.locator(".toc-link.active")).toHaveCount(1);
  });

  test("prose headings render as plain text, not links", async ({ page }) => {
    await page.goto("/spec/0001-introduction/");
    const h2 = page.locator(".prose h2").first();
    const color = await h2.evaluate((el) => getComputedStyle(el).color);
    // on-surface (#dce1fb) — NOT the cyan link colour (#77d7f4 / #bdeeff).
    expect(color).toBe("rgb(220, 225, 251)");
  });
});

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
    await expect(page.locator(".nav-link", { hasText: "Docs" })).toBeVisible();
    await toggle.click();
    await expect(links).toBeHidden();
  });

  test("mobile menu link navigates", async ({ page }) => {
    await page.goto("/");
    await page.locator("#mobile-menu-toggle").click();
    await page.locator(".nav-link", { hasText: "Blog" }).click();
    await expect(page).toHaveURL(/\/blog\/$/);
  });

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
});

// Diagrams. Prose uses ```mermaid (rendered in the browser from the vendored
// runtime) and ```typediagram (rendered to inline SVG at build time) — never
// ASCII art. A silently-unrendered diagram still LOOKS like a code block, so
// these assert the SVG actually exists.
test.describe("diagrams", () => {
  test("mermaid blocks render to SVG in dark theme", async ({ page }) => {
    await page.goto("/spec/0023-languageflavors/", { waitUntil: "networkidle" });
    const blocks = page.locator("figure.diagram pre.mermaid");
    await expect(blocks.first().locator("svg")).toBeVisible();
    expect(await blocks.count()).toBe(await page.locator("pre.mermaid svg").count());
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
    for (const path of ["/spec/0023-languageflavors/", "/docs/web-apps/"]) {
      await page.goto(path, { waitUntil: "networkidle" });
      await expect(page.locator("pre.language-mermaid, pre.language-typediagram")).toHaveCount(0);
    }
  });
});

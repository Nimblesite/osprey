const { test, expect } = require("@playwright/test");
const { paintedColours, VIEWPORTS } = require("./helpers");
const { mkdir, readFile, writeFile } = require("node:fs/promises");
const { execFileSync } = require("node:child_process");
const path = require("node:path");

async function executeExample(source, file) {
  await mkdir(path.dirname(file), { recursive: true });
  await writeFile(file, source);
  return execFileSync(path.resolve("../target/release/osprey"), [file, "--run", "--quiet"], { cwd: path.dirname(file), encoding: "utf8", timeout: 20_000 });
}

async function verifyDraftExample(source, file) {
  expect(await executeExample(source, file)).toBe("Saved\n");
  expect(await readFile(path.join(path.dirname(file), "draft.txt"), "utf8")).toBe("Ship it.");
  const offline = source.replace('writeFile(path, text)', source.startsWith('# osprey:') ? 'Error(message="offline")' : 'Error { message: "offline" }')
    .replace('writeFile path text', 'Error(message = "offline")');
  const isolated = path.join(path.dirname(file), "offline", path.basename(file));
  expect(offline).not.toBe(source);
  expect(await executeExample(offline, isolated)).toBe("Retry: offline\n");
  await expect(readFile(path.join(path.dirname(isolated), "draft.txt"))).rejects.toMatchObject({ code: "ENOENT" });
  const unhandled = source.replace('disk(onSave)', 'onSave()').replace('disk onSave', 'onSave ()');
  expect(unhandled).not.toBe(source);
  expect(await exampleResult(unhandled, path.join(path.dirname(file), "unhandled", path.basename(file))))
    .toContain("unhandled effect operations at program entry: Drafts.save");
}

// stdout of a program that runs, or the compiler's diagnostic for one it rejects.
async function exampleResult(source, file) {
  try {
    return await executeExample(source, file);
  } catch (error) {
    return error.stderr.replace(`${file}: `, "");
  }
}

async function heroBounds(page) {
  return page.evaluate(() => ({ scrollY, boxes:
    [".hero", ".hero-title", ".hero-code .flight-code", ".hero-code-caption", ".hero-footnote", ".effect-story"].map(selector => {
      const { x, y, width, height } = document.querySelector(selector).getBoundingClientRect();
      return { x, y, width, height };
    }) }));
}

async function scrollSource(source) {
  await source.hover({ position: { x: 20, y: 20 } });
  await source.page().mouse.wheel(0, 200);
  await expect.poll(() => source.evaluate(el => el.scrollTop)).toBeGreaterThan(0);
}

async function openInstallation(page) {
  const action = page.locator(".hero-actions").getByRole("link", { name: "Install Osprey", exact: true });
  await action.click();
  await expect(page).toHaveURL(/\/#install$/);
  await expect(page.locator("#install .section-title")).toBeInViewport();
}

test.describe("homepage flight and source examples", () => {
  for (const viewport of VIEWPORTS) {
    test(`homepage keeps a fixed code box when switching flavors @ ${viewport.name}`, async ({ page }) => {
      await page.setViewportSize({ width: viewport.width, height: viewport.height });
      await page.emulateMedia({ reducedMotion: "reduce" });
      await page.goto("/");
      await page.evaluate(() => document.fonts.ready);
      await page.locator(".flavor-controls").scrollIntoViewIfNeeded();
      const initial = await heroBounds(page);
      for (const flavor of ["ml", "python", "default"]) {
        await page.locator(`.flavor-controls [data-flavor="${flavor}"]`).click();
        const source = page.locator(`#example-${flavor}`);
        await expect(source).toBeVisible();
        expect(await heroBounds(page)).toEqual(initial);
        expect(await source.evaluate(el => el.scrollWidth - el.clientWidth)).toBeLessThanOrEqual(2);
        if (flavor === "python") await scrollSource(source);
        expect(await heroBounds(page)).toEqual(initial);
      }
    });
  }
  test("homepage opens in Default flavor with inactive examples out of the layout", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator('.flavor-controls [aria-pressed="true"]')).toHaveText("Default");
    await expect(page.locator("#source-extension")).toHaveText(".osp");
    await expect(page.locator("#example-default")).toBeVisible();
    for (const flavor of ["ml", "python"]) {
      const source = page.locator(`#example-${flavor}`);
      await expect(source).toBeHidden();
      expect(await source.evaluate(element => element.getBoundingClientRect().height)).toBe(0);
    }
    await page.reload();
    await expect(page.locator('.flavor-controls [aria-pressed="true"]')).toHaveText("Default");
  });

  test("all three flavor examples execute with the displayed output", async ({ page }, testInfo) => {
    await page.goto("/");
    for (const [flavor, extension] of [["default", "osp"], ["ml", "ospml"], ["python", "py"]]) {
      await page.locator(`.flavor-controls button[data-flavor="${flavor}"]`).click();
      const source = page.locator(`#example-${flavor}`);
      await expect(source).toBeVisible();
      const file = testInfo.outputPath(`${flavor}/save-draft.${extension}`);
      const text = await source.textContent();
      await verifyDraftExample(text, file);
      if (flavor === "default") {
        const readme = await readFile(path.resolve("../README.md"), "utf8");
        expect(readme.match(/```osprey\n([\s\S]*?)\n```/)[1]).toBe(text);
      }
      expect(`${await page.locator(".hero .code-result samp").textContent()}\n`).toBe("Saved\n");
    }
  });

  test("the effects guide preserves runnable authored examples in both flavors", async ({ page }, testInfo) => {
    await page.goto("/docs/effects/");
    const blocks = page.locator('pre > code.language-osprey, pre > code.language-osprey-ml');
    const outputs = ["Saved\n", "Saved\n", "Retry: offline\n", "Saved: Ship it.\n", "30\n", "Submitted / Cancelled\n", "Report: Saved\n"];
    await expect(blocks).toHaveCount(outputs.length);
    for (let i = 0; i < outputs.length; i++) {
      const block = blocks.nth(i);
      const extension = (await block.getAttribute('class')).includes('osprey-ml') ? 'ospml' : 'osp';
      const output = await executeExample(await block.textContent(), testInfo.outputPath(`effects-${i}.${extension}`));
      expect(output).toBe(outputs[i]);
    }
  });

  test("the effect handlers post shows what every example prints or the error that rejects it", async ({ page }, testInfo) => {
    await page.goto("/blog/2026-10-05-effect-handlers-you-can-call/");
    const blocks = await page.locator("main pre > code").evaluateAll((codes) =>
      codes.map((code) => ({ lang: code.className, text: code.textContent })));
    const examples = blocks.flatMap((block, i) => (block.lang.includes("language-osprey") ? [{ ...block, shown: blocks[i + 1] }] : []));
    expect(examples).toHaveLength(10);
    for (const [i, { lang, text, shown }] of examples.entries()) {
      expect(shown.lang).toContain("language-text");
      const file = testInfo.outputPath(`handlers-post-${i}.${lang.includes("osprey-ml") ? "ospml" : "osp"}`);
      expect(await exampleResult(text, file)).toBe(`${shown.text.trim()}\n`);
    }
  });

  test("the match guide demonstrates binding scope in both flavors", async ({ page }, testInfo) => {
    await page.goto("/docs/keywords/match/");
    const blocks = page.locator('pre > code.language-osprey, pre > code.language-osprey-ml');
    await expect(blocks).toHaveCount(2);
    for (const [index, extension] of [[0, "osp"], [1, "ospml"]]) {
      const source = await blocks.nth(index).textContent();
      expect(await executeExample(source, testInfo.outputPath(`match.${extension}`))).toBe("2:100 / 100:100\n");
    }
  });

  test("motion starts automatically without a button and resumes when visible", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await page.goto("/");
    const video = page.locator("#flight-video");
    await expect(page.locator("#motion-toggle")).toHaveCount(0);
    await expect(video).toHaveAttribute("autoplay", "");
    await expect.poll(() => video.evaluate((el) => el.currentTime)).toBeGreaterThan(2);
    expect(await video.evaluate((el) => el.getVideoPlaybackQuality().totalVideoFrames)).toBeGreaterThan(15);
    await page.locator(".home-install").scrollIntoViewIfNeeded();
    await expect.poll(() => video.evaluate((el) => el.paused)).toBe(true);
    await page.locator(".hero-title").scrollIntoViewIfNeeded();
    await expect.poll(() => video.evaluate((el) => el.paused)).toBe(false);
  });

  test("reduced motion loads the poster without fetching the film", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    const requests = [];
    page.on("request", (request) => { if (/osprey-flight\.(webm|mp4)$/.test(request.url())) requests.push(request.url()); });
    await page.goto("/", { waitUntil: "networkidle" });
    await expect(page.locator("#motion-toggle")).toHaveCount(0);
    expect(requests).toEqual([]);
    expect(await page.locator("#flight-video").evaluate((el) => el.paused)).toBe(true);
    await expect(page.locator("#flight-video")).toHaveAttribute("poster", /osprey-flight\.png$/);
  });

  test("the page works without JavaScript", async ({ browser }) => {
    const context = await browser.newContext({ javaScriptEnabled: false });
    const page = await context.newPage();
    await page.goto("/");
    await expect(page.locator(".hero-title")).toBeVisible();
    await expect(page.locator("#example-default")).toContainText("handler Drafts");
    await expect(page.locator("#motion-toggle")).toBeHidden();
    await openInstallation(page);
    await page.locator("#install").getByRole("link", { name: "Getting started guide" }).click();
    await expect(page).toHaveURL(/\/docs\/installation\/$/);
    await context.close();
  });
});

for (const width of [1440, 390]) {
  test(`navigation separates documentation from destinations at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/docs/");
    if (width < 900) await page.locator("#mobile-menu-toggle").click();
    const nav = page.locator("#site-navigation");
    await expect(nav.locator("summary", { hasText: "Tools" })).toHaveCount(0);
    for (const [label, url] of [["Playground", "/playground/"], ["Blog", "/blog/"], ["Discord", "https://discord.gg/faH4xjd5zh"]]) {
      const link = nav.locator(":scope > li > a", { hasText: label });
      await expect(link).toBeVisible();
      await expect(link).toHaveAttribute("href", url);
    }
    await expect(nav.locator(":scope > li > a", { hasText: "Benchmarks" })).toHaveCount(0);
    await nav.locator("summary", { hasText: "Docs" }).click();
    await nav.getByRole("link", { name: "Packages", exact: true }).click();
    await expect(page).toHaveURL(/\/docs\/package-manager\/$/);
  });
}

test.describe("desktop interactions", () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test("nav links navigate to the right pages", async ({ page }) => {
    await page.goto("/");
    await expect(page.locator("#site-navigation > li")).toHaveCount(5);
    await page.locator(".nav-group > summary", { hasText: "Docs" }).click();
    await page.locator(".nav-submenu").getByRole("link", { name: "Language", exact: true }).click();
    await expect(page).toHaveURL(/\/spec\/language\/$/);
    await page.locator(".nav-group > summary", { hasText: "Docs" }).click();
    await page.locator(".nav-submenu").getByRole("link", { name: "Start here", exact: true }).click();
    await expect(page).toHaveURL(/\/docs\/$/);
  });

  test("Docs submenu supports keyboard and outside-click dismissal", async ({ page }) => {
    await page.goto("/");
    const docs = page.locator(".nav-group");
    const summary = docs.locator("summary");
    await expect(docs).toHaveCount(1);
    await summary.focus();
    await page.keyboard.press("Enter");
    await expect(docs.getByRole("link").first()).toBeVisible();
    // The open menu overlays the title's centre; click its corner, outside the menu.
    await page.locator(".hero-title").click({ position: { x: 8, y: 8 } });
    await expect(docs).not.toHaveAttribute("open", "");
    await summary.click();
    await page.keyboard.press("Escape");
    await expect(docs).not.toHaveAttribute("open", "");
    await expect(summary).toBeFocused();
  });

  test("header submenus remain usable without JavaScript", async ({ browser }) => {
    for (const width of [1440, 390]) {
      const context = await browser.newContext({ javaScriptEnabled: false, viewport: { width, height: 900 } });
      const page = await context.newPage();
      await page.goto("/");
      const nav = page.locator("#site-navigation");
      await nav.locator("summary", { hasText: "Docs" }).click();
      await nav.getByRole("link", { name: "Tutorials", exact: true }).click();
      await expect(page).toHaveURL(/\/docs\/tutorials\/$/);
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
    await expect(install.locator("code")).toContainText("brew install nimblesite/tap/osprey");
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
    await expect.poll(editorValue, { timeout: 20_000 }).toContain("fn account()");
    // Both samples render highlighted, not as one plain colour.
    const editorColours = () => paintedColours(page.locator('.monaco-editor .view-line span[class*="mtk"]'));

    // Default flavor is active on load.
    await expect(page.locator("#flavor-osp")).toHaveClass(/active/);

    // Switching to ML swaps in the offside-rule twin (no `fn`, `handle … in`).
    await page.locator("#flavor-ospml").click();
    await expect(page.locator("#flavor-ospml")).toHaveClass(/active/);
    await expect.poll(editorValue).toContain("account () =");
    expect(await editorValue()).not.toContain("fn account()");

    await expect.poll(editorColours).toBeGreaterThan(1);

    // Switching back restores the Default sample.
    await page.locator("#flavor-osp").click();
    await expect.poll(editorValue).toContain("fn account()");
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
    for (const path of ["/spec/0029-packagemanagement/", "/docs/web-apps/"]) {
      await page.goto(path, { waitUntil: "networkidle" });
      await expect(page.locator("pre.language-mermaid, pre.language-typediagram")).toHaveCount(0);
    }
  });
});

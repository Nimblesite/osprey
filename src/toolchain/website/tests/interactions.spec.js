const { test, expect } = require("@playwright/test");
const { paintedColours, VIEWPORTS, executeExample, exampleResult } = require("./helpers");
const { readFile } = require("node:fs/promises");
const { execFileSync } = require("node:child_process");
const path = require("node:path");

const hero = require("../src/_data/heroExample.json");
const productionDisk = { default: ["    handle Files { write path text => writeFile(path, text) }\n", ""], ml: ["    handle Files\n        write path text => writeFile path text\n", ""],
  python: ['with disk:\n    with terminal:\n        print(install("serde"))', 'with terminal:\n    print(install("serde"))'] };
async function verifyHeroExample(flavor, source, file) {
  expect(await executeExample(source, file)).toBe(`${hero.output}\n`);
  expect(hero.output).toContain("log: resolving serde\ninstalled\nok 1 - a full disk rolls back");
  expect(await readFile(path.join(path.dirname(file), "osprey.lock"), "utf8")).toBe("serde = 2");
  const unhandled = source.replace(...productionDisk[flavor]);
  expect(unhandled).not.toBe(source);
  expect(await exampleResult(unhandled, path.join(path.dirname(file), "unhandled", path.basename(file))))
    .toContain("unhandled effect operations at program entry: Files.write");
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
      const file = testInfo.outputPath(`${flavor}/install.${extension}`);
      const text = await source.textContent();
      await verifyHeroExample(flavor, text, file);
      await expect(page.locator("#source-extension")).toHaveText(`.${extension}`);
      if (flavor === "default") {
        const readme = await readFile(path.resolve("../../../README.md"), "utf8");
        expect(readme.match(/```osprey\n([\s\S]*?)\n```/)[1]).toBe(text);
      }
      expect(await page.locator(".hero .code-result samp").textContent()).toBe(hero.output);
    }
  });

  test("the effects guide runs its package-manager test doubles in all three flavors", async ({ page }, testInfo) => {
    await page.goto("/docs/effects/");
    const blocks = page.locator("pre > code.language-osprey, pre > code.language-python, pre > code.language-osprey-ml");
    const tap = (names) => `${names.map((name, i) => `ok ${i + 1} - ${name}\n`).join("")}1..${names.length}\n# tests=${names.length} passed=${names.length} failed=0 skipped=0\n`;
    const [pair, trio] = [tap(["a full disk rolls the manifest back", "a healthy disk writes manifest then lock"]), tap(["a healthy disk installs", "a full disk rolls the manifest back", "a read-only disk changes nothing"])];
    const outputs = [pair, null, trio, pair, "version = 3\n", "30\n", "locked osprey.toml\nwriting osprey.lock\nunlocked osprey.toml\ninstalled\nstopped: another install holds osprey.toml\n",
      { rejects: "`Versions.pick` is declared `many`, but a re-entrant continuation does not exist" }, { rejects: "handler for `Versions.pick` may resume more than once, but the handled expression requires non-replayable effect `Files.write`" }];
    await expect(blocks).toHaveCount(outputs.length);
    for (let i = 0; i < outputs.length; i++) {
      const [lang, source] = [await blocks.nth(i).getAttribute("class"), await blocks.nth(i).textContent()];
      const extension = lang.includes("osprey-ml") ? "ospml" : lang.includes("python") ? "py" : "osp";
      if (outputs[i] === null) expect(source).toBe(require("../src/_data/effectsTestExample.js").excerpt);
      else if (outputs[i].rejects) expect(await exampleResult(source, testInfo.outputPath(`effects-${i}.osp`))).toContain(outputs[i].rejects);
      else expect(await executeExample(source, testInfo.outputPath(`effects-${i}.${extension}`))).toBe(outputs[i]);
      if (i === 0) expect(await exampleResult(source.replace("memory(true)(upgrade)", "upgrade()"), testInfo.outputPath("effects-unhandled.osp"))).toContain("unhandled effect operations at program entry: Files.read, Files.write");
    }
  });

  test("the Python interop guide's shared snippet runs the same under CPython and Osprey", async ({ page }, testInfo) => {
    await page.goto("/docs/python/interop/");
    const file = testInfo.outputPath("shared.py");
    const output = await executeExample(await page.locator("pre > code.language-python").first().textContent(), file);
    expect(output).toBe("alice: 0 is none\nbob: 12 is low\ncarol: 80 is high\n");
    expect(execFileSync("python3", [file], { encoding: "utf8" })).toBe(output);
    const example = await readFile(path.resolve("../../osprey/examples/python/interop.osp"), "utf8");
    expect(`${await page.locator("pre > code.language-osprey").textContent()}\n`).toBe(example);
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
    await expect(page.locator("#example-default")).toContainText("handle Files");
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
    for (const [label, url] of [["Playground", "/playground/"], ["Blog", "/blog/"], ["GitHub", "https://github.com/Nimblesite/osprey"]]) {
      const link = nav.locator(":scope > li > a", { hasText: label });
      await expect(link).toBeVisible();
      await expect(link).toHaveAttribute("href", url);
    }
    await expect(nav.locator(":scope > li > a", { hasText: "Benchmarks" })).toHaveCount(0);
    await expect(nav.locator(":scope > li > a", { hasText: "Credits" })).toHaveCount(0);
    await expect(nav.locator("details, summary")).toHaveCount(0);
    await nav.getByRole("link", { name: "Docs", exact: true }).click();
    await expect(page).toHaveURL(/\/docs\/$/);
  });
}

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
    await expect(discord).toHaveAttribute("href", "https://discord.gg/faH4xjd5zh");
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
    await expect.poll(editorValue, { timeout: 20_000 }).toContain("fn install(manifest, lock)");
    // Both samples render highlighted, not as one plain colour.
    const editorColours = () => paintedColours(page.locator('.monaco-editor .view-line span[class*="mtk"]'));

    // Default flavor is active on load.
    await expect(page.locator("#flavor-osp")).toHaveClass(/active/);

    // Switching to ML swaps in the offside-rule twin (no `fn`, `handle … in`).
    await page.locator("#flavor-ospml").click();
    await expect(page.locator("#flavor-ospml")).toHaveClass(/active/);
    await expect.poll(editorValue).toContain("install manifest lock =");
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
    await expect.poll(editorValue).toContain("install manifest lock =");
    await page.getByRole("button", { name: "Run", exact: true }).click();

    // The posted request selects ML and carries the offside-rule source.
    await expect.poll(() => runBody).not.toBeNull();
    expect(runBody.flavor).toBe("ml");
    expect(runBody.code).toContain("install manifest lock =");
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

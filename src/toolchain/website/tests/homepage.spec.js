// The homepage: the flight and source examples in every flavor and viewport,
// what it says Osprey is for and runs on, the Osprey Native panel, and the
// effects and build panels.
const fs = require("node:fs");
const path = require("node:path");
const { readFile } = require("node:fs/promises");
const { execFileSync } = require("node:child_process");
const { test, expect } = require("@playwright/test");
const { VIEWPORTS, executeExample, exampleResult, openInstallation } = require("./helpers");

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

test("the homepage says what Osprey is for, lists every platform, and has no numbered or duplicated panels", async ({ page }) => {
  await page.goto("/");
  const hero = page.locator(".hero-description");
  for (const phrase of ["apps", "backends", "ML and AI", "mathematics", "GPU"]) await expect(hero).toContainText(phrase);
  for (const gone of ["instead of panics", "native LLVM compilation"]) await expect(hero).not.toContainText(gone);
  await expect(hero.locator('a[href="/docs/python/flavor/#local-llms-in-python-flavor"]')).toContainText("Transformers");
  await expect(page.locator(".target-links a")).toHaveText(["macOS", "Windows", "Linux", "WebAssembly", "iOS", "Android"]);
  for (const href of await page.locator(".target-links a").evaluateAll((links) => links.map((a) => a.getAttribute("href")))) {
    const [route, hash] = href.split("#");
    const response = await page.request.get(route);
    expect(response.status(), href).toBe(200);
    if (hash) expect(await response.text(), href).toContain(`id="${hash}"`);
  }
  expect(await page.locator(".home-eyebrow, .target-index").allTextContents()).not.toContainEqual(expect.stringMatching(/\d\d \//));
  await expect(page.locator(".language-section")).toHaveCount(0);
  const effects = page.locator("section#effects");
  await expect(effects.locator(".language-grid article")).toHaveCount(4);
  await expect(effects).not.toContainText("default allocator");
});

test("the homepage presents Osprey Native in one panel with the showcase film and the phone captures", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#targets, #native, #mobile-apps, .deployment-section, .mobile-capabilities")).toHaveCount(0);
  const native = page.locator("section#showcase");
  await expect(page.locator(".home > section").nth(1)).toHaveId("showcase");
  await expect(native.locator("h2")).toContainText("Osprey Native");
  for (const phrase of ["@osprey/native", "native widgets", "canvas", "Flutter", "React Native", "Compose Multiplatform"]) await expect(native).toContainText(phrase);
  for (const platform of ["macOS", "Windows", "Linux", "WebAssembly", "iOS", "Android"]) await expect(native.locator(".native-platforms")).toContainText(platform);
  const phones = native.locator(".mobile-gallery img");
  await expect(phones).toHaveCount(2);
  const video = native.locator("#showcase-video");
  const captures = native.locator(".mobile-gallery .mobile-capture");
  for (const width of [1920, 1440, 1280, 1100, 1001, 1000, 768, 390]) {
    await page.setViewportSize({ width, height: 900 });
    await video.scrollIntoViewIfNeeded();
    const v = await video.boundingBox();
    let overlapping = 0;
    for (const phone of await phones.all()) {
      await expect(phone).toBeVisible();
      const p = await phone.boundingBox();
      if (p.x < v.x + v.width && p.x + p.width > v.x && p.y < v.y + v.height && p.y + p.height > v.y) overlapping += 1;
      // Never over the play button or the control bar.
      expect(p.x > v.x + v.width / 2 + 60 || p.y > v.y + v.height, `play button clear at ${width}px`).toBe(true);
      expect(p.y + p.height < v.y + v.height - 56 || p.y > v.y + v.height || p.x > v.x + v.width, `controls clear at ${width}px`).toBe(true);
    }
    // The phones and their captions sit beside or below the film, never on top of it.
    for (const capture of await captures.all()) {
      const c = await capture.boundingBox();
      expect(c.x >= v.x + v.width || c.y >= v.y + v.height, `phone card clear of the film at ${width}px`).toBe(true);
    }
    expect(overlapping, `no phone overlaps the film at ${width}px`).toBe(0);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), `no sideways scroll at ${width}px`).toBe(true);
  }
  await native.getByRole("link", { name: "Explore Osprey Native" }).click();
  await expect(page).toHaveURL(/\/docs\/osprey-native\/$/);
  const messaging = fs.readFileSync(path.resolve(__dirname, "../../../../docs/messaging.md"), "utf8");
  expect(messaging).toContain("## Osprey Native");
  for (const rival of ["Flutter", "React Native", "Compose Multiplatform"]) expect(messaging).toContain(rival);
});

test("the homepage's effects panel is short and the build panel covers LLMs, maths and bringing Python along", async ({ page }) => {
  await page.goto("/");
  const words = (locator) => locator.evaluate((el) => el.innerText.split(/\s+/).filter(Boolean).length);
  const effects = page.locator("section#effects");
  expect(await words(effects), "effects panel words").toBeLessThan(170);
  await expect(effects.locator(".feature-mark")).toHaveCount(0);
  await expect(page.locator("#gpt2")).toHaveCount(0);
  const build = page.locator("section#build");
  await expect(build.locator("h2")).toContainText("LLMs");
  for (const [name, href] of [["LLMs", "/docs/python/flavor/#local-llms-in-python-flavor"], ["Mathematics", "/docs/proofs/"], ["Mathematics", "/docs/mathlib-port/"], ["Python", "/docs/python/interop/"]]) {
    const item = build.locator("article", { has: page.locator("h3", { hasText: name }) });
    await expect(item, name).toHaveCount(1);
    await expect(item.locator(`a[href="${href}"]`), name).toHaveCount(1);
  }
  expect(await words(build.locator(".build-items")), "build panel words").toBeLessThan(130);
});

// Safari draws no tab icon from a 1024-pixel PNG, and every browser asks for
// /favicon.ico whatever the page links: the site serves small square icons.
test("the tab and home-screen icons are small, square and served", async ({ page, request }) => {
  await page.goto("/");
  const sizes = { "/favicon.ico": [16, 32, 48] };
  const icon = await page.locator('link[rel="icon"]').getAttribute("href");
  const touch = await page.locator('link[rel="apple-touch-icon"]').getAttribute("href");
  expect(icon).toMatch(/^\/assets\/images\/favicon-32\.png\?v=[0-9a-f]{12}$/);
  expect(touch).toMatch(/^\/assets\/images\/apple-touch-icon\.png\?v=[0-9a-f]{12}$/);
  for (const [url, width] of [[icon, 32], [touch, 180]]) {
    const png = await (await request.get(url)).body();
    expect(png.readUInt32BE(16), url).toBe(width);
    expect(png.readUInt32BE(20), url).toBe(width);
  }
  const ico = await request.get("/favicon.ico");
  expect(ico.status()).toBe(200);
  const body = await ico.body();
  expect(body.readUInt16LE(2)).toBe(1);
  expect([...Array(body.readUInt16LE(4)).keys()].map((i) => body.readUInt8(6 + 16 * i))).toEqual(sizes["/favicon.ico"]);
});

// Page-health checks for every template at every breakpoint:
// no JS errors, no failed requests, no horizontal overflow, dark theme intact,
// site chrome present, the logo image resolving, and Osprey code highlighted.
const fs = require("node:fs");
const path = require("node:path");
const { test, expect } = require("@playwright/test");
const { SITE, builtPages, PAGES, VIEWPORTS, collectProblems, paintedColours } = require("./helpers");

const OSPREY_CODE = /<code([^>]*)class="language-(?:osprey(?:-ml)?|ospml|osp)"[^>]*>([\s\S]*?)<\/code>/g;
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
  const root = path.resolve("../../../docs/specs");
  const file = path.join(root, "platforms/0038-iOSTarget.md");
  const compiler = path.join(root, "compiler/iOSCodegen.md");
  const pages = new Map([[compiler, { permalink: "/spec/ioscodegen/" }]]);
  expect(resolveLink("../compiler/iOSCodegen.md#ios-host-abi", file, pages)).toBe("/spec/ioscodegen/#ios-host-abi");
  expect(resolveLink("../README.md", file, pages)).toBe("/spec/");
  expect(resolveLink("../../../src/toolchain/crates/osprey-cli/src/ios.rs", file, pages)).toBe("https://github.com/Nimblesite/osprey/blob/main/src/toolchain/crates/osprey-cli/src/ios.rs");
  expect(resolveLink("https://example.com/#host", file, pages)).toBe("https://example.com/#host");
  expect(() => resolveLink("../../../../outside", file, pages)).toThrow("Link escapes repository");
  const spec = { file, section: { slug: "platforms", title: "Platform integration" } };
  expect(() => pagesFrom([spec, spec])).toThrow("Duplicate specification URL");
  const { cleanContent } = require("../scripts/copy-spec.js");
  const prose = "Apply `f[int](value)` like [iOS](../compiler/iOSCodegen.md).\n```python\nPrice[int](...)\n```";
  expect(cleanContent(prose, file, pages)).toBe("Apply `f[int](value)` like [iOS](/spec/ioscodegen/).\n```python\nPrice[int](...)\n```");
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
  const pages = builtPages();
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
    await expect(tables).toHaveCount(4);
    await expect(page.locator(".bench-card")).toHaveCount(0);
    const relative = tables.first();
    for (const header of ["Osprey backend", "Geometric mean", "Median", "Geometric SD", "Cases"]) {
      await expect(relative.locator("thead")).toContainText(header);
    }
    await expect(relative).toContainText("Osprey (ARC)");
    await expect(relative).toContainText("Osprey (GC)");
    expect(await relative.evaluate((table) => table.clientWidth / table.closest("article").clientWidth),
      "the relative table sizes to its content").toBeLessThan(0.7);
    await expect(page.getByText("Lowest time of all languages")).toHaveCount(0);
    for (const compact of [relative]) {
      const [left, right] = await compact.evaluate((table) => {
        const box = table.getBoundingClientRect(), page = table.closest("article").getBoundingClientRect();
        return [box.left - page.left, page.right - box.right];
      });
      expect(Math.abs(left - right), "compact tables are centred").toBeLessThanOrEqual(2);
    }
    const cpu = tables.nth(1);
    expect(await cpu.evaluate((table) => table.clientWidth / table.closest("article").clientWidth)).toBeGreaterThan(0.95);
    await expect(cpu.locator("td.col-osprey .sd").first()).toHaveText(/^±\d/);
    await expect(cpu.locator("td.col-osprey").first()).toHaveAttribute("title", /mean .*median .*standard deviation .*measured \d{4}-\d{2}-\d{2}/);
    const runs = tables.nth(3);
    await expect(runs.locator("thead")).toContainText("Toolchains");
    await expect(runs.locator("tbody tr")).toHaveCount(5);
    await expect(runs).toContainText("Apple M4 Max");
    const overflow = await tables.evaluateAll((elements) => elements.map((element) => Math.max(0, element.scrollWidth - element.clientWidth)));
    expect(overflow, "every benchmark column must fit without horizontal scrolling").toEqual([0, 0, 0, 0]);
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
    await expect(links.getByRole("link", { name: "Docs", exact: true })).toBeVisible();
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
      const controls = page.locator("#site-navigation > li > a");
      // Docs, Playground, Blog, GitHub.
      await expect(controls).toHaveCount(4);
      await expectNavigationFits(controls);
      await expectNavigationFits(page.locator(".nav-cta"));
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

  test("the one side panel is a sticky menu on mobile", async ({ page }) => {
    for (const url of ["/spec/0001-introduction/", "/docs/effects/", "/blog/2026-10-05-effect-handlers-you-can-call/"]) {
      await page.goto(url);
      await expect(page.locator(".docs-nav-desktop")).toBeHidden();
      const menu = page.locator(".docs-nav-mobile");
      await page.evaluate(() => window.scrollTo(0, 2000));
      const summary = menu.locator(":scope > summary");
      await expect(summary).toBeInViewport();
      expect((await summary.boundingBox()).y).toBeLessThan(120);
      await summary.click();
      const heading = menu.locator(".toc-link").nth(1);
      await expect(heading).toBeVisible();
      const list = menu.locator(".docs-nav-mobile-list");
      expect((await list.boundingBox()).y + (await list.boundingBox()).height).toBeLessThanOrEqual(page.viewportSize().height + 1);
      const href = await heading.getAttribute("href");
      await heading.click();
      await expect(page).toHaveURL(new RegExp(`${href}$`));
      await expect(menu).not.toHaveAttribute("open", "");
      await expect(page.locator(href)).toBeInViewport();
    }
  });

  test("mobile menu supports keyboard dismissal and restores focus", async ({ page }) => {
    await page.goto("/");
    const toggle = page.locator("#mobile-menu-toggle");
    const links = page.locator(".nav-links");
    await expect(toggle).toHaveAttribute("aria-expanded", "false");
    await expect(toggle).toHaveAttribute("aria-controls", await links.getAttribute("id"));
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-expanded", "true");
    await expect(links.locator("a").first()).toBeFocused();
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

const orphanedDocs = (sources) => {
  const pages = new Map(sources.map(([url, html]) => [url, new DOMParser().parseFromString(html, "text/html")]));
  const reached = new Set(["/docs/"]);
  for (const url of reached) {
    for (const link of pages.get(url)?.querySelectorAll("a[href]") || []) {
      const target = new URL(link.getAttribute("href"), "https://docs.test" + url);
      if (target.origin === "https://docs.test" && pages.has(target.pathname)) reached.add(target.pathname);
    }
  }
  return [...pages.keys()].filter((url) => !reached.has(url)).sort();
};

test("every published docs page is reachable from documentation home", async ({ page }) => {
  const root = path.join(SITE, "docs");
  const sources = fs.readdirSync(root, { recursive: true }).filter((file) => file.endsWith(".html")).map((file) => [
    "/docs/" + file.replaceAll(path.sep, "/").replace(/index\.html$/, ""), fs.readFileSync(path.join(root, file), "utf8"),
  ]);
  expect(sources.length).toBeGreaterThan(100);
  expect(await page.evaluate(orphanedDocs, sources)).toEqual([]);
  expect(await page.evaluate(orphanedDocs, [["/docs/", ""], ["/docs/forgotten/", ""]])).toEqual(["/docs/forgotten/"]);
});

test("reactive signals is in the Language section and its ML example is highlighted", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto("/docs/reactive-signals/");
  const section = page.locator(".docs-nav-desktop .docs-nav-section", { hasText: "Language" });
  await expect(section.getByRole("link", { name: "Reactive signals", exact: true })).toHaveClass(/active/);
  const code = page.locator('pre[data-flavor="ml"] code').filter({ hasText: "signalSnapshot label" });
  await expect(code).toHaveCount(1);
  await expect(code.locator(".token.keyword").first()).toBeAttached();
  expect(await paintedColours(code.locator(".token"))).toBeGreaterThan(1);
});

for (const viewport of [{ width: 320, height: 568 }, { width: 768, height: 320 }]) {
  test(`site menu keeps every link inside ${viewport.width}x${viewport.height}`, async ({ page }) => {
    await page.setViewportSize(viewport);
    await page.goto("/docs/proofs/");
    await page.locator("#mobile-menu-toggle").click();
    await expectNavigationFits(page.locator("#site-navigation > li > a"));
    await expectNavigationFits(page.locator(".nav-cta"));
  });
}

test("Python GPU documentation preserves the current fallible tensor contract", async ({ page }) => {
  await page.goto("/docs/python/flavor/");
  const snippets = await page.locator('code.language-python').allTextContents();
  const source = snippets.join("\n");
  expect(source).toContain("-> Result[B, str]");
  expect(source).toContain("return answered(gpuMatMul(a, b, rows, inner, cols))");
  expect(source).not.toContain("or_abort");
});

test("backend guide handles product failures without the removed abort helpers", async ({ page }) => {
  await page.goto("/docs/cpu-or-gpu/");
  const code = (await page.locator('pre code').allTextContents()).join("\n");
  expect(code).toContain("Result<B, string>");
  expect(code).toContain("answered(gpuMatMul(a, b, rows, inner, cols))");
  expect(code).not.toContain("orAbort");
  await expect(page.locator("article")).toContainText("device-kernel faults");
});

test("long authored guides retain a useful image or inline video per 500 prose words", async ({ page }) => {
  const { execFileSync } = require("node:child_process");
  const markdown = require("markdown-it")();
  const repo = path.resolve(__dirname, "../../../..");
  const files = execFileSync("git", ["ls-files", "src/toolchain/website/src/docs"], { cwd: repo, encoding: "utf8" }).trim().split("\n").filter(file => file.endsWith(".md"));
  for (const file of files) {
    const source = fs.readFileSync(path.join(repo, file), "utf8");
    const prose = source.replace(/^---\n[\s\S]*?\n---\n/, "").replace(/{[%{][\s\S]*?[%}]}/g, "");
    const words = markdown.parse(prose, {}).filter(token => token.type === "inline")
      .flatMap(token => token.children || []).filter(token => ["text", "code_inline"].includes(token.type))
      .reduce((count, token) => count + (token.content.match(/\b[\w]+(?:['’-][\w]+)*\b/g) || []).length, 0);
    if (words < 500) continue;
    const route = source.match(/^permalink:\s*["']?([^\s"']+)/m)?.[1]
      || `/${file.replace(/^src\/toolchain\/website\/src\//, "").replace(/(?:index)?\.md$/, "")}${file.endsWith("index.md") ? "" : "/"}`;
    await page.goto(route);
    const article = page.locator("article");
    const images = article.locator("img:not(.video-card img)");
    const videos = article.locator("figure.video video");
    expect(await images.count() + await videos.count(), `${file}: ${words} words`).toBeGreaterThanOrEqual(Math.ceil(words / 500));
    for (const image of await images.all()) {
      await image.scrollIntoViewIfNeeded();
      expect(await image.evaluate(async el => { await el.decode(); return el.naturalWidth; }), file).toBeGreaterThan(0);
      await expect(image).toHaveAttribute("alt", /\S/);
    }
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), file).toBe(true);
  }
});

test("Python flavor guide shows valid Python that Osprey rejects, the ported packages and the local LLM work", async ({ page }, testInfo) => {
  const { execFileSync } = require("node:child_process");
  const fs = require("node:fs");
  const path = require("node:path");
  const { executeExample, exampleResult } = require("./helpers");
  await page.goto("/docs/python/flavor/");
  await expect(page.locator("h2#how-python-flavor-differs-from-python")).toHaveCount(1);
  await expect(page.locator("h2", { hasText: "leaves out" })).toHaveCount(0);
  // Valid CPython that Osprey refuses: a list with no single element type.
  const source = await page.locator("pre > code.language-python", { hasText: 'row = [42, "Ada", 3.5]' }).textContent();
  const file = testInfo.outputPath("row.py");
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, source);
  expect(execFileSync("python3", [file], { encoding: "utf8" })).toBe("Ada scored 3.5\n");
  const rejected = await exampleResult(source, testInfo.outputPath("osprey/row.py"));
  expect(rejected).toContain("3:6: type mismatch: cannot unify int with string");
  expect(rejected).toContain("3:6: type mismatch: cannot unify int with float");
  const fixed = await page.locator("pre > code.language-python", { hasText: "class Row:" }).textContent();
  expect(await executeExample(fixed, testInfo.outputPath("fixed/row.py"))).toBe("Ada scored 3.5\n");
  await expect(page.locator('img[src="/assets/images/guides/python-flavor-static-types.png"]')).toHaveCount(1);
  // Imports reach Osprey's ports of Python packages; the quoted import is real.
  const text = await page.locator(".prose").innerText();
  expect(text).not.toContain("are unavailable");
  for (const name of ["numpy", "torch", "transformers", "scipy", "sklearn", "tokenizers", "tiktoken", "safetensors", "faiss", "einops", "jinja2", "pydantic_ai", "fastapi"]) {
    expect(text, name).toContain(`@osprey/${name}`);
    expect(fs.existsSync(path.resolve(__dirname, `../../../osprey/packages/${name}/src`)), name).toBe(true);
  }
  const imports = await page.locator("pre > code.language-python", { hasText: "from numpy import Array, Errors, Host" }).textContent();
  const port = fs.readFileSync(path.resolve(__dirname, "../../../osprey/examples/python/ports/vector-search/main.py"), "utf8");
  for (const line of imports.trim().split("\n")) expect(port).toContain(line);
  // The LLM section states Qwen and the Ollama-compatible server, and keeps GPT-2.
  await expect(page.locator("h2#local-llms-in-python-flavor")).toHaveCount(1);
  for (const phrase of ["Qwen3.8-27B", "Ollama", "/api/chat", "/v1/chat/completions", "GPT-2", "plan 0049"]) expect(text).toContain(phrase);
});

test("Python interop guide's Codon comparison matches the recorded benchmark results", async ({ page }) => {
  const results = JSON.parse(fs.readFileSync(path.resolve(__dirname, "../../benchmarks/results/results.json"), "utf8"));
  const ratios = Object.values(results.cases).filter((c) => c.codon?.status === "ok" && c["osprey-arc"]?.status === "ok")
    .map((c) => c["osprey-arc"].mean / c.codon.mean);
  const geomean = Math.exp(ratios.reduce((sum, r) => sum + Math.log(r), 0) / ratios.length).toFixed(2);
  const slowest = ratios.filter((r) => r >= 5).map(Math.round).sort((a, b) => a - b);
  const slower = ratios.filter((r) => r >= 1 && r < 5);
  await page.goto("/docs/python/interop/");
  await expect(page.locator(".prose li", { hasText: "Linux and macOS today; Windows coming soon." })).toHaveCount(1);
  await expect(page.locator(".prose li", { hasText: "library.keep(obj)" })).toContainText("never cross this bridge");
  const section = page.locator(".prose p", { hasText: "Codon" });
  await expect(section).toContainText(`${geomean}×`);
  await expect(section).toContainText(`${ratios.filter((r) => r < 1).length} of ${ratios.length}`);
  const word = (n) => ["zero", "one", "two", "three", "four", "five", "six"][n] ?? `${n}`;
  await expect(section).toContainText(`${word(slower.length)} are up to ${Math.round(Math.max(...slower))}× slower`);
  if (slowest.length) await expect(section).toContainText(`${word(slowest.length)} allocation-heavy sorts are ${slowest[0]}–${slowest.at(-1)}× slower`);
  else await expect(section).not.toContainText("allocation-heavy");
  await expect(section.locator('a[href="/benchmarks/"]')).toHaveCount(1);
  await page.locator('.prose a[href="/benchmarks/"]').first().click();
  await expect(page.locator("main")).toContainText("Codon");
});

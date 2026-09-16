// Actual generated sites, loaded without a server and under a static URL prefix.
// Implements [DOC-EXPORT-HTML], [DOC-EXPORT-PAGES], [DOC-EXPORT-CSS], [DOC-LINK].
const { test: base, expect } = require("@playwright/test");
const fs = require("node:fs");
const { fileURLToPath } = require("node:url");
const { execFileSync } = require("node:child_process");
const os = require("node:os");
const path = require("node:path");
const { fixture, THEMES, CODE_SAMPLES } = require("./docs-fixture.cjs");

const test = base.extend({
  docs: [async ({}, use) => {
    const docs = fixture();
    try { await use(docs); }
    finally { fs.rmSync(docs.root, { recursive: true, force: true }); }
  }, { scope: "worker" }],
});

test("documentation guide displays executable examples in both flavors", async ({ page }) => {
  await page.goto("/docs/documentation/");
  const blocks = page.locator("main pre code.language-osprey, main pre code.language-osprey-ml");
  await expect(blocks).toHaveCount(2);
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-guide-"));
  try {
    for (const [index, extension] of ["osp", "ospml"].entries()) {
      const source = await blocks.nth(index).textContent();
      expect(source).toContain(index === 0 ? "/// Greets a reader" : "(** Greets a reader");
      const filename = path.join(root, `greetings.${extension}`);
      fs.writeFileSync(filename, source);
      const compiler = path.resolve(__dirname, "../../target/release/osprey");
      const stdout = execFileSync(compiler, ["--doctests", filename], { encoding: "utf8" });
      expect(stdout).toBe("doctests: 1 passed, 0 failed\n");
    }
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
});

for (const [theme, [background, accent]] of Object.entries(THEMES)) {
  for (const mode of ["file", "http"]) {
    test(`API docs: ${theme} theme and search over ${mode}`, async ({ page, docs }) => {
      const url = mode === "file" ? docs.file(theme, "api/a-read.html") : `${docs.prefix}/${theme}/api/a-read.html`;
      const errors = [];
      page.on("pageerror", (error) => errors.push(error.message));
      page.on("console", (message) => { if (message.type() === "error") errors.push(message.text()); });
      await page.goto(url);
      expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe(background);
      expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--accent").trim())).toBe(accent);
      await expect(page.locator("main h1")).toHaveText("A::read");
      await expect(page.locator("#tree [aria-current='page']")).toHaveCount(1);
      await expect(page.locator("main a", { hasText: "helper" })).toHaveAttribute("href", "../api/a-helper.html");
      await page.getByRole("searchbox").fill("Getting started");
      await expect(page.locator("#results a", { hasText: "Getting started" })).toBeVisible();
      await page.getByRole("searchbox").fill("no-such-page-672319");
      await expect(page.locator("#results")).toContainText("No page matches");
      await page.getByRole("searchbox").press("Escape");
      await expect(page.getByRole("searchbox")).toHaveValue("");
      expect(errors).toEqual([]);
    });
  }
}

test("API docs: authored pages, actual anchors, and inert raw HTML", async ({ page, docs }) => {
  await page.goto(docs.file("osprey", "guides/introduction.html"));
  await expect(page).toHaveTitle("Introduction — Osprey documentation");
  await page.locator("main").getByRole("link", { name: "Examples", exact: true }).click();
  await expect(page).toHaveURL(/deep\/get-started\.html#examples$/);
  await expect(page.locator("h2#examples")).toHaveText("Examples");
  await expect(page.locator("h2#examples-2")).toHaveText("Examples");
  await expect(page.locator("main table tbody td").first()).toHaveText("Guides");
  await expect(page.locator("main input[type=checkbox]")).toBeChecked();
  await expect(page.locator("main del")).toHaveText("removed");
  await expect(page.locator("main")).toContainText("<script>window.docInjected = true</script>");
  expect(await page.evaluate(() => window.docInjected)).toBeUndefined();
  await expect(page.locator("main [onclick]")).toHaveCount(0);
  const ids = await page.locator("[id]").evaluateAll((nodes) => nodes.map((node) => node.id));
  expect(new Set(ids).size, `IDs must be unique: ${ids.join(", ")}`).toBe(ids.length);
});

test("API docs: custom styles load in order and affect the browser", async ({ page, docs }) => {
  await page.goto(docs.file("custom", "index.html"));
  const styles = await page.locator("link[rel=stylesheet]").evaluateAll((links) => links.map((link) => link.getAttribute("href")));
  expect(styles).toEqual(["assets/theme.css", "assets/custom-0-brand.css", "assets/custom-1-override.css"]);
  expect(await page.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(1, 2, 3)");
  expect(await page.evaluate(() => getComputedStyle(document.documentElement).getPropertyValue("--accent").trim())).toBe("rgb(255, 0, 128)");
});

test("API docs: highlighting keeps both flavors byte exact and HTML inert", async ({ page, docs }) => {
  await page.goto(docs.file("osprey", "guides/deep/get-started.html"));
  for (const [language, source] of Object.entries(CODE_SAMPLES)) {
    const code = page.locator(`main code.language-${language}`);
    expect(await code.textContent()).toBe(source);
    expect(await code.locator(".token.string").count()).toBe(1);
    expect(await code.locator("img, script, b").count()).toBe(0);
  }
});

test("API docs: mobile navigation opens by keyboard and search stays usable", async ({ page, docs }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto(docs.file("midnight", "api/a-read.html"));
  await expect(page.locator("#menu")).not.toHaveAttribute("open", "");
  await expect(page.locator("#tree")).toBeHidden();
  expect((await page.locator("main").boundingBox()).y).toBeLessThan(260);
  expect(await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth)).toBeLessThanOrEqual(1);
  await page.keyboard.press("Tab");
  await expect(page.getByRole("link", { name: "Skip to content" })).toBeFocused();
  await page.locator("#menu > summary").focus();
  await page.keyboard.press("Enter");
  await expect(page.locator("#tree")).toBeVisible();
  await page.getByRole("searchbox").fill("A::helper");
  await page.locator("#results").getByRole("link", { name: "A::helper", exact: true }).click();
  await expect(page.locator("main h1")).toHaveText("A::helper");
});

for (const theme of Object.keys(THEMES)) {
  test(`API docs: ${theme} layout stays readable at desktop and phone widths`, async ({ page, docs }) => {
    for (const width of [1440, 390, 320]) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(docs.file(theme, "index.html"));
      await expect(page.locator(".hero h1")).toHaveText("Osprey documentation");
      await expect(page.locator(".catalog .page-card").first()).toBeVisible();
      await expect(page.locator(".catalog .page-card").first()).toContainText("A");
      await expect(page.locator(".catalog .page-card").first()).toContainText("2 public members");
      await expect(page.locator(".catalog .page-card").nth(1)).toContainText("1 public member");
      await expect(page.locator(".stats")).toContainText("2 modules");
      const layout = await page.evaluate(() => ({
        overflow: document.documentElement.scrollWidth - innerWidth,
        fontSize: parseFloat(getComputedStyle(document.querySelector('.hero-description')).fontSize),
        cardWidth: document.querySelector('.page-card').getBoundingClientRect().width,
        top: document.querySelector('main').getBoundingClientRect().top,
      }));
      expect(layout.overflow).toBeLessThanOrEqual(1);
      expect(layout.fontSize).toBeGreaterThanOrEqual(15);
      expect(layout.cardWidth).toBeGreaterThan(200);
      expect(layout.top).toBeLessThan(260);
      await page.goto(docs.file(theme, "api/a-read.html"));
      await expect(page.locator("#tree [aria-current='page']")).toHaveText("A::read");
      expect(await page.locator(".nav-group[open]").count()).toBe(1);
      expect(await page.evaluate(() => document.documentElement.scrollWidth - innerWidth)).toBeLessThanOrEqual(1);
      if (width === 1440) await expect(page.locator("#tree [aria-current='page']")).toBeVisible();
      else await expect(page.locator("#tree")).toBeHidden();
    }
  });
}

test("API docs: templates differ in typography and geometry, beyond their palettes", async ({ page, docs }) => {
  await page.setViewportSize({ width: 1440, height: 1000 });
  const identities = [];
  for (const theme of Object.keys(THEMES)) {
    await page.goto(docs.file(theme, "index.html"));
    identities.push(await page.evaluate(() => {
      const heading = getComputedStyle(document.querySelector('.hero h1'));
      const card = getComputedStyle(document.querySelector('.page-card'));
      return [heading.fontFamily, heading.fontSize, card.borderRadius, card.display].join('|');
    }));
  }
  expect(new Set(identities).size).toBe(3);
});

test("API docs: outline links reach real headings and slash focuses search", async ({ page, docs }) => {
  await page.goto(docs.file("osprey", "guides/deep/get-started.html"));
  await expect(page.locator(".toc")).toBeVisible();
  const headings = await page.locator(".article h2[id], .article h3[id]").count();
  await expect(page.locator(".toc a")).toHaveCount(headings);
  await page.locator(".toc a").first().click();
  await expect(page).toHaveURL(/#examples$/);
  await page.keyboard.press("/");
  await expect(page.getByRole("searchbox")).toBeFocused();
  await page.getByRole("searchbox").fill("A::helper");
  await expect(page.locator("#results a").first()).toHaveText("A::helper");
});

test("API docs: without JavaScript every navigation group stays reachable", async ({ browser, docs }) => {
  const context = await browser.newContext({ javaScriptEnabled: false });
  try {
    const page = await context.newPage();
    await page.goto(docs.file("osprey", "api/a-read.html"));
    await expect(page.locator("#tree")).toBeVisible();
    expect(await page.locator(".nav-group[open]").count()).toBe(await page.locator(".nav-group").count());
    await expect(page.locator("#tree [aria-current='page']")).toBeVisible();
    await expect(page.locator("main h1")).toHaveText("A::read");
  } finally { await context.close(); }
});

test("API docs: every generated internal link and local asset exists", async ({ page, docs }) => {
  const files = fs.readdirSync(`${docs.root}/osprey`, { recursive: true }).filter((file) => file.endsWith(".html"));
  expect(files.length).toBeGreaterThan(130);
  expect(files).not.toContain("api/a-privatedetail.html");
  const missing = [];
  for (const file of files) {
    await page.goto(docs.file("osprey", file));
    const urls = await page.locator("a[href],link[href],script[src]").evaluateAll((nodes) => nodes.map((node) => node.href || node.src));
    for (const url of urls.filter((url) => url.startsWith("file:"))) {
      if (!fs.existsSync(fileURLToPath(new URL(url)))) missing.push(`${file} -> ${url}`);
    }
  }
  expect(missing).toEqual([]);
});

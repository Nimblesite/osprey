import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { startServer } from "../scripts/serve.mjs";

const require = createRequire(import.meta.url);
const { test, expect } = require("../../website/node_modules/@playwright/test");

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const repoRoot = path.resolve(root, "../../..");

let serverHandle;

test.beforeAll(async () => {
  serverHandle = await startServer(0);
});

test.afterAll(async () => {
  if (serverHandle?.server) {
    await new Promise((resolve) => serverHandle.server.close(resolve));
  }
});

test("Osprey Native website renders cleanly with flagship media, logo/favicon, and passes interactive journeys", async ({ page, request }) => {
  const errors = [];
  page.on("pageerror", (err) => errors.push(err.message));
  page.on("response", (res) => {
    if (res.status() >= 400) errors.push(`HTTP ${res.status()} for ${res.url()}`);
  });

  await page.goto(serverHandle.url, { waitUntil: "networkidle" });
  expect(await page.evaluate(() => window.scrollY), "page opens at top of hero").toBe(0);
  const stageBox = await page.locator(".device-stage").boundingBox();
  expect(stageBox.y + stageBox.height, "entire hero device stage fits above the 900px fold").toBeLessThanOrEqual(900);

  // 1. Metadata, landmarks, headings, logo, favicon, and content-hashed assets
  await expect(page.locator('meta[name="color-scheme"]')).toHaveAttribute("content", "light dark");
  await expect(page.locator("h1")).toHaveCount(1);
  await expect(page.locator("header, nav[aria-label='Primary'], main, footer")).toHaveCount(4);

  const faviconHref = await page.locator('link[rel="icon"]').getAttribute("href");
  expect(faviconHref).toMatch(/^\/assets\/brand\/favicon\.svg\?v=[0-9a-f]{12}$/);
  const favRes = await request.get(`${serverHandle.url}${faviconHref}`);
  expect(favRes.status()).toBe(200);

  const cssHrefs = await page.locator('link[rel="stylesheet"]').evaluateAll((els) => els.map((e) => e.getAttribute("href")));
  expect(cssHrefs.length).toBe(7);
  for (const href of cssHrefs) {
    expect(href).toMatch(/^\/css\/[a-z]+\.css\?v=[0-9a-f]{12}$/);
  }
  const navLabels = await page.locator('nav[aria-label="Primary"] a').allTextContents();
  expect(navLabels).toEqual(["Features", "Showcase", "Compare", "Blog", "Docs", "GitHub"]);
  const jsSrc = await page.locator("script[src]").getAttribute("src");
  expect(jsSrc).toMatch(/^\/js\/site\.js\?v=[0-9a-f]{12}$/);

  // 2. Flagship screenshots load cleanly; no toy Tally/Flock screenshots on homepage
  const allImgSrcs = await page.locator("img").evaluateAll((els) => els.map((e) => e.getAttribute("src") || ""));
  expect(allImgSrcs.some((s) => /tally|flock/i.test(s))).toBe(false);

  const visibleImgs = page.locator("img:visible");
  const visibleCount = await visibleImgs.count();
  expect(visibleCount).toBeGreaterThanOrEqual(8);
  for (let i = 0; i < visibleCount; i++) {
    const img = visibleImgs.nth(i);
    await img.scrollIntoViewIfNeeded();
    const naturalWidth = await img.evaluate((el) =>
      el.complete && el.naturalWidth > 0
        ? el.naturalWidth
        : new Promise((resolve) => {
            el.addEventListener("load", () => resolve(el.naturalWidth), { once: true });
            el.addEventListener("error", () => resolve(0), { once: true });
          })
    );
    expect(naturalWidth, `visible image ${i} loaded`).toBeGreaterThan(0);
  }

  // 3. Playable videos have posters and English WebVTT caption tracks
  const videos = page.locator("video");
  await expect(videos).toHaveCount(3);
  for (let i = 0; i < 3; i++) {
    const poster = await videos.nth(i).getAttribute("poster");
    expect(poster).toMatch(/^\/assets\/videos\/.+\.jpg$/);
    const posterRes = await request.get(`${serverHandle.url}${poster}`);
    expect(posterRes.status()).toBe(200);
    await expect(videos.nth(i).locator('track[kind="captions"]')).toHaveCount(1);
  }

  // 4. Dual-engine rendering mode switcher (Native Platform Controls <-> Custom Canvas) & no GPUI/FerroUI jargon on front page
  await expect(page.locator("main")).not.toContainText("GPUI");
  await expect(page.locator("main")).not.toContainText("FerroUI");
  const modeNative = page.getByRole("tab", { name: "Native Platform Controls" });
  const modeCanvas = page.getByRole("tab", { name: "Custom Canvas" });
  await expect(modeNative).toHaveAttribute("aria-selected", "true");
  await modeCanvas.click();
  await expect(modeCanvas).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "Custom Canvas" })).toContainText("Sherbet Signal");
  await modeNative.click();

  // 5. Syntax flavor tabs: fixed box height across all tabs + syntax highlighting + keyboard arrows
  const codeWin = page.locator("#flavors .code-window");
  const initialBox = await codeWin.boundingBox();
  await expect(codeWin.locator(".tok-kw").first()).toBeVisible();

  const tabDefault = page.getByRole("tab", { name: "Default (.osp)" });
  const tabMl = page.getByRole("tab", { name: "ML (.ospml)" });
  const tabPython = page.getByRole("tab", { name: "Python" });
  const tabTs = page.getByRole("tab", { name: "TypeScript" });
  const tabRoadmap = page.getByRole("tab", { name: "Dart & Kotlin" });

  await expect(tabDefault).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "Default (.osp)" })).toContainText("Signal<Accounts>.read()");

  await tabDefault.focus();
  await page.keyboard.press("ArrowRight");
  await expect(tabMl).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "ML (.ospml)" })).toContainText("Keyed::region");
  expect((await codeWin.boundingBox()).height).toBeCloseTo(initialBox.height, 0);

  await tabPython.click();
  await expect(tabPython).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "Python" })).toContainText("# osprey: flavor=python");
  expect((await codeWin.boundingBox()).height).toBeCloseTo(initialBox.height, 0);

  await tabTs.click();
  await expect(tabTs).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "TypeScript" })).toContainText("// osprey: flavor=typescript");
  expect((await codeWin.boundingBox()).height).toBeCloseTo(initialBox.height, 0);

  await page.keyboard.press("End");
  await expect(tabRoadmap).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "Dart & Kotlin" })).toContainText("Dart Flavor");
  await expect(page.getByRole("tabpanel", { name: "Dart & Kotlin" })).toContainText("Kotlin Flavor");
  expect((await codeWin.boundingBox()).height).toBeCloseTo(initialBox.height, 0);

  // 6. Features nav link points to real #features section & Accessibility band renders @osprey/uitest snippet
  await expect(page.locator('nav[aria-label="Primary"] a', { hasText: "Features" })).toHaveAttribute("href", "/#features");
  await expect(page.locator("#features h2")).toContainText("Everything you need");
  await expect(page.locator("#accessibility")).toContainText('tapAt ("Volume", 0.5)');

  // 7. Theme Toggle switches cleanly between Light and Dark surfaces
  const themeBtn = page.locator("#theme-toggle");
  await themeBtn.click(); // Light
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  const lightBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);

  await themeBtn.click(); // Dark
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await expect(page.locator("body")).not.toHaveCSS("background-color", lightBg);

  await themeBtn.click(); // Auto
  await expect(page.locator("html")).not.toHaveAttribute("data-theme");

  // 8. Comparison table column order (Osprey Native, Flutter, React Native, CMP) & zero overflow
  const colHeaders = await page.locator(".compare-table thead th").allTextContents();
  expect(colHeaders).toEqual(["Capability", "Osprey Native", "Flutter", "React Native", "Compose Multiplatform (CMP)"]);

  for (const width of [1440, 375]) {
    await page.setViewportSize({ width, height: 900 });
    const fits = await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth);
    expect(fits, `no horizontal overflow at ${width}px`).toBe(true);
  }

  const theadDisplay = await page.locator(".compare-table thead").evaluate((el) => getComputedStyle(el).display);
  expect(theadDisplay).toBe("none");

  // 9. Internal /docs/ section (Get Started + Docs nav link stay on-site)
  await page.setViewportSize({ width: 1440, height: 900 });
  await expect(page.getByRole("link", { name: "Get Started" })).toHaveAttribute("href", "/docs/");
  await page.locator('nav[aria-label="Primary"] a[href="/docs/"]').click();
  await expect(page).toHaveURL(/\/docs\/$/);
  await expect(page.locator('nav[aria-label="Primary"] a[aria-current="page"]')).toHaveText("Docs");
  await expect(page.locator("article h1")).toHaveText("Osprey Native Documentation");
  await page.locator('.docs-nav-list a[href="/docs/bridge/"]').click();
  await expect(page).toHaveURL(/\/docs\/bridge\/$/);
  await expect(page.locator("article h1")).toHaveText("The Shared-Memory Bridge");

  // 10. Blog grid on homepage, dedicated /blog/ index, and blog article page
  await page.goto(serverHandle.url, { waitUntil: "networkidle" });
  await expect(page.locator("#blog .blog-grid .blog-card")).toHaveCount(3);

  await page.locator('nav[aria-label="Primary"] a[href="/blog/"]').click();
  await expect(page).toHaveURL(/\/blog\/$/);
  await expect(page.locator('nav[aria-label="Primary"] a[aria-current="page"]')).toHaveText("Blog");
  await expect(page.locator(".blog-grid .blog-card")).toHaveCount(3);

  await page.locator(".blog-card-title a").first().click();
  await expect(page.locator("article h1")).toHaveCount(1);
  await expect(page.locator(".prose-article-body")).toContainText("Osprey Native");
  const postImgWidth = await page.locator(".post-hero-figure img").evaluate((el) => el.naturalWidth);
  expect(postImgWidth).toBeGreaterThan(0);

  expect(errors).toEqual([]);
});

test("Messaging docs live together in docs/messaging/ and follow messaging guardrails", async () => {
  const ospreyDoc = fs.readFileSync(path.join(repoRoot, "docs/messaging/osprey.md"), "utf8");
  const nativeDoc = fs.readFileSync(path.join(repoRoot, "docs/messaging/osprey-native.md"), "utf8");

  expect(fs.existsSync(path.join(repoRoot, "docs/messaging.md"))).toBe(false);
  expect(ospreyDoc).toContain("## Osprey Native");

  for (const phrase of [
    "FerroUI",
    "SwiftUI",
    "WinUI",
    "Signal<T>",
    "Dart flavor",
    "Kotlin flavor",
    "@osprey/uitest",
    "Accessibilitymaxxing",
    "Compose Multiplatform",
    "Flutter",
    "React Native",
    "actively evolving",
  ]) {
    expect(nativeDoc).toContain(phrase);
  }
});

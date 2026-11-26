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

  // 1. Metadata, landmarks, headings, logo, favicon, and content-hashed assets
  await expect(page.locator('meta[name="color-scheme"]')).toHaveAttribute("content", "light dark");
  await expect(page.locator("h1")).toHaveCount(1);
  await expect(page.locator("header, nav[aria-label='Primary'], main, footer")).toHaveCount(4);

  const faviconHref = await page.locator('link[rel="icon"]').getAttribute("href");
  expect(faviconHref).toMatch(/^\/assets\/brand\/favicon\.svg\?v=[0-9a-f]{12}$/);
  const favRes = await request.get(`${serverHandle.url}${faviconHref}`);
  expect(favRes.status()).toBe(200);

  const cssHrefs = await page.locator('link[rel="stylesheet"]').evaluateAll((els) => els.map((e) => e.getAttribute("href")));
  expect(cssHrefs.length).toBe(5);
  for (const href of cssHrefs) {
    expect(href).toMatch(/^\/css\/[a-z]+\.css\?v=[0-9a-f]{12}$/);
  }
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
    const naturalWidth = await img.evaluate((el) => el.naturalWidth);
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

  // 4. Dual-engine rendering mode switcher (Native Platform Controls <-> FerroUI GPU Canvas)
  const modeNative = page.getByRole("tab", { name: "Native Platform Controls" });
  const modeCanvas = page.getByRole("tab", { name: "FerroUI / GPUI GPU Canvas" });
  await expect(modeNative).toHaveAttribute("aria-selected", "true");
  await modeCanvas.click();
  await expect(modeCanvas).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "FerroUI / GPUI GPU Canvas" })).toContainText("Sherbet Signal");
  await modeNative.click();

  // 5. Syntax flavor tabs: click + keyboard arrow navigation + hidden="until-found"
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

  await tabPython.click();
  await expect(tabPython).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "Python" })).toContainText("# osprey: flavor=python");

  await tabTs.click();
  await expect(tabTs).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "TypeScript" })).toContainText("// osprey: flavor=typescript");

  await page.keyboard.press("End");
  await expect(tabRoadmap).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tabpanel", { name: "Dart & Kotlin" })).toContainText("Dart Flavor");
  await expect(page.getByRole("tabpanel", { name: "Dart & Kotlin" })).toContainText("Kotlin Flavor");

  // 6. Interactive Accessibility Tree & @osprey/uitest Explorer
  const controls = [
    ["Volume", "Slidable", 'tapAt ("Volume", 0.5)'],
    ["Position", "Slidable", 'expect (letters "00:50", true)'],
    ["Receiver: Living Room TV", "Popup", 'tap "Receiver: Living Room TV"'],
    ["Open file…", "Pressable", 'picked "/music/Sunroom.mp3"'],
    ["Play", "Pressable", 'tap "Play"'],
  ];

  for (const [label, role, expectedSnippet] of controls) {
    const btn = page.locator("#a11y-tree-list button", { hasText: label });
    await btn.click();
    await expect(btn).toHaveAttribute("aria-pressed", "true");
    await expect(page.locator("#a11y-inspector-title")).toHaveText(`Inspected Control: ${label}`);
    await expect(page.locator("#a11y-role")).toHaveText(role);
    await expect(page.locator("#a11y-test-code")).toContainText(expectedSnippet);
    await expect(page.locator("#a11y-live-region")).toContainText(label);
  }

  // 7. Theme Toggle switches cleanly between Light and Dark surfaces
  const themeBtn = page.locator("#theme-toggle");
  await themeBtn.click(); // Light
  await expect(page.locator("html")).toHaveAttribute("data-theme", "light");
  const lightBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);

  await themeBtn.click(); // Dark
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  const darkBg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(lightBg).not.toBe(darkBg);

  await themeBtn.click(); // Auto
  await expect(page.locator("html")).not.toHaveAttribute("data-theme");

  // 8. Comparison table & zero horizontal overflow at 1440px and 375px
  const compareRegion = page.getByRole("region", { name: /Architectural comparison/i });
  for (const framework of ["Osprey Native", "Compose Multiplatform (CMP)", "Flutter", "React Native"]) {
    await expect(compareRegion).toContainText(framework);
  }

  for (const width of [1440, 375]) {
    await page.setViewportSize({ width, height: 900 });
    const fits = await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth);
    expect(fits, `no horizontal overflow at ${width}px`).toBe(true);
  }

  const theadDisplay = await page.locator(".compare-table thead").evaluate((el) => getComputedStyle(el).display);
  expect(theadDisplay).toBe("none");

  // 9. Blog grid on homepage, dedicated /blog/ index, and blog article page
  await page.setViewportSize({ width: 1440, height: 900 });
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

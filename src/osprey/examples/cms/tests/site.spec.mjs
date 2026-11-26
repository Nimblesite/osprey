// The sample site the developer hands over [CMS-E2E] [CMS-SITE]: Eleventy builds it from the same
// Markdown and media the editor changes, and a reader sees a navigable site: a nav that marks the
// page, a hero, the latest post, a journal grid of every post newest first, articles and an about page.
import { expect, test } from "@playwright/test";
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { ELEVENTY_SITE as SITE, buildSite, out, serveSite } from "./fixture.mjs";

const BUILT = path.join(out, "site-build");
const PORT = 4396;
const BASE = `http://127.0.0.1:${PORT}`;

/** Every post as its front matter says, newest first. */
function posts() {
  const folder = path.join(SITE, "content/blog");
  return readdirSync(folder).filter((name) => name.endsWith(".md")).map((name) => {
    const front = readFileSync(path.join(folder, name), "utf8").split("---")[1];
    const field = (key) => front.match(new RegExp(`^${key}: (.*)$`, "m"))[1];
    return { url: `/blog/${name.slice(0, -3)}/`, title: field("title"), date: field("date"), summary: field("summary") };
  }).sort((a, b) => b.date.localeCompare(a.date));
}

let server;
test.beforeAll(async () => {
  buildSite(SITE, BUILT);
  server = await serveSite(BUILT, PORT);
});
test.afterAll(() => server?.kill());

async function nav(page, url, current) {
  await page.goto(BASE + url);
  const links = page.getByRole("navigation", { name: "Main" }).getByRole("link");
  await expect(links).toHaveText(["Northwind Studio", "Home", "Journal", "About"]);
  await expect(page.locator('[aria-current="page"]')).toHaveText(current);
}

test("a reader browses the site: nav, hero, latest post, journal grid, articles, about [CMS-SITE]", async ({ page }) => {
  const all = posts();
  expect(all.length).toBeGreaterThanOrEqual(4);
  await nav(page, "/", "Home");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("We design calm software for busy people.");
  await expect(page.locator(".featured h2")).toHaveText(all[0].title);
  await expect(page.locator(".grid .card h3")).toHaveText(all.slice(1, 7).map((post) => post.title));
  for (const image of await page.locator("img").all()) expect(await image.evaluate((element) => element.complete && element.naturalWidth > 0)).toBe(true);

  await page.getByRole("link", { name: "Read the journal" }).click();
  await expect(page).toHaveURL(`${BASE}/blog/`);
  await nav(page, "/blog/", "Journal");
  await expect(page.locator(".grid .card h3")).toHaveText(all.map((post) => post.title));
  await expect(page.locator(".grid .card .summary")).toHaveText(all.map((post) => post.summary));
  const columns = await page.locator(".grid").evaluate((grid) => getComputedStyle(grid).gridTemplateColumns.split(" ").length);
  expect(columns).toBe(3);

  for (const post of all) {
    await page.goto(`${BASE}/blog/`);
    await page.locator(".card", { hasText: post.title }).click();
    await expect(page).toHaveURL(BASE + post.url);
    await expect(page.locator('[aria-current="page"]')).toHaveText("Journal");
    await expect(page.getByRole("heading", { level: 1 })).toHaveText(post.title);
    await expect(page.locator(".post-head .meta")).toContainText("min read");
    await expect(page.getByRole("heading", { level: 1 })).toHaveCount(1);
    await page.getByRole("link", { name: "← All posts" }).click();
    await expect(page).toHaveURL(`${BASE}/blog/`);
  }

  await nav(page, "/about/", "About");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("About the studio");
  await expect(page).toHaveTitle("About · Northwind Studio");
});

test("the site reads on a phone and in the dark [CMS-SITE]", async ({ browser }) => {
  const phone = await browser.newPage({ viewport: { width: 390, height: 844 }, colorScheme: "dark" });
  await phone.goto(`${BASE}/blog/`);
  expect(await phone.evaluate(() => document.documentElement.scrollWidth)).toBeLessThanOrEqual(390);
  expect(await phone.locator(".grid").evaluate((grid) => getComputedStyle(grid).gridTemplateColumns.split(" ").length)).toBe(1);
  expect(await phone.evaluate(() => getComputedStyle(document.body).backgroundColor)).toBe("rgb(11, 13, 18)");
  await expect(phone.getByRole("navigation", { name: "Main" }).getByRole("link", { name: "About" })).toBeVisible();
  await phone.close();
});

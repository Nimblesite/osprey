// The credits page (/credits/): it must build, be linked from the main
// navigation and footer, list every license-audit entry with its authors,
// carry only well-formed upstream links, and never expose an email address.
const fs = require("node:fs");
const path = require("node:path");
const { test, expect } = require("@playwright/test");

const SITE = path.resolve(__dirname, "../_site");
const AUDIT = JSON.parse(
  fs.readFileSync(path.resolve(__dirname, "../../../osprey/packages/license-audit.json"), "utf8")
);
const PACKAGE_IDS = Object.keys(AUDIT.packages);
const EMAIL = /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/;

test("the credits page builds and the navigation links to it", async ({ page }) => {
  const html = fs.readFileSync(path.join(SITE, "credits", "index.html"), "utf8");
  expect(html).toContain("Announcements and credits");

  await page.goto("/credits/");
  await expect(page.locator("h1")).toHaveText("Announcements and credits");
  await expect(page.locator('header .nav-link[href="/credits/"]')).toBeVisible();
  await expect(page.locator('footer a[href="/credits/"]')).toBeVisible();
});

test("every license-audit entry appears with its authors", async ({ page }) => {
  await page.goto("/credits/");
  for (const id of PACKAGE_IDS) {
    const row = page.locator(`tr[data-package="${id}"]`);
    await expect(row, `audit entry "${id}" must appear`).toHaveCount(1);
    // Authors are the second cell (Project, Authors, Licence, Osprey package, Relationship).
    const authors = row.locator("td").nth(1);
    await expect(authors, `authors for "${id}"`).not.toHaveText(/^\s*$/);
  }
});

test("every link on the credits page is well formed", async ({ page }) => {
  await page.goto("/credits/");
  const hrefs = await page
    .locator("main a[href]")
    .evaluateAll((links) => links.map((link) => link.getAttribute("href")));
  expect(hrefs.length).toBeGreaterThan(0);
  for (const href of hrefs) {
    let url;
    expect(() => {
      url = new URL(href, "http://localhost");
    }, `malformed href: ${href}`).not.toThrow();
    expect(["http:", "https:"], `href is not a well-formed http(s) link: ${href}`).toContain(
      url.protocol
    );
  }
});

test("no email address appears on the credits page", async ({ page }) => {
  await page.goto("/credits/");
  const text = await page.locator("body").textContent();
  expect(text).not.toMatch(EMAIL);
});

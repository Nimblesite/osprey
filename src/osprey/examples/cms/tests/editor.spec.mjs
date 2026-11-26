// The Git CMS editor in a real browser [CMS-E2E] [CMS-EDITOR]: one editor's session against the
// Osprey Native editor the CMS serves. It browses the tree, selects text, reads syntax-coloured
// code, restyles and edits blocks, saves (a commit only), publishes (a push) and checks the raw
// Markdown, asserting the page, the layout and the site's git history at every step.
import { expect, test } from "@playwright/test";
import { readFileSync } from "node:fs";
import path from "node:path";
import { BROWSER_SITE, SITE, git, head, published } from "./fixture.mjs";

const HOME = "content/index.md";
const POST = "content/blog/first.md";
/** The files the tree may show: under a root, none hidden. [CMS-PATH-SAFE] [CMS-API-TREE] */
const LISTED = Object.keys(SITE).filter((file) => /^(content|media)\//.test(file) && !file.split("/").some((part) => part.startsWith("."))).sort();

const row = (page, key) => page.locator(`.osp-row[data-row-key="${key}"]`);
const disk = (file) => readFileSync(path.join(BROWSER_SITE, file), "utf8");
const brand = (token) => `var(--osp-brand-${token})`;

/** Every coloured run of the code block, as [text, colour]. */
async function runs(page) {
  return page.locator(".osp-frame-source [style*=color]").evaluateAll((spans) => spans.map((span) => [span.textContent, span.style.color]));
}

async function treeAndChrome(page) {
  await expect(row(page, HOME)).toBeVisible();
  expect(await page.locator(".osp-row[data-row-key*='/']").evaluateAll((rows) => rows.map((item) => item.dataset.rowKey))).toEqual(LISTED);
  expect(await page.locator(".osp-row-group").allTextContents()).toEqual(["content", "media"]);
  await expect(row(page, POST)).toHaveText("blog/first.md");
  const sidebar = await page.locator(".osp-frame-sidebar").boundingBox();
  expect(sidebar.height).toBe(900);
  const item = await row(page, POST).boundingBox();
  const label = await row(page, POST).locator(".osp-row-cell").boundingBox();
  expect(label.x - item.x).toBeLessThan(40);
  await expect(page.getByText("Search files", { exact: true }).filter({ visible: true })).toHaveCount(0);
  await expect(page.getByPlaceholder("Search files")).toHaveCount(1);
  const pill = await page.locator(".osp-frame-status").boundingBox();
  expect(pill.height).toBeLessThan(34);
  await expect(page.getByRole("button", { name: /Publish/ })).toBeDisabled();
}

async function search(page) {
  await page.getByPlaceholder("Search files").fill("blog");
  await expect(page.locator(".osp-row[data-row-key*='/']")).toHaveCount(1);
  await page.getByPlaceholder("Search files").fill("");
  await expect(page.locator(".osp-row[data-row-key*='/']")).toHaveCount(LISTED.length);
}

async function highlighted(page) {
  await row(page, POST).click();
  await expect(page.getByRole("textbox", { name: "h1" })).toHaveValue("First post");
  await expect(page.locator(".osp-frame-code")).toBeVisible();
  const coloured = await runs(page);
  expect(coloured.map(([text]) => text).join("")).toBe('fn main() = print("hi")');
  expect(coloured).toContainEqual(["fn", brand("synKeyword")]);
  expect(coloured).toContainEqual(['"hi"', brand("synString")]);
  expect(coloured.filter(([, colour]) => colour === brand("synKeyword")).map(([text]) => text)).toEqual(["fn"]);
  await expect(page.locator(".osp-frame-code")).toContainText("OSP");
  await expect(page.getByText("```")).toHaveCount(0);
  await expect(page.getByText("`code`")).toHaveCount(0);
}

async function codeEditing(page) {
  await page.locator(".osp-frame-code").click();
  const code = page.getByRole("textbox", { name: "code" });
  await expect(code).toHaveValue('```osp\nfn main() = print("hi")\n```');
  expect(await code.evaluate((element) => getComputedStyle(element).fontFamily)).toMatch(/mono/i);
  await expect(page.getByRole("textbox", { name: "h1" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "First post" })).toBeVisible();
}

/** Drag to select backwards and double-click a word: the same editor keeps the selection throughout. */
async function selection(page) {
  await row(page, HOME).click();
  const editor = page.getByRole("textbox", { name: "h1" });
  await expect(editor).toHaveValue("Welcome 👋");
  await editor.evaluate((element) => { window.held = element; });
  const box = await editor.boundingBox();
  await page.mouse.click(box.x + box.width - 4, box.y + 20);
  await page.mouse.down();
  for (let step = 1; step <= 6; step += 1) {
    await page.mouse.move(box.x + box.width - 4 - step * 60, box.y + 20);
    await page.waitForTimeout(60);
    expect(await page.evaluate(() => document.activeElement === window.held)).toBe(true);
  }
  await page.mouse.move(box.x + 2, box.y + 20);
  await page.mouse.up();
  await page.waitForTimeout(400);
  expect(await page.evaluate(() => [window.held.isConnected, window.held.selectionStart, window.held.selectionEnd > 0])).toEqual([true, 0, true]);
  await editor.dblclick({ position: { x: 30, y: 20 } });
  await page.waitForTimeout(400);
  expect(await page.evaluate(() => window.held.value.slice(window.held.selectionStart, window.held.selectionEnd))).toBe("Welcome");
}

/** Typing never flashes the toolbar disabled, and the edit leaves the page unsaved, never published. */
async function typing(page) {
  const editor = page.getByRole("textbox", { name: "h1" });
  await editor.click();
  await page.keyboard.press("End");
  await page.keyboard.type(" hom");
  const save = page.getByRole("button", { name: "Save" });
  await expect(save).toBeEnabled();
  await page.keyboard.type("e");
  const opacity = await save.evaluate(async (element) => {
    const seen = [];
    for (let sample = 0; sample < 6; sample += 1) {
      seen.push(Number(getComputedStyle(element).opacity));
      await new Promise((resolve) => setTimeout(resolve, 25));
    }
    return seen;
  });
  expect(opacity).toEqual([1, 1, 1, 1, 1, 1]);
  await expect(page.locator(".osp-frame-status")).toHaveText(/Unsaved/);
  await expect(editor).toHaveValue("Welcome 👋 home");
}

async function restyled(page) {
  await page.getByRole("button", { name: "H2" }).click();
  await expect(page.getByRole("textbox", { name: "h2" })).toHaveValue("Welcome 👋 home");
  await page.getByRole("button", { name: "H1" }).click();
  await expect(page.getByRole("textbox", { name: "h1" })).toHaveValue("Welcome 👋 home");
}

async function saveAndPublish(page) {
  const live = published(BROWSER_SITE);
  await page.getByRole("button", { name: "Save" }).click();
  await expect(page.locator(".osp-frame-status")).toHaveText(/Saved/);
  expect(disk(HOME)).toContain("# Welcome 👋 home\n");
  expect(git(BROWSER_SITE, "log", "-1", "--format=%s")).toBe(`Edit ${HOME}`);
  expect(published(BROWSER_SITE)).toBe(live);
  const publish = page.getByRole("button", { name: "Publish 1" });
  await expect(publish).toBeEnabled();
  await publish.click();
  await expect(page.getByText("Published", { exact: true })).toBeVisible();
  expect(published(BROWSER_SITE)).toBe(head(BROWSER_SITE));
  await expect(page.getByRole("button", { name: /Publish/ })).toBeDisabled();
}

async function rawMarkdown(page) {
  await page.getByRole("button", { name: "Markdown" }).click();
  const source = page.getByRole("textbox", { name: "Markdown" });
  await expect(source).toHaveValue(disk(HOME));
  await expect(page.locator(".osp-frame-code")).toHaveCount(0);
  await page.getByRole("button", { name: "Formatted" }).click();
  await expect(page.getByRole("textbox", { name: "Markdown" })).toHaveCount(0);
}

test("an editor's session: browse, select, read highlighted code, edit, save, publish [CMS-E2E]", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  await page.goto("/");
  for (const step of [treeAndChrome, search, highlighted, codeEditing, selection, typing, restyled, saveAndPublish, rawMarkdown]) {
    await test.step(step.name, () => step(page));
  }
  expect(errors).toEqual([]);
});

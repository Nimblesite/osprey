// The Git CMS editor in a real browser [CMS-E2E] [CMS-EDITOR]: one editor's session against the
// Osprey Native editor the CMS serves. It browses the tree, selects text, reads syntax-coloured
// code, restyles and edits blocks, saves (a commit only), publishes (a push) and checks the raw
// Markdown, asserting the page, the layout and the site's git history at every step.
import { expect, test } from "@playwright/test";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { BROWSER_SITE, SITE, git, head, published } from "./fixture.mjs";

const HOME = "content/index.md";
const POST = "content/blog/first.md";
/** The files the tree may show: under a root, none hidden. [CMS-PATH-SAFE] [CMS-API-TREE] */
const LISTED = Object.keys(SITE).filter((file) => /^(content|media)\//.test(file) && !file.split("/").some((part) => part.startsWith("."))).sort();

const row = (page, key) => page.locator(`.osp-row[data-row-key="${key}"]`);
const disk = (file) => readFileSync(path.join(BROWSER_SITE, file), "utf8");
const brand = (token) => `var(--osp-brand-${token})`;
/** The status pill whose words are exactly `words` (case-sensitive, so "Saved" never matches "Unsaved"). */
const status = (page, words) => page.locator(".osp-frame-status").filter({ hasText: new RegExp(`(^|[^A-Za-z])${words}$`) });
/** Whether the ribbon lights the tool named `label`. */
const lit = (page, label) => page.getByRole("button", { name: label, exact: true }).evaluate((button) => button.closest(".osp-framed").getAttribute("style").includes("accentSoft"));

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
  expect(await page.getByPlaceholder("Search files").evaluate((input) => getComputedStyle(input, "::placeholder").color)).not.toBe("rgb(117, 117, 117)");
  const live = status(page, "Live");
  await expect(live).toBeVisible();
  expect((await live.boundingBox()).height).toBeLessThan(34);
  await expect(status(page, "Saved")).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Publish/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Delete" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "New page" })).toHaveCount(2);
  await row(page, HOME).click();
  const save = page.getByRole("button", { name: "Save" });
  await expect(save).toBeDisabled();
  expect(await save.evaluate((button) => button.closest(".osp-framed").getAttribute("style"))).toContain("--osp-brand-sunken");
  await page.getByRole("button", { name: "Markdown" }).click();
  await page.getByRole("button", { name: "Formatted" }).click();
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
  expect(await lit(page, "H1")).toBe(true);
  expect(await lit(page, "Body")).toBe(false);
  await expect(page.getByRole("button", { name: "Delete", exact: true })).toHaveCount(1);
  await expect(page.getByText("›", { exact: true })).toHaveCount(2);
  const heading = await page.getByRole("textbox", { name: "h1" }).boundingBox();
  expect(heading.height).toBeLessThan(90);
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
  await expect(page.locator(".osp-row-cell").getByText("First post", { exact: true })).toHaveCSS("color", /./);
  await expect(page.getByText("[First post](/blog/first/)")).toHaveCount(0);
  await expect(page.getByText("“curly quotes”, café crème and 🚀", { exact: false })).toBeVisible();
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
  await expect(status(page, "Unsaved changes")).toBeVisible();
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
  await expect(status(page, "Saved")).toBeVisible();
  expect(disk(HOME)).toContain("# Welcome 👋 home\n");
  expect(git(BROWSER_SITE, "log", "-1", "--format=%s")).toBe(`Edit ${HOME}`);
  expect(published(BROWSER_SITE)).toBe(live);
  const publish = page.getByRole("button", { name: "Publish 1" });
  await expect(publish).toBeEnabled();
  await publish.click();
  await expect(page.getByText("Published", { exact: true })).toBeVisible();
  expect(published(BROWSER_SITE)).toBe(head(BROWSER_SITE));
  await expect(page.getByRole("button", { name: /Publish/ })).toHaveCount(0);
  await expect(status(page, "Live")).toBeVisible();
}

/** The open file goes with one tap on its Delete button: one commit, gone from disk and from the tree. */
async function deleted(page, file) {
  const before = head(BROWSER_SITE);
  await expect(page.getByRole("button", { name: "Delete", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Delete file" })).toBeVisible();
  await page.getByRole("button", { name: "Delete file" }).click();
  await expect(row(page, file)).toHaveCount(0);
  expect(git(BROWSER_SITE, "log", "-1", "--format=%s")).toBe(`Delete ${file}`);
  expect(git(BROWSER_SITE, "rev-list", "--count", `${before}..HEAD`)).toBe("1");
  expect(existsSync(path.join(BROWSER_SITE, file))).toBe(false);
  await expect(page.getByRole("button", { name: /Delete/ })).toHaveCount(0);
  await expect(page.getByText("Pick a page to edit")).toBeVisible();
}

/** A picture bigger than one request: the browser's chooser, several parts, one commit, a preview. [CMS-API-UPLOAD] */
async function upload(page) {
  const bytes = Buffer.alloc(2_000_000 + 17, 0);
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]).copy(bytes);
  for (let at = 8; at < bytes.length; at += 1) bytes[at] = (at * 31 + 7) % 256;
  const before = head(BROWSER_SITE);
  const chooser = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: /Upload/ }).click();
  await (await chooser).setFiles({ name: "Team Photo.png", mimeType: "image/png", buffer: bytes });
  await expect(row(page, "media/team-photo.png")).toBeVisible({ timeout: 30_000 });
  expect(git(BROWSER_SITE, "log", "-1", "--format=%s")).toBe("Upload media/team-photo.png");
  expect(git(BROWSER_SITE, "rev-list", "--count", `${before}..HEAD`)).toBe("1");
  expect(readFileSync(path.join(BROWSER_SITE, "media/team-photo.png")).equals(bytes)).toBe(true);
  await expect(page.locator("img[src^='data:image/png;base64,']")).toBeVisible();
  await expect(page.getByRole("button", { name: "Publish 1" })).toBeEnabled();
  const uploaded = head(BROWSER_SITE);
  const cancelled = page.waitForEvent("filechooser");
  await page.getByRole("button", { name: /Upload/ }).click();
  await (await cancelled).setFiles([]);
  await page.waitForTimeout(500);
  expect(head(BROWSER_SITE)).toBe(uploaded);
  await expect(page.getByRole("button", { name: "Publish 1" })).toBeEnabled();
  await deleted(page, "media/team-photo.png");
}

async function rawMarkdown(page) {
  await page.getByRole("button", { name: "Markdown" }).click();
  const source = page.getByRole("textbox", { name: "Markdown" });
  await expect(source).toHaveValue(disk(HOME));
  expect((await source.boundingBox()).width).toBeGreaterThan(450);
  expect((await source.boundingBox()).height).toBeGreaterThan(500);
  await expect(page.locator(".osp-frame-code")).toHaveCount(0);
  await page.getByRole("button", { name: "Formatted" }).click();
  await expect(page.getByRole("textbox", { name: "Markdown" })).toHaveCount(0);
}

test("an editor's session: browse, select, read highlighted code, edit, save, publish [CMS-E2E]", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => message.type() === "error" && errors.push(message.text()));
  await page.goto("/");
  for (const step of [treeAndChrome, search, highlighted, codeEditing, selection, typing, restyled, saveAndPublish, rawMarkdown, upload]) {
    await test.step(step.name, () => step(page));
  }
  expect(errors).toEqual([]);
});

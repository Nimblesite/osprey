// An author's journey from the editor to the live site [CMS-E2E] [CMS-EDITOR] [CMS-PUBLISH] [CMS-SITE]:
// the CMS runs over a git copy of the real Eleventy sample site. The author creates a post (and is
// refused bad paths), writes it in both modes with every ribbon tool, is guarded from losing unsaved
// words, reads the history, publishes, and then reads the post on the site Eleventy builds from the
// published remote; finally they delete it, behind a confirmation, and the rebuilt site drops it.
import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { AUTHOR_PORT, AUTHOR_SITE, buildSite, git, head, out, published, row, selectWord, serveSite, siteFiles, ELEVENTY_SITE, status } from "./fixture.mjs";

test.use({ baseURL: `http://127.0.0.1:${AUTHOR_PORT}` });

const POST = "content/blog/field-notes.md";
const SITE_PORT = 4399;
const LIVE = `http://127.0.0.1:${SITE_PORT}`;
const CLONE = path.join(out, "site-author-live");
const BUILT = path.join(out, "site-author-built");
const disk = (file) => readFileSync(path.join(AUTHOR_SITE, file), "utf8");
const subject = () => git(AUTHOR_SITE, "log", "-1", "--format=%s");
const button = (page, name) => page.getByRole("button", { name, exact: true });
const FRONT = "---\ntitle: Field Notes\ndate: 2026-12-01\ntopic: Travel\nsummary: What a week on the coast taught us about planning.\ncover: /media/cover-tide.svg\n---\n";

/** Every page and medium of the real site, and none of its layouts, data, styles or config. [CMS-PATH-SAFE] */
async function realTree(page) {
  const listed = Object.keys(siteFiles(ELEVENTY_SITE)).filter((file) => /^(content|media)\//.test(file)).sort();
  await expect(row(page, "content/index.md")).toBeVisible();
  expect(await page.locator(".osp-row[data-row-key*='/']").evaluateAll((rows) => rows.map((item) => item.dataset.rowKey))).toEqual(listed);
  for (const hidden of ["_includes/base.njk", "_data/site.json", "assets/site.css", "eleventy.config.mjs"]) await expect(row(page, hidden)).toHaveCount(0);
  await expect(status(page, "Live")).toBeVisible();
}

async function draftPath(page, value) {
  await page.getByRole("button", { name: "New page" }).first().click();
  const field = page.getByRole("textbox", { name: /Path/ });
  await expect(field).toHaveValue("content/new-page.md");
  await field.fill(value);
  await button(page, "Create page").click();
}

/** A new page: cancelling leaves nothing, an existing page or a non-Markdown path is refused, a fresh path is created. */
async function creating(page) {
  const before = head(AUTHOR_SITE);
  await page.getByRole("button", { name: "New page" }).first().click();
  await expect(page.getByText("Where should it live?")).toBeVisible();
  await button(page, "Cancel").click();
  await expect(page.getByText("Where should it live?")).toHaveCount(0);
  await draftPath(page, "content/blog/first.md");
  await expect(page.getByText("That page already exists", { exact: true })).toBeVisible();
  expect(disk("content/blog/first.md")).toBe(readFileSync(path.join(ELEVENTY_SITE, "content/blog/first.md"), "utf8"));
  await button(page, "Dismiss").click();
  await page.getByRole("textbox", { name: /Path/ }).fill("content/blog/field-notes.txt");
  await button(page, "Create page").click();
  await expect(page.getByText("Pages are Markdown", { exact: true })).toBeVisible();
  await button(page, "Dismiss").click();
  expect(head(AUTHOR_SITE)).toBe(before);
  await page.getByRole("textbox", { name: /Path/ }).fill(POST);
  await button(page, "Create page").click();
  await expect(row(page, POST)).toBeVisible();
  await expect(status(page, "Saved")).toBeVisible();
  expect(subject()).toBe(`Edit ${POST}`);
  expect(disk(POST)).toBe("---\ntitle: Field Notes\n---\n\n# Field Notes\n\nStart writing here.\n");
  await expect(button(page, "Publish 1")).toBeVisible();
}

/** Front matter is written in Markdown; the body in Formatted, one paragraph per Enter. */
async function writing(page) {
  await button(page, "Markdown").click();
  const source = page.getByRole("textbox", { name: "Markdown" });
  await source.fill(`${FRONT}\n# Field Notes\n\nStart writing here.\n`);
  await expect(status(page, "Unsaved changes")).toBeVisible();
  await button(page, "Formatted").click();
  await page.locator(".osp-row-cell").getByText("Start writing here.", { exact: true }).click();
  const paragraph = page.getByRole("textbox", { name: "paragraph" });
  await expect(paragraph).toBeFocused();
  await paragraph.evaluate((field) => field.select());
  await page.keyboard.type("Notes from the coast");
  for (const words of ["Tide tables matter", "Pack light, then lighter"]) {
    await page.keyboard.press("Enter");
    await page.keyboard.type(words);
  }
  await expect(paragraph).toHaveValue("Pack light, then lighter");
  for (const words of ["Notes from the coast", "Tide tables matter"]) await expect(page.locator(".osp-row-cell").getByText(words, { exact: true })).toHaveCount(1);
}

async function block(page, words) {
  await page.locator(".osp-row-cell").getByText(words).first().click();
}

/** Each `[word, tool, value?, nth?]`: select the word in `field`, press the tool, and expect the field's value when one is given. */
async function styled(page, field, steps) {
  for (const [word, tool, value, nth] of steps) {
    await selectWord(field, word, nth);
    await button(page, tool).click();
    if (value) await expect(field).toHaveValue(value);
  }
}

/** Every ribbon tool in Formatted mode: styles toggle, emphasis wraps and unwraps, inserts land at the caret. */
async function ribbonFormatted(page) {
  await block(page, "Notes from the coast");
  const paragraph = page.getByRole("textbox", { name: "paragraph" });
  await styled(page, paragraph, [["coast", "B", "Notes from the **coast**"], ["coast", "B", "Notes from the coast"], ["coast", "B"],
    ["Notes", "I", "*Notes* from the **coast**"], ["coast", "I", "*Notes* from the ***coast***"], ["coast", "I", "*Notes* from the **coast**"]]);
  // Code wraps a word in backticks and, pressed again on the same word, takes them off.
  await styled(page, paragraph, [["from", "Code", "*Notes* `from` the **coast**"], ["from", "Code", "*Notes* from the **coast**"]]);
  await block(page, "Tide tables matter");
  await button(page, "• List").click();
  await expect(page.getByRole("textbox", { name: "bullets" })).toHaveValue("Tide tables matter");
  await block(page, "Pack light");
  await button(page, "❝ Quote").click();
  const quote = page.getByRole("textbox", { name: "quote" });
  await expect(quote).toHaveValue("Pack light, then lighter");
  await quote.click();
  await page.keyboard.press("End");
  await page.keyboard.type(" ");
  await button(page, "Link").click();
  await expect(quote).toHaveValue("Pack light, then lighter [link text](https://)");
  await block(page, "Field Notes");
  const heading = page.getByRole("textbox", { name: "h1" });
  await expect(heading).toHaveValue("Field Notes");
  for (const [tool, label] of [["H2", "h2"], ["H2", "paragraph"], ["H3", "h3"], ["H1", "h1"]]) {
    await button(page, tool).click();
    await expect(page.getByRole("textbox", { name: label })).toHaveValue("Field Notes");
  }
}

/** The same tools on raw Markdown: line styles replace and toggle, emphasis unwraps a selection with its markers. */
async function ribbonMarkdown(page) {
  await button(page, "Markdown").click();
  const source = page.getByRole("textbox", { name: "Markdown" });
  await selectWord(source, "Field Notes", 1);
  for (const [tool, line] of [["H2", "\n## Field Notes\n"], ["H2", "\nField Notes\n"], ["H1", "\n# Field Notes\n"]]) {
    await button(page, tool).click();
    await expect(source).toHaveValue(new RegExp(line.replace(/[#]/g, "\\#")));
  }
  await expect(source).not.toHaveValue(/# #|## #/);
  // List, quote and heading tools take a line's own prefix off and put it back, never doubling one.
  await styled(page, source, [["Tide tables matter", "• List", /\nTide tables matter\n/], ["Tide tables matter", "• List", /\n- Tide tables matter\n/],
    ["Pack light", "❝ Quote", /\nPack light, then lighter \[link text\]\(https:\/\/\)\n/], ["Pack light", "❝ Quote", /\n> Pack light, then lighter \[link text\]\(https:\/\/\)\n/],
    ["Field Notes", "H3", /\n### Field Notes\n/, 1], ["Field Notes", "H1", /\n# Field Notes\n/, 1]]);
  await expect(source).toHaveValue(`${FRONT}\n# Field Notes\n\n*Notes* from the **coast**\n\n- Tide tables matter\n\n> Pack light, then lighter [link text](https://)\n`);
  await selectWord(source, "https://");
  await page.keyboard.type("https://example.com/tides");
  await selectWord(source, "link text");
  await page.keyboard.type("tide tables");
  await styled(page, source, [["**coast**", "B", /the coast\n/], ["coast", "B", /the \*\*coast\*\*\n/, 1],
    ["Travel", "Image", /!\[Describe the image\]\(\/media\/\)/]]);
  // Italic wraps a word of the front matter and, pressed again on it, unwraps it.
  await styled(page, source, [["planning", "I", /\nsummary: What a week on the coast taught us about \*planning\*\.\n/],
    ["planning", "I", /\nsummary: What a week on the coast taught us about planning\.\n/]]);
  await selectWord(source, "![Describe the image](/media/)");
  await page.keyboard.press("Backspace");
  await expect(source).toHaveValue(/topic: \n/);
  await selectWord(source, "topic: ");
  await source.evaluate((field) => field.setSelectionRange(field.selectionEnd, field.selectionEnd));
  await page.keyboard.type("Travel");
  await button(page, "Save").click();
  await expect(status(page, "Saved")).toBeVisible();
  expect(disk(POST)).toBe(`${FRONT}\n# Field Notes\n\n*Notes* from the **coast**\n\n- Tide tables matter\n\n> Pack light, then lighter [tide tables](https://example.com/tides)\n`);
  await button(page, "Formatted").click();
}

/** Unsaved words survive a click elsewhere until the author chooses to drop them. */
async function guarded(page) {
  await block(page, "Tide tables matter");
  const list = page.getByRole("textbox", { name: "bullets" });
  await list.evaluate((field) => field.setSelectionRange(field.value.length, field.value.length));
  await page.keyboard.type(" (and the moon)");
  await expect(status(page, "Unsaved changes")).toBeVisible();
  await row(page, "content/about.markdown").click();
  await expect(page.getByText("Unsaved changes", { exact: true }).last()).toBeVisible();
  await button(page, "Keep editing").click();
  await expect(page.getByRole("textbox", { name: "bullets" })).toHaveValue("Tide tables matter (and the moon)");
  await row(page, "content/about.markdown").click();
  await button(page, "Discard changes").click();
  await expect(page.getByText("About", { exact: false }).first()).toBeVisible();
  await expect(status(page, "Saved")).toBeVisible();
  await row(page, POST).click();
  await expect(page.locator(".osp-row-cell").getByText("Tide tables matter", { exact: true })).toHaveCount(1);
  expect(disk(POST)).not.toContain("moon");
}

/** The history drawer lists the commits newest first. */
async function history(page) {
  await page.getByRole("button", { name: "History" }).click();
  const expected = git(AUTHOR_SITE, "log", "-3", "--format=%s").split("\n");
  expect(expected).toEqual([`Edit ${POST}`, `Edit ${POST}`, "Init site"]);
  const drawer = page.locator(".osp-frame-drawer");
  for (const words of expected) await expect(drawer.getByText(words, { exact: true }).first()).toBeVisible();
  await expect(drawer.getByText(head(AUTHOR_SITE).slice(0, 7), { exact: true })).toBeVisible();
  await drawer.getByRole("button", { name: "Close" }).click();
  await expect(drawer).toHaveCount(0);
}

/** Publish pushes both saves; the site Eleventy builds from the remote carries the post. */
async function publishing(page) {
  await button(page, "Publish 2").click();
  await expect(page.getByText("Published", { exact: true })).toBeVisible();
  await expect(status(page, "Live")).toBeVisible();
  expect(published(AUTHOR_SITE)).toBe(head(AUTHOR_SITE));
}

/** Clone what is published, build it with Eleventy and serve it, as the site's pipeline would. */
async function liveSite() {
  rmSync(CLONE, { recursive: true, force: true });
  execFileSync("git", ["clone", "-q", `${AUTHOR_SITE}.git`, CLONE]);
  buildSite(CLONE, BUILT);
  return serveSite(BUILT, SITE_PORT);
}

async function readsLive(page) {
  const server = await liveSite();
  try {
    await page.goto(`${LIVE}/`);
    const featured = page.getByRole("region", { name: "Latest" });
    await expect(featured.getByRole("heading", { name: "Field Notes" })).toBeVisible();
    await expect(featured.getByText("What a week on the coast taught us about planning.")).toBeVisible();
    await page.goto(`${LIVE}/blog/`);
    await expect(page.locator(".card h3").first()).toHaveText("Field Notes");
    await expect(page.locator(".card h3")).toHaveCount(5);
    await page.locator(".card-link").first().click();
    await expect(page).toHaveURL(`${LIVE}/blog/field-notes/`);
    const prose = page.locator(".prose");
    await expect(page.getByRole("heading", { level: 1 }).first()).toHaveText("Field Notes");
    await expect(prose.locator("em")).toHaveText("Notes");
    await expect(prose.locator("strong")).toHaveText("coast");
    await expect(prose.locator("ul > li")).toHaveText("Tide tables matter");
    await expect(prose.locator("blockquote")).toContainText("Pack light, then lighter");
    await expect(prose.locator("blockquote a")).toHaveAttribute("href", "https://example.com/tides");
    await expect(page.locator(".post-cover")).toHaveAttribute("src", "/media/cover-tide.svg");
    await expect(page.locator(".post-head")).toContainText("1 December 2026");
    await expect(page.locator(".post-head")).toContainText("Travel");
  } finally {
    server.kill();
  }
}

/** A commit made outside the editor shows after Refresh, and counts towards Publish. */
async function refreshed(page) {
  const file = "content/blog/from-the-terminal.md";
  await expect(row(page, "content/index.md")).toBeVisible();
  await expect(status(page, "Live")).toBeVisible();
  writeFileSync(path.join(AUTHOR_SITE, file), "---\ntitle: From the terminal\ndate: 2026-08-01\nsummary: Committed by hand.\n---\n\nHello.\n");
  git(AUTHOR_SITE, "add", file);
  git(AUTHOR_SITE, "commit", "-q", "-m", `Add ${file}`);
  await expect(row(page, file)).toHaveCount(0);
  await page.getByRole("button", { name: "Refresh" }).click();
  await expect(row(page, file)).toBeVisible();
  await expect(button(page, "Publish 1")).toBeVisible();
}

/** Delete asks first; cancelling keeps the post, confirming commits its removal, publishing takes it off the site. */
async function deleting(page) {
  await row(page, POST).click();
  const before = head(AUTHOR_SITE);
  await button(page, "Delete").click();
  await expect(page.getByText("Delete field-notes.md?")).toBeVisible();
  await button(page, "Cancel").click();
  await expect(page.getByText("Delete field-notes.md?")).toHaveCount(0);
  expect(head(AUTHOR_SITE)).toBe(before);
  expect(existsSync(path.join(AUTHOR_SITE, POST))).toBe(true);
  await button(page, "Delete").click();
  await button(page, "Yes, delete").click();
  await expect(row(page, POST)).toHaveCount(0);
  expect(subject()).toBe(`Delete ${POST}`);
  expect(existsSync(path.join(AUTHOR_SITE, POST))).toBe(false);
  await expect(page.getByText("Pick a page to edit")).toBeVisible();
  await button(page, "Publish 2").click();
  await expect(status(page, "Live")).toBeVisible();
  const server = await liveSite();
  try {
    const gone = await fetch(`${LIVE}/blog/field-notes/`);
    expect(gone.status).toBe(404);
    await page.goto(`${LIVE}/blog/`);
    await expect(page.locator(".card h3")).toHaveText(["Type on screens", "Shipping small, every day", "Colour systems that survive dark mode", "First post", "From the terminal"]);
  } finally {
    server.kill();
  }
}

test("an author writes a post, publishes it, reads it on the built site, then deletes it [CMS-E2E]", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => message.type() === "error" && !message.text().includes("404") && errors.push(message.text()));
  await page.goto("/");
  for (const step of [realTree, creating, writing, ribbonFormatted, ribbonMarkdown, guarded, history, publishing]) {
    await test.step(step.name, () => step(page));
  }
  await test.step("readsLive", () => readsLive(page));
  await page.goto("/");
  for (const step of [refreshed, deleting]) await test.step(step.name, () => step(page));
  expect(errors).toEqual([]);
});

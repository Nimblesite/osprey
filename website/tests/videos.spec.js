// Video tutorials follow Google's video indexing guidance
// (https://developers.google.com/search/docs/appearance/video): the watch page
// embeds the video in a <video> element, carries VideoObject markup with a Clip
// per key moment, uses one thumbnail URL everywhere, is listed in the video
// sitemap, and honours the ?t= deep links its key moments point at.
const fs = require("node:fs");
const path = require("node:path");
const { test, expect } = require("@playwright/test");

const SITE = path.resolve(__dirname, "../_site");
const ORIGIN = "https://www.ospreylang.dev";
const WATCH = "/docs/tutorials/swift-interop/";
/** Every watch page, each with its own video. */
const WATCH_PAGES = [WATCH, "/docs/tutorials/profiler/", "/docs/packages-in-vscode/"];
const read = (file) => fs.readFileSync(path.join(SITE, file), "utf8");
const jsonLd = (html) =>
  [...html.matchAll(/<script type="application\/ld\+json">([\s\S]*?)<\/script>/g)].map(([, json]) => JSON.parse(json));
const meta = (html, key) => html.match(new RegExp(`<meta (?:property|name)="${key}" content="([^"]*)">`))?.[1];

test("the native chapter track loads its WebVTT file", async () => {
  const { videoPlayer, cues } = await import("../lib/videos.mjs");
  const player = videoPlayer("swift-interop");
  const source = player.match(/<track kind="chapters"[^>]*src="([^"]+)"/)[1];
  expect(source).toBe("/assets/videos/osprey-swift-interop.chapters.vtt");
  expect(cues(source).length).toBeGreaterThan(3);
});

for (const watch of WATCH_PAGES) test(`${watch} describes its video with VideoObject and key moments`, () => {
  const html = read(`${watch.slice(1)}index.html`);
  const video = jsonLd(html).find((data) => data["@type"] === "VideoObject");
  expect(video, "VideoObject").toBeTruthy();
  for (const key of ["name", "description", "thumbnailUrl", "uploadDate", "duration", "contentUrl"]) expect(video[key], key).toBeTruthy();
  expect(video.uploadDate).toMatch(/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d[+-]\d\d:\d\d$/);
  expect(video.duration).toMatch(/^PT(\d+H)?(\d+M)?(\d+S)?$/);
  expect(video.name).toBe(html.match(/<title>(.*?)<\/title>/)[1]);
  expect(fs.existsSync(path.join(SITE, new URL(video.contentUrl).pathname))).toBe(true);

  const starts = video.hasPart.map((clip) => clip.startOffset);
  expect(video.hasPart.length).toBeGreaterThan(3);
  expect(new Set(starts).size, "no two clips share a start").toBe(starts.length);
  expect(starts).toEqual([...starts].sort((a, b) => a - b));
  for (const clip of video.hasPart) {
    expect(clip.name).toBeTruthy();
    expect(clip.endOffset).toBeGreaterThan(clip.startOffset);
    expect(clip.url).toBe(`${ORIGIN}${watch}?t=${clip.startOffset}`);
  }
});

test("one thumbnail URL serves the poster, Open Graph, the markup and the sitemap", () => {
  const html = read("docs/tutorials/swift-interop/index.html");
  const video = jsonLd(html).find((data) => data["@type"] === "VideoObject");
  const poster = html.match(/<video [^>]*poster="([^"]+)"/)[1];
  const thumbnail = `${ORIGIN}${poster}`;
  expect(video.thumbnailUrl).toEqual([thumbnail]);
  expect(meta(html, "og:image")).toBe(thumbnail);
  expect(meta(html, "twitter:image")).toBe(thumbnail);
  expect(meta(html, "og:video")).toBe(video.contentUrl);
  expect(meta(html, "robots")).toContain("max-video-preview:-1");
  expect(fs.existsSync(path.join(SITE, poster))).toBe(true);

  const sitemap = read("video-sitemap.xml");
  expect(sitemap).toContain(`<loc>${ORIGIN}${WATCH}</loc>`);
  expect(sitemap).toContain(`<video:thumbnail_loc>${thumbnail}</video:thumbnail_loc>`);
  expect(sitemap).toContain(`<video:content_loc>${video.contentUrl}</video:content_loc>`);
  expect(sitemap).toContain(`<video:title>${video.name}</video:title>`);
  expect(read("robots.txt")).toContain(`Sitemap: ${ORIGIN}/video-sitemap.xml`);
  expect(read("robots.txt")).not.toContain("Disallow: /assets/");
});

test("the watch page's breadcrumb follows the docs sections", () => {
  const graph = jsonLd(read("docs/tutorials/swift-interop/index.html"))[0]["@graph"];
  const trail = graph.find((item) => item["@type"] === "BreadcrumbList").itemListElement;
  expect(trail.map((item) => [item.position, item.item])).toEqual([
    [1, `${ORIGIN}/`],
    [2, `${ORIGIN}/docs/`],
    [3, `${ORIGIN}/docs/tutorials/`],
    [4, `${ORIGIN}${WATCH}`],
  ]);
});

test("only the watch page carries the video's markup", () => {
  const pages = fs.readdirSync(SITE, { recursive: true }).filter((file) => file.endsWith(".html"));
  const watchPages = pages.filter((file) => read(file).includes('"@type": "VideoObject"'));
  expect(watchPages.sort()).toEqual(WATCH_PAGES.map((watch) => `${watch.slice(1)}index.html`).sort());
});

test("the package guide links to the canonical package video watch page", () => {
  const html = read("docs/package-manager/index.html");
  expect(html).toContain('class="video-card" href="/docs/packages-in-vscode/"');
  expect(jsonLd(html).some((data) => data["@type"] === "VideoObject")).toBe(false);
  expect(read("video-sitemap.xml")).toContain(`${ORIGIN}/docs/packages-in-vscode/`);
});

test("a ?t= link starts the video there and a chapter seeks in place", async ({ page }) => {
  await page.goto(`${WATCH}?t=50`);
  const video = page.locator("figure.video video");
  await expect.poll(() => video.evaluate((element) => element.currentTime)).toBe(50);
  await page.locator('.video-chapters a[data-seek="110"]').click();
  await expect.poll(() => video.evaluate((element) => element.currentTime)).toBeGreaterThanOrEqual(110);
  expect(new URL(page.url()).search).toBe("?t=110");
  await expect(page.locator(".video-transcript li").first()).toContainText("Osprey Storage");
});

test("the tutorials section lists each watch page", async ({ page }) => {
  await page.goto("/docs/tutorials/");
  await expect(page.locator(`a.video-card[href="${WATCH}"]`)).toBeVisible();
  await expect(page.locator(`.docs-nav-desktop a[href="/docs/tutorials/"]`)).toBeVisible();
});

test("WebVTT cues and durations parse the way the markup needs them", async () => {
  const { clock, isoDuration } = await import("../lib/videos.mjs");
  expect(clock(0)).toBe("0:00");
  expect(clock(194.537)).toBe("3:14");
  expect(clock(3725)).toBe("1:02:05");
  expect(isoDuration(194.537)).toBe("PT3M14S");
  expect(isoDuration(3600)).toBe("PT1H");
  expect(isoDuration(0)).toBe("PT0S");
});

async function captureHomepage(page, width) {
  await page.locator('.mobile-gallery').scrollIntoViewIfNeeded();
  for (const image of await page.locator('.mobile-gallery img').all()) {
    await image.evaluate((element) => element.decode());
  }
  await page.evaluate(() => window.scrollTo({ top: 0, behavior: "instant" }));
  await page.screenshot({ path: `/tmp/osprey-home-${width}.png`, fullPage: true });
  await page.screenshot({ path: `/tmp/osprey-hero-${width}.png` });
}

for (const width of [390, 1440]) {
  test(`homepage overview plays with captions and sound at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/");
    const player = page.locator("#overview-video");
    await player.scrollIntoViewIfNeeded();
    await expect(player).toBeVisible();
    await expect(player.locator('track[kind="captions"]')).toHaveAttribute("src", /overview\.en\.vtt$/);
    await player.evaluate((video) => video.play());
    await expect.poll(() => player.evaluate((video) => video.currentTime)).toBeGreaterThan(0.5);
    expect(await player.evaluate((video) => video.muted)).toBe(false);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await captureHomepage(page, width);
  });
}

for (const width of [390, 1440]) {
  test(`homepage showcase film plays directly under the hero at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/");
    await expect(page.locator(".home > section").nth(1)).toHaveId("showcase");
    await page.locator(".hero").getByRole("link", { name: "Watch the showcase" }).click();
    const player = page.locator("#showcase #showcase-video");
    await expect(player).toBeInViewport();
    await player.scrollIntoViewIfNeeded();
    await expect(player).toBeVisible();
    await expect(player).toHaveAttribute("poster", "/assets/videos/osprey-showcase-poster.jpg");
    await expect(player.locator("source")).toHaveAttribute("src", "/assets/videos/osprey-showcase.mp4");
    await expect(player).toHaveAttribute("aria-label", /web, Android, iPhone, macOS/);
    await player.evaluate((video) => video.play());
    await expect.poll(() => player.evaluate((video) => video.currentTime)).toBeGreaterThan(0.5);
    expect(await player.evaluate((video) => video.videoWidth)).toBe(1920);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  });
}

test("homepage Python GPU excerpt includes the actual tensor operation", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator('#gpt2 code.language-python')).toContainText('return perform(Tensor.mat_mul(a, b, rows, inner, cols))');
});

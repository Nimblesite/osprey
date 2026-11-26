// The sample site a developer hands over to the CMS [CMS-E2E]. The editor may
// change `content/` and `media/`; the layout in `_includes/` stays the
// developer's. The CMS never reads this file: the site's own pipeline does.
export default function (eleventyConfig) {
  eleventyConfig.addPassthroughCopy({ media: "media" });
  eleventyConfig.addGlobalData("layout", "layout.njk");
  return { dir: { input: "content", includes: "../_includes", output: "_site" } };
}

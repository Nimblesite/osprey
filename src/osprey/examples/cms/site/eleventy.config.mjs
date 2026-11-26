// The sample site a developer hands over to the CMS [CMS-E2E]. The editor may change `content/`
// and `media/`; the layouts in `_includes/`, the data in `_data/` and the styles in `assets/` stay
// the developer's. The CMS never reads this file: the site's own pipeline does.
const LONG_DATE = { year: "numeric", month: "long", day: "numeric", timeZone: "UTC" };
const WORDS_PER_MINUTE = 220;

export default function (eleventyConfig) {
  // `.markdown` pages are Markdown too, so every page the editor can open is built ([CMS-KINDS]).
  eleventyConfig.addExtension("markdown", { key: "md" });
  eleventyConfig.addPassthroughCopy({ media: "media", assets: "assets" });
  eleventyConfig.addGlobalData("layout", "page.njk");
  eleventyConfig.addFilter("readable", (date) => new Date(date).toLocaleDateString("en-GB", LONG_DATE));
  eleventyConfig.addFilter("minutes", (text) => Math.max(1, Math.round(String(text).split(/\s+/).length / WORDS_PER_MINUTE)));
  eleventyConfig.addCollection("posts", (api) => api.getFilteredByGlob("content/blog/*.md").sort((a, b) => b.date - a.date));
  return {
    templateFormats: ["md", "markdown", "njk"],
    markdownTemplateEngine: "njk",
    dir: { input: "content", includes: "../_includes", data: "../_data", output: "_site" },
  };
}

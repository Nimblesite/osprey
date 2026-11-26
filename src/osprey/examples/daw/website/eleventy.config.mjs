import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

const SITE_ROOT = path.resolve(import.meta.dirname, "src");

function contentHash(assetPath) {
  const fullPath = path.join(SITE_ROOT, assetPath.replace(/^\/+/, ""));
  if (!fs.existsSync(fullPath)) return "";
  return crypto
    .createHash("sha256")
    .update(fs.readFileSync(fullPath))
    .digest("hex")
    .slice(0, 10);
}

export default function (eleventyConfig) {
  eleventyConfig.addPassthroughCopy({ "src/assets": "assets" });
  eleventyConfig.addPassthroughCopy({ "../screens": "screens" });

  eleventyConfig.addWatchTarget("src/assets/");

  // [WEBSITE-ASSET-VERSIONS] Content-hashed URLs for site-local CSS/JS.
  eleventyConfig.addFilter("versioned", (assetPath) => {
    const hash = contentHash(assetPath);
    return hash ? `${assetPath}?v=${hash}` : assetPath;
  });

  return {
    dir: {
      input: "src",
      output: "_site",
      includes: "_includes",
      data: "_data",
    },
    htmlTemplateEngine: "njk",
    markdownTemplateEngine: "njk",
    templateFormats: ["njk", "md", "html"],
  };
}

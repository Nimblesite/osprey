import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const sharedNativeImages = path.resolve(root, "../website/src/assets/images/native");

const REFERENCED_IMAGES = [
  "tally-macos-counted.webp",
  "tally-ios.webp",
  "tally-android.webp",
  "windows-storage.png",
  "windows-flock.png",
  "macos-synth.png",
  "macos-painting.png",
  "web-bank.png",
];

function checkCssBudget() {
  const cssDir = path.join(root, "src", "css");
  let totalLines = 0;
  for (const file of fs.readdirSync(cssDir)) {
    if (!file.endsWith(".css")) continue;
    totalLines += fs.readFileSync(path.join(cssDir, file), "utf8").split("\n").length;
  }
  if (totalLines > 1800) {
    throw new Error(`CSS budget exceeded: ${totalLines} LOC (limit: 1800 LOC)`);
  }
}

export default function (eleventyConfig) {
  eleventyConfig.on("eleventy.before", () => {
    checkCssBudget();
  });

  eleventyConfig.addPassthroughCopy({ "src/css": "css" });
  eleventyConfig.addPassthroughCopy({ "src/js": "js" });

  // Copy verified native captures from the shared website assets at build time
  // so no duplicate binary screenshots are committed to git.
  for (const img of REFERENCED_IMAGES) {
    const srcPath = path.join(sharedNativeImages, img);
    if (fs.existsSync(srcPath)) {
      eleventyConfig.addPassthroughCopy({ [srcPath]: `assets/images/${img}` });
    }
  }

  // [WEBSITE-ASSET-VERSIONS] content-hashed URLs for site-local CSS & JS
  eleventyConfig.addFilter("assetUrl", (relPath) => {
    const clean = relPath.replace(/^\/+/, "");
    const fullPath = path.join(root, "src", clean);
    const hash = createHash("sha256").update(fs.readFileSync(fullPath)).digest("hex").slice(0, 12);
    return `/${clean}?v=${hash}`;
  });

  eleventyConfig.addFilter("readableDate", (dateObj) => {
    const d = dateObj instanceof Date ? dateObj : new Date(dateObj);
    return d.toLocaleDateString("en-US", {
      year: "numeric",
      month: "short",
      day: "numeric",
      timeZone: "UTC",
    });
  });

  eleventyConfig.addFilter("isoDate", (dateObj) => {
    const d = dateObj instanceof Date ? dateObj : new Date(dateObj);
    return d.toISOString().slice(0, 10);
  });

  eleventyConfig.addCollection("blog", (collectionApi) =>
    collectionApi
      .getFilteredByTag("blog")
      .sort((a, b) => b.date - a.date)
  );

  return {
    dir: {
      input: "src",
      output: "_site",
      includes: "_includes",
    },
    templateFormats: ["njk", "md", "html"],
    markdownTemplateEngine: "njk",
    htmlTemplateEngine: "njk",
  };
}

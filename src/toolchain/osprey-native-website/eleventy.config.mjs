import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.dirname(fileURLToPath(import.meta.url));
const sharedAssets = path.resolve(root, "../website/src/assets");

// Curated flagship screenshots & videos (zero toy demos on the front page)
const MEDIA_PASSTHROUGH = [
  ["images/native/macos-synth.png", "assets/images/macos-synth.png"],
  ["images/native/prism-spectral.webp", "assets/images/prism-spectral.webp"],
  ["images/native/macos-agent.png", "assets/images/macos-agent.png"],
  ["images/native/windows-storage.png", "assets/images/windows-storage.png"],
  ["images/guides/storage-macos.png", "assets/images/storage-macos.png"],
  ["images/native/web-bank.png", "assets/images/web-bank.png"],
  ["images/native/talon-mac.webp", "assets/images/talon-mac.webp"],
  ["images/mobile/issue-inbox-ios.png", "assets/images/issue-inbox-ios.png"],
  ["images/mobile/issue-inbox-android.png", "assets/images/issue-inbox-android.png"],
  ["images/blog/chromecast-from-mac-player.webp", "assets/images/chromecast-player.webp"],
  ["videos/osprey-prism-daw.mp4", "assets/videos/osprey-prism-daw.mp4"],
  ["videos/osprey-prism-daw-poster.jpg", "assets/videos/osprey-prism-daw-poster.jpg"],
  ["videos/osprey-prism-daw.en.vtt", "assets/videos/osprey-prism-daw.en.vtt"],
  ["videos/osprey-agent-native.mp4", "assets/videos/osprey-agent-native.mp4"],
  ["videos/osprey-agent-native-poster.jpg", "assets/videos/osprey-agent-native-poster.jpg"],
  ["videos/osprey-agent-native.en.vtt", "assets/videos/osprey-agent-native.en.vtt"],
  ["videos/osprey-light-orchestra.mp4", "assets/videos/osprey-light-orchestra.mp4"],
  ["videos/osprey-light-orchestra-poster.jpg", "assets/videos/osprey-light-orchestra-poster.jpg"],
  ["videos/osprey-light-orchestra.en.vtt", "assets/videos/osprey-light-orchestra.en.vtt"],
  ["videos/osprey-ui-testing-by-label.mp4", "assets/videos/osprey-ui-testing-by-label.mp4"],
  ["videos/osprey-ui-testing-by-label-poster.jpg", "assets/videos/osprey-ui-testing-by-label-poster.jpg"],
  ["videos/osprey-ui-testing-by-label.en.vtt", "assets/videos/osprey-ui-testing-by-label.en.vtt"],
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
  eleventyConfig.addPassthroughCopy({ "src/assets/brand": "assets/brand" });

  for (const [srcRel, dstRel] of MEDIA_PASSTHROUGH) {
    const srcPath = path.join(sharedAssets, srcRel);
    if (fs.existsSync(srcPath)) {
      eleventyConfig.addPassthroughCopy({ [srcPath]: dstRel });
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

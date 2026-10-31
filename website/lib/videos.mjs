// Video tutorials. One entry in src/_data/videos.json plus its two WebVTT
// tracks (captions and chapters) give everything a watch page needs, so no
// fact about a video is written twice:
//   - the player, with its captions, its chapters and seekable key moments
//   - the transcript, from the captions
//   - VideoObject structured data with a Clip per chapter, Open Graph video
//     tags and a poster for social cards (Google's video SEO guidance:
//     https://developers.google.com/search/docs/appearance/video)
//   - the video sitemap entry (src/video-sitemap.njk)
// A watch page names its video in front matter (`video: swift-interop`) and
// places `{% videoPlayer video %}` and `{% videoTranscript video %}`; any page
// links to one with `{% videoCard id, url %}`.
import { readFileSync } from "node:fs";

const SRC = new URL("../src/", import.meta.url);
const VIDEOS = JSON.parse(readFileSync(new URL("_data/videos.json", SRC), "utf8"));

const escapeHtml = (text) =>
  String(text).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

// `mm:ss.mmm` or `hh:mm:ss.mmm` as seconds.
const seconds = (stamp) => stamp.split(":").reduce((total, part) => total * 60 + Number(part), 0);

/** The cues of a WebVTT file under src/, as `{ start, end, text }`. */
export const cues = (file) =>
  readFileSync(new URL(file.replace(/^\//, ""), SRC), "utf8")
    .split(/\r?\n\r?\n/)
    .map((block) => block.trim().split(/\r?\n/))
    .filter((lines) => lines.some((line) => line.includes("-->")))
    .map((lines) => {
      const at = lines.findIndex((line) => line.includes("-->"));
      const [start, end] = lines[at].split("-->").map((stamp) => seconds(stamp.trim().split(/\s/)[0]));
      return { start, end, text: lines.slice(at + 1).join(" ") };
    });

// Whole hours, minutes and seconds in a number of seconds.
const hms = (value) => {
  const whole = Math.floor(value);
  return [Math.floor(whole / 3600), Math.floor((whole % 3600) / 60), whole % 60];
};

/** `m:ss` (or `h:mm:ss`) for a number of seconds. */
export const clock = (value) => {
  const [h, m, s] = hms(value);
  const pad = (n) => String(n).padStart(2, "0");
  return h ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
};

/** ISO 8601 duration, e.g. `PT3M15S`. */
export const isoDuration = (value) => {
  const [h, m, s] = hms(value);
  return `PT${h ? `${h}H` : ""}${m ? `${m}M` : ""}${s || !(h || m) ? `${s}S` : ""}`;
};

/** A video by id, with its chapters, transcript and duration worked out. */
export const video = (id) => {
  const entry = VIDEOS[id];
  if (!entry || id.startsWith("_")) throw new Error(`No video "${id}" in src/_data/videos.json`);
  const chapters = cues(entry.chapters);
  const starts = chapters.map((chapter) => Math.floor(chapter.start));
  if (new Set(starts).size !== starts.length) throw new Error(`Video "${id}" has two chapters starting at the same second`);
  return { id, ...entry, chapterTrack: entry.chapters, chapters, transcript: cues(entry.captions), duration: Math.floor(chapters.at(-1).end) };
};

// A link that starts the video at `start`: `?t=` is what Google's key moments
// deep-link to, and js/video.js seeks to it in place.
const seekLink = (start, label) =>
  `<a href="?t=${Math.floor(start)}" data-seek="${Math.floor(start)}"><time>${clock(start)}</time> ${label}</a>`;

/** The player and its chapter list. */
export const videoPlayer = (id, embedded = false) => {
  const v = video(id);
  const chapters = v.chapters.map((chapter) => `    <li>${seekLink(chapter.start, escapeHtml(chapter.text))}</li>`).join("\n");
  const player = `<figure class="video${embedded ? " video-embedded" : ""}" data-video="${v.id}">
  <video id="video-${v.id}" controls preload="${embedded ? "none" : "metadata"}" playsinline width="${v.width}" height="${v.height}" poster="${v.poster}" aria-label="${escapeHtml(v.name)}">
    <source src="${v.content}" type="video/mp4">
    <track kind="captions" srclang="en" label="English" src="${v.captions}" default>
    <track kind="chapters" srclang="en" label="Chapters" src="${v.chapterTrack}">
  </video>
  <figcaption>${escapeHtml(v.name)} (${clock(v.duration)}). ${embedded ? "English captions are available in the player." : "Captions are on; choose a chapter to jump to it."}</figcaption>
</figure>`;
  if (embedded) return player;
  return `${player}
<nav class="video-chapters" aria-label="Video chapters">
  <ol>
${chapters}
  </ol>
</nav>
<script src="/js/video.js" defer></script>`;
};

/** A poster that links to the video's watch page `url`, with its length. */
export const videoCard = (id, url) => {
  const v = video(id);
  return `<a class="video-card" href="${url}"><img src="${v.poster}" alt="${escapeHtml(v.posterAlt)}" width="${v.width}" height="${v.height}" loading="lazy"><span class="video-card-length">Watch the video, ${clock(v.duration)}</span></a>`;
};

/** The spoken words, line by line, each linked to its moment. */
export const videoTranscript = (id) =>
  `<ol class="video-transcript">\n${video(id)
    .transcript.map((cue) => `  <li>${seekLink(cue.start, "")}<span>${escapeHtml(cue.text)}</span></li>`)
    .join("\n")}\n</ol>`;

/** VideoObject structured data for the video on `pageUrl`. */
export const videoObject = (id, pageUrl, site) => {
  const v = video(id);
  const absolute = (path) => new URL(path, site.url).href;
  return {
    "@context": "https://schema.org",
    "@type": "VideoObject",
    "@id": `${pageUrl}#video`,
    name: v.name,
    description: v.description,
    thumbnailUrl: [absolute(v.poster)],
    uploadDate: v.uploadDate,
    duration: isoDuration(v.duration),
    contentUrl: absolute(v.content),
    encodingFormat: "video/mp4",
    width: v.width,
    height: v.height,
    inLanguage: "en",
    isFamilyFriendly: true,
    keywords: v.tags.join(", "),
    url: pageUrl,
    mainEntityOfPage: { "@id": `${pageUrl}#webpage` },
    creator: { "@type": "Person", name: site.author, url: site.authorUrl },
    publisher: { "@id": `${site.url}/#organization` },
    caption: { "@type": "MediaObject", contentUrl: absolute(v.captions), encodingFormat: "text/vtt", inLanguage: "en" },
    transcript: v.transcript.map((cue) => cue.text).join(" "),
    hasPart: v.chapters.map((chapter) => ({
      "@type": "Clip",
      name: chapter.text,
      startOffset: Math.floor(chapter.start),
      endOffset: Math.floor(chapter.end),
      url: `${pageUrl}?t=${Math.floor(chapter.start)}`,
    })),
  };
};

const setMeta = (html, attribute, name, value) => {
  const tag = new RegExp(`<meta ${attribute}="${name}" content="[^"]*">`);
  const meta = `<meta ${attribute}="${name}" content="${escapeHtml(value)}">`;
  return tag.test(html) ? html.replace(tag, meta) : html.replace("</head>", `  ${meta}\n</head>`);
};

/**
 * A watch page's head, rewritten for its video: the poster as the social
 * image, Open Graph video tags, full video previews allowed, and the
 * VideoObject. Every other page passes through untouched.
 */
export const withVideoMetadata = (html, pageUrl, site) => {
  const id = html.match(/<figure class="video" data-video="([^"]+)"/)?.[1];
  if (!id) return html;
  const v = video(id);
  const absolute = (path) => new URL(path, site.url).href;
  const tags = [
    ["name", "robots", "index, follow, max-video-preview:-1, max-image-preview:large"],
    ["property", "og:type", "video.other"],
    ["property", "og:image", absolute(v.poster)],
    ["property", "og:image:width", v.width],
    ["property", "og:image:height", v.height],
    ["property", "og:image:alt", v.posterAlt],
    ["property", "og:video", absolute(v.content)],
    ["property", "og:video:secure_url", absolute(v.content)],
    ["property", "og:video:type", "video/mp4"],
    ["property", "og:video:width", v.width],
    ["property", "og:video:height", v.height],
    ["property", "video:duration", v.duration],
    ["property", "video:release_date", v.uploadDate],
    ["name", "twitter:image", absolute(v.poster)],
    ["name", "twitter:image:alt", v.posterAlt],
  ];
  const json = JSON.stringify(videoObject(id, pageUrl, site), null, 2).replace(/</g, "\\u003c");
  const head = tags.reduce((result, [attribute, name, value]) => setMeta(result, attribute, name, value), html);
  return head.replace("</head>", `<script type="application/ld+json">\n${json}\n</script>\n</head>`);
};

/** Shortcodes, the head transform and the sitemap filter. */
export default function videos(eleventyConfig, site) {
  eleventyConfig.addShortcode("videoPlayer", videoPlayer);
  eleventyConfig.addShortcode("videoTranscript", videoTranscript);
  eleventyConfig.addShortcode("videoCard", videoCard);
  eleventyConfig.addFilter("video", video);
  eleventyConfig.addTransform("video-metadata", function (content, outputPath) {
    if (!outputPath || !outputPath.endsWith(".html")) return content;
    return withVideoMetadata(content, `${site.url}${this.page.url}`, site);
  });
}

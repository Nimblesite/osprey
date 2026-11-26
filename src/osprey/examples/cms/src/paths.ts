// osprey: namespace=cms
// osprey: module=Paths
/**
 * Which repository paths the editor may touch, and what kind of file each is.
 * Git runs through a shell, so this check is also what keeps a path from
 * injecting a command. [CMS-PATH-SAFE] [CMS-KINDS]
 */

/** The only bytes a path may hold. */
const SAFE_BYTES = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-/";
const MAX_PATH = 200;
const MARKDOWN = ["md", "markdown"];
const IMAGES = ["png", "jpg", "jpeg", "gif", "webp", "svg", "avif"];
const VIDEOS = ["mp4", "webm", "mov"];
const AUDIO = ["mp3", "wav", "ogg", "m4a", "flac"];

/** Whether every byte of `text` from `index` on is one of `SAFE_BYTES`. */
function safeFrom(text, index) {
  if (index === byteLength(text)) {
    return true;
  }
  const byte = unwrapOr(substring(text, index, satAdd(index, 1)), "'");
  return contains(SAFE_BYTES, byte) && safeFrom(text, satAdd(index, 1));
}

/** Whether every byte of `text` is a letter, digit, `.`, `_`, `-` or `/`. */
export function plainText(text: string) {
  return safeFrom(text, 0);
}

/** A segment is a visible, non-empty name. */
function visible(segment) {
  return segment !== "" && !startsWith(segment, ".");
}

function placed(segments, roots) {
  return segments.length > 1 && segments.filter(visible).length === segments.length && listContains(roots, unwrapOr(listGet(segments, 0), ""));
}

/** `path` itself when the editor may touch it, otherwise why not. */
export function safePath(roots: List<string>, path: string): Result<string, string> {
  if (byteLength(path) > MAX_PATH) {
    return Error("path is longer than 200 bytes");
  }
  if (!safeFrom(path, 0)) {
    return Error("path may only use A-Z a-z 0-9 . _ - /");
  }
  return placed(unwrapOr(split(path, "/"), []), roots) ? Success(path) : Error(`path must be a visible file under one of: ${join(roots, ", ")}`);
}

/** The lowercase extension of `path`, or "" when it has none. */
function extension(path) {
  const parts = unwrapOr(split(path, "."), []);
  return parts.length < 2 ? "" : toLowerCase(unwrapOr(listGet(parts, satSub(parts.length, 1)), ""));
}

/** `markdown`, `image`, `video`, `audio` or `other`, from the extension. */
export function kindFor(path: string) {
  const ext = extension(path);
  if (listContains(MARKDOWN, ext)) {
    return "markdown";
  }
  if (listContains(IMAGES, ext)) {
    return "image";
  }
  if (listContains(VIDEOS, ext)) {
    return "video";
  }
  return listContains(AUDIO, ext) ? "audio" : "other";
}

/** Whether `path` is an image, video or audio file. */
export function isMedia(path: string) {
  const kind = kindFor(path);
  return kind === "image" || kind === "video" || kind === "audio";
}

/** The folder holding a safe `path`: everything before its last `/`. */
export function folderOf(path: string) {
  const fromEnd = unwrapOr(indexOf(reverse(path), "/"), 0);
  return take(path, satSub(satSub(byteLength(path), fromEnd), 1));
}

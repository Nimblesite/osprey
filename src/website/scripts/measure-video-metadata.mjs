// Refresh measured duration metadata after replacing delivered MP4s.
import { readFileSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
const src = new URL("../src/", import.meta.url);
const manifest = new URL("_data/videos.json", src);
const videos = JSON.parse(readFileSync(manifest, "utf8"));
for (const [id, entry] of Object.entries(videos)) {
  if (id.startsWith("_")) continue;
  const file = fileURLToPath(new URL(entry.content.slice(1), src));
  const measured = Number(execFileSync("ffprobe", ["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", file], { encoding: "utf8" }));
  if (!Number.isFinite(measured) || measured <= 0) throw new Error(`Invalid duration: ${file}`);
  entry.durationSeconds = measured;
}
writeFileSync(manifest, `${JSON.stringify(videos, null, 2)}\n`);

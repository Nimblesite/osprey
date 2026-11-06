// Build-time release list. Eleventy awaits this default-exported async function
// and exposes the result as the global `releases` data. It pulls published
// releases from the GitHub Releases API so the status page always reflects the
// real, shipped versions without anyone hand-editing markdown.
//
// The fetch is best-effort: on any failure (offline build, rate limit, API
// change) it logs a warning and returns an empty list so the site still builds.
// A GITHUB_TOKEN in the environment (present in CI) raises the rate limit.

const REPO = "Nimblesite/osprey";
const API = `https://api.github.com/repos/${REPO}/releases?per_page=100`;

const headers = {
  "User-Agent": "osprey-website-build",
  Accept: "application/vnd.github+json",
};
if (process.env.GITHUB_TOKEN) {
  headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
}

const toDate = (iso) =>
  new Date(iso).toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  });

export default async function () {
  try {
    const res = await fetch(API, { headers });
    if (!res.ok) throw new Error(`GitHub API ${res.status} ${res.statusText}`);
    const raw = await res.json();

    const list = raw
      .filter((r) => !r.draft)
      .sort((a, b) => new Date(b.published_at) - new Date(a.published_at))
      .map((r) => ({
        tag: r.tag_name,
        name: r.name || r.tag_name,
        date: toDate(r.published_at),
        prerelease: r.prerelease,
        url: r.html_url,
      }));

    console.log(`[releases] fetched ${list.length} releases from ${REPO}`);
    return { latest: list[0] || null, list };
  } catch (err) {
    console.warn(`[releases] could not fetch releases: ${err.message}`);
    return { latest: null, list: [] };
  }
}

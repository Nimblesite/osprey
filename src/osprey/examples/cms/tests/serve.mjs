// The browser session's server [CMS-E2E]: builds the editor and the server, hands it a fresh site
// with a bare "published" remote, and serves until Playwright stops it; a second server holds a copy of
// the real Eleventy site for the author's journey.
import { AUTHOR_PORT, AUTHOR_SITE, BROWSER_PORT, BROWSER_SITE, ELEVENTY_SITE, build, buildEditor, createSite, siteFiles, start, stop } from "./fixture.mjs";

buildEditor();
const binary = build("gc");
createSite(AUTHOR_SITE, siteFiles(ELEVENTY_SITE));
createSite(BROWSER_SITE);
// The author's server first: Playwright waits on the browser server, so both listen once it answers.
const servers = [await start({ memory: "author", site: AUTHOR_SITE, port: AUTHOR_PORT }, binary),
  await start({ memory: "browser", site: BROWSER_SITE, port: BROWSER_PORT }, binary)];
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => Promise.all(servers.map(stop)).then(() => process.exit(0)));

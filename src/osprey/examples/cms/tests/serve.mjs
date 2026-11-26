// The browser session's server [CMS-E2E]: builds the editor and the server, hands it a fresh site
// with a bare "published" remote, and serves until Playwright stops it.
import { BROWSER_PORT, BROWSER_SITE, build, buildEditor, createSite, start, stop } from "./fixture.mjs";

buildEditor();
const binary = build("gc");
createSite(BROWSER_SITE);
const server = await start({ memory: "browser", site: BROWSER_SITE, port: BROWSER_PORT }, binary);
for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => stop(server).then(() => process.exit(0)));

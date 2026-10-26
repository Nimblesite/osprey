// Shared fixtures/data for the Osprey website e2e tests.

// Every distinct page template the site renders.
const PAGES = [
  { name: "home", path: "/", kind: "home" },
  { name: "spec-index", path: "/spec/", kind: "prose" },
  { name: "spec-prose", path: "/spec/0001-introduction/", kind: "prose" },
  { name: "docs-index", path: "/docs/", kind: "prose" },
  { name: "docs-effects", path: "/docs/effects/", kind: "prose" },
  { name: "docs-python-flavor", path: "/docs/python-flavor/", kind: "prose" },
  { name: "docs-python-flavor-for-ai", path: "/docs/python-flavor-for-ai/", kind: "prose" },
  { name: "docs-my-first-app", path: "/docs/my-first-app/", kind: "prose" },
  { name: "docs-mobile-apps", path: "/docs/mobile-apps/", kind: "prose" },
  { name: "docs-web-apps", path: "/docs/web-apps/", kind: "prose" },
  { name: "docs-mac-and-iphone-apps", path: "/docs/mac-and-iphone-apps/", kind: "prose" },
  { name: "docs-tutorials", path: "/docs/tutorials/", kind: "prose" },
  { name: "docs-tutorial-video", path: "/docs/tutorials/swift-interop/", kind: "prose" },
  { name: "docs-function", path: "/docs/functions/map/", kind: "prose" },
  { name: "docs-type", path: "/docs/types/string/", kind: "prose" },
  { name: "blog-index", path: "/blog/", kind: "listing" },
  { name: "blog-post", path: "/blog/2026-05-17-persistent-collections/", kind: "prose" },
  { name: "blog-gpt2", path: "/blog/2026-10-12-gpt2-on-the-gpu-surface/", kind: "prose" },
  {
    name: "blog-semver-post",
    path: "/blog/2026-07-25-semver-is-all-lies-please-stop/",
    kind: "prose",
  },
  { name: "status", path: "/status/", kind: "prose" },
  { name: "benchmarks", path: "/benchmarks/", kind: "prose" },
  { name: "docs-releases", path: "/docs/releases/", kind: "prose" },
  // Full-screen studio: the 3-tab demo fills the viewport, so the footer is
  // intentionally hidden and the page never scrolls (kind stays "wasm" so the
  // interaction + overflow tests still target it).
  { name: "wasm", path: "/wasm/", kind: "wasm", fullscreen: true },
  { name: "playground", path: "/playground/", kind: "app" },
];

const VIEWPORTS = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "tablet", width: 768, height: 1024 },
  { name: "mobile", width: 390, height: 844 },
];

// Per-page error suppressions. Empty: the playground's "totalResults is not
// defined" crash (an unescaped ${} in the embedded sample) is fixed at the
// source (website/scripts/update-playground.js now escapes ${), so the suite
// asserts every page — playground included — is genuinely error-free.
const KNOWN_ERRORS = [];

// Third-party/CDN request failures we don't control: the playground's Monaco
// editor and the Google Analytics beacon (gtag.js + /g/collect), whose async
// analytics requests are aborted by headless Chromium and are not site defects.
const IGNORED_REQUEST_HOSTS = /monaco|cdnjs|jsdelivr|unpkg|googleapis|gstatic|googletagmanager|google-analytics/;

// Attaches console-error / pageerror / failed-request collectors to a page.
function collectProblems(page, pageName) {
  const consoleErrors = [];
  const pageErrors = [];
  const failedRequests = [];
  page.on("console", (m) => {
    if (m.type() === "error" && !isKnown(pageName, m.text())) consoleErrors.push(m.text());
  });
  page.on("pageerror", (e) => {
    if (!isKnown(pageName, String(e))) pageErrors.push(String(e));
  });
  page.on("requestfailed", (r) => {
    // A video with preload="metadata" reads its header and the browser cancels
    // the rest of the download: that abort is the browser working, not a defect.
    const mediaAbort = r.resourceType() === "media" && r.failure()?.errorText === "net::ERR_ABORTED";
    if (!mediaAbort && !IGNORED_REQUEST_HOSTS.test(r.url())) failedRequests.push(`${r.url()} (${r.failure()?.errorText})`);
  });
  page.on("response", (r) => {
    if (r.status() >= 400 && !IGNORED_REQUEST_HOSTS.test(r.url())) failedRequests.push(`${r.status()} ${r.url()}`);
  });
  return { consoleErrors, pageErrors, failedRequests };
}

// Plain text paints one colour, so highlighted code is any code surface whose
// text carriers paint more than one.
const paintedColours = (carriers) =>
  carriers.evaluateAll((nodes) => new Set(nodes.map((node) => getComputedStyle(node).color)).size);

function isKnown(pageName, text) {
  return KNOWN_ERRORS.some((k) => k.page === pageName && k.pattern.test(text));
}

module.exports = { PAGES, VIEWPORTS, collectProblems, paintedColours };

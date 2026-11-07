// The iphone-safari project needs Playwright's WebKit. CI installs only
// Chromium, so install WebKit (and its Linux libraries) before the run.
const { execFileSync } = require("node:child_process");

module.exports = () => {
  const withDeps = process.platform === "linux" ? ["--with-deps"] : [];
  execFileSync("npx", ["playwright", "install", ...withDeps, "webkit"], { stdio: "inherit" });
};

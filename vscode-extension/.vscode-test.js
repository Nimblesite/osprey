const { defineConfig } = require("@vscode/test-cli");

module.exports = defineConfig({
  files: "out/test/suite/**/*.test.js",
  srcDir: "client/src",
  version: "stable",
  mocha: {
    ui: "tdd",
    timeout: 10000,
    color: true,
    // Run a focused subset by exporting OSPREY_TEST_GREP (a mocha grep pattern),
    // e.g. OSPREY_TEST_GREP="Debugger E2E Workflows". Unset runs the full suite.
    ...(process.env.OSPREY_TEST_GREP
      ? { grep: process.env.OSPREY_TEST_GREP }
      : {}),
  },
  launchArgs: ["--disable-extensions", "--disable-workspace-trust"],
  coverage: {
    reporter: ["text-summary", "json-summary", "html"],
    include: ["out/client/src/**/*.js"],
    exclude: ["out/test/**", "**/node_modules/**"],
    includeAll: true,
  },
});

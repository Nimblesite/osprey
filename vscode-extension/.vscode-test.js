const { defineConfig } = require("@vscode/test-cli");

// SHAPE MATTERS. `defineConfig` is the identity function, and the loader only
// reads a `coverage` block from the TOP LEVEL: a config object without a
// `tests` key is wrapped as `{ tests: [config] }`, which buries `coverage`
// inside the test entry where nothing ever looks at it. This file used to be
// flat, so every coverage option below — include, exclude, includeAll — was
// silently discarded and c8 ran on its defaults. The gate was reading a number
// that counted the test harnesses (dap-harness, osprey-test-env,
// test-explorer-harness) as product code.
//
// The patterns are absolute-path globs on purpose: @vscode/test-cli forces
// `relativePath = false` on the exclusion matcher to work around an Istanbul
// casing bug, so a bare "out/test/**" is compared against a full path and can
// never match. Every pattern must lead with `**/`.
module.exports = defineConfig({
  tests: [
    {
      files: "out/test/suite/**/*.test.js",
      srcDir: "client/src",
      version: "stable",
      mocha: {
        ui: "tdd",
        timeout: 10000,
        color: true,
        // Run a focused subset by exporting OSPREY_TEST_GREP (a mocha grep
        // pattern), e.g. OSPREY_TEST_GREP="Debugger E2E Workflows". Unset runs
        // the full suite.
        ...(process.env.OSPREY_TEST_GREP
          ? { grep: process.env.OSPREY_TEST_GREP }
          : {}),
      },
      launchArgs: ["--disable-extensions", "--disable-workspace-trust"],
    },
  ],
  coverage: {
    reporter: ["text-summary", "json-summary", "html"],
    // Product code only. A harness is exercised by definition, so counting it
    // inflates the number the threshold gate reads.
    include: ["**/out/client/src/**/*.js"],
    exclude: [
      "**/out/test/**",
      "**/node_modules/**",
      "**/.vscode-test/**",
    ],
    includeAll: true,
  },
});

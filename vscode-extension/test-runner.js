#!/usr/bin/env node

const { spawn } = require("child_process");
const path = require("path");

// Simple test runner that VS Code can discover
function runTests() {
  console.log("🧪 Running Osprey Extension Tests...\n");

  const testProcess = spawn("npm", ["test"], {
    cwd: __dirname,
    stdio: "inherit",
    shell: true,
  });

  testProcess.on("close", (code) => {
    if (code === 0) {
      console.log("\n✅ All tests passed!");
    } else {
      console.log("\n❌ Tests failed!");
      process.exit(code);
    }
  });

  testProcess.on("error", (err) => {
    console.error("Failed to start test process:", err);
    process.exit(1);
  });
}

if (require.main === module) {
  runTests();
}

module.exports = { runTests };

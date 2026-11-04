#!/usr/bin/env node

const { spawn } = require("child_process");
const fs = require("fs");
const path = require("path");

const COMMANDS = {
  test: "Run all tests using VS Code test CLI",
  "test:watch": "Run tests in watch mode",
  "test:debug": "Run tests with debugging enabled",
  "test:unit": "Run only unit tests",
  "test:integration": "Run only integration tests",
  "test:coverage": "Run tests with coverage (if configured)",
  compile: "Compile TypeScript before testing",
  clean: "Clean compiled output",
  help: "Show this help message",
};

function showHelp() {
  console.log("\n🧪 VS Code Extension Test Launcher\n");
  console.log("Available commands:");
  Object.entries(COMMANDS).forEach(([cmd, desc]) => {
    console.log(`  ${cmd.padEnd(15)} - ${desc}`);
  });
  console.log("\nExamples:");
  console.log("  node scripts/test-launcher.js test");
  console.log("  node scripts/test-launcher.js test:debug");
  console.log("  node scripts/test-launcher.js compile");
  console.log("");
}

function runCommand(command, args = [], options = {}) {
  return new Promise((resolve, reject) => {
    const proc = spawn(command, args, {
      stdio: "inherit",
      shell: true,
      cwd: path.resolve(__dirname, ".."),
      ...options,
    });

    proc.on("close", (code) => {
      if (code === 0) {
        resolve();
      } else {
        reject(new Error(`Command failed with exit code ${code}`));
      }
    });

    proc.on("error", reject);
  });
}

async function executeCommand(cmd) {
  const startTime = Date.now();

  try {
    switch (cmd) {
      case "test":
        console.log("🧪 Running all tests...\n");
        await runCommand("npm", ["test"]);
        break;

      case "test:watch":
        console.log("👀 Running tests in watch mode...\n");
        await runCommand("npm", ["run", "watch"]);
        break;

      case "test:debug":
        console.log("🐛 Running tests with debugging...\n");
        console.log(
          'Use VS Code debugger with "Extension Tests" configuration',
        );
        await runCommand("code", [".vscode/launch.json"]);
        break;

      case "test:unit":
        console.log("🔧 Running unit tests only...\n");
        await runCommand("npm", ["run", "test:unit"]);
        break;

      case "test:integration":
        console.log("🔗 Running integration tests only...\n");
        await runCommand("vscode-test", ["--label", "integration"]);
        break;

      case "test:coverage":
        console.log("📊 Running tests with coverage...\n");
        console.log("Coverage not configured yet. Add nyc or c8 for coverage.");
        break;

      case "compile":
        console.log("🔨 Compiling TypeScript...\n");
        await runCommand("npm", ["run", "compile"]);
        break;

      case "clean":
        console.log("🧹 Cleaning compiled output...\n");
        if (fs.existsSync("out")) {
          fs.rmSync("out", { recursive: true, force: true });
          console.log("✅ Cleaned output directory");
        } else {
          console.log("ℹ️  Output directory already clean");
        }
        break;

      case "help":
        showHelp();
        break;

      default:
        console.error(`❌ Unknown command: ${cmd}`);
        showHelp();
        process.exit(1);
    }

    const duration = Date.now() - startTime;
    console.log(`\n✅ Command '${cmd}' completed in ${duration}ms`);
  } catch (error) {
    const duration = Date.now() - startTime;
    console.error(`\n❌ Command '${cmd}' failed after ${duration}ms:`);
    console.error(error.message);
    process.exit(1);
  }
}

async function main() {
  const command = process.argv[2] || "help";

  if (command === "help" || command === "--help" || command === "-h") {
    showHelp();
    return;
  }

  // Check if we're in the right directory
  if (!fs.existsSync("package.json")) {
    console.error(
      "❌ This script must be run from the extension root directory",
    );
    process.exit(1);
  }

  // Check if VS Code test CLI is installed
  if (!fs.existsSync("node_modules/@vscode/test-cli")) {
    console.error(
      "❌ @vscode/test-cli not found. Run: npm install --save-dev @vscode/test-cli",
    );
    process.exit(1);
  }

  await executeCommand(command);
}

if (require.main === module) {
  main().catch(console.error);
}

module.exports = { executeCommand, showHelp };

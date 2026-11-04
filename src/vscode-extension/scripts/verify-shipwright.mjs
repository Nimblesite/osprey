import { readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const extensionRoot = resolve(scriptDir, "..");
const repoRoot = resolve(extensionRoot, "..");
const manifestPath = join(repoRoot, "shipwright.json");
const manifestSchemaPath = join(repoRoot, "scripts", "schemas", "shipwright.schema.json");

function readJson(file) {
  return JSON.parse(readFileSync(file, "utf8"));
}

function validateJson(schemaPath, dataPath, label) {
  const ajv = new Ajv2020({ allErrors: true, strict: false });
  addFormats(ajv);
  const validate = ajv.compile(readJson(schemaPath));
  const data = readJson(dataPath);
  if (!validate(data)) {
    const errors = validate.errors
      ?.map((error) => `${error.instancePath || "/"} ${error.message}`)
      .join("\n");
    throw new Error(`${label} failed validation:\n${errors}`);
  }
  console.log(`${label}: valid`);
}

function verifyManifest() {
  validateJson(manifestSchemaPath, manifestPath, "shipwright.json");
}

const command = process.argv[2];
if (command === "manifest") {
  verifyManifest();
} else {
  console.error("Usage: node scripts/verify-shipwright.mjs manifest");
  process.exit(2);
}

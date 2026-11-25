// [LINTS-DEADCODE-REACH] Product exports cannot be kept alive by tests.
import { readFileSync } from "node:fs";
import { dirname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import ts from "typescript";

const root = resolve(process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), ".."));
const product = join(root, "client", "src") + sep;
const manifest = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const main = resolve(root, manifest.main.replace(/^\.\/out\//, "").replace(/\.js$/, ".ts"));
const config = ts.getParsedCommandLineOfConfigFile(join(root, "tsconfig.json"), {}, {
  ...ts.sys,
  onUnRecoverableConfigFileDiagnostic: (diagnostic) => {
    throw new Error(ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n"));
  },
});
const host = {
  ...ts.sys,
  // ts.sys holds a boolean; the language service calls a function (true on Linux).
  useCaseSensitiveFileNames: () => ts.sys.useCaseSensitiveFileNames,
  getCompilationSettings: () => config.options,
  getScriptFileNames: () => config.fileNames,
  getScriptVersion: () => "0",
  getScriptSnapshot: (file) => {
    const source = ts.sys.readFile(file);
    return source === undefined ? undefined : ts.ScriptSnapshot.fromString(source);
  },
  getCurrentDirectory: () => root,
  getDefaultLibFileName: (options) => ts.getDefaultLibFilePath(options),
};

const service = ts.createLanguageService(host);
const program = service.getProgram();
if (!program.getSourceFile(main)) throw new Error(`Missing VS Code entry point: ${main}`);
const checker = program.getTypeChecker();
const extensionHooks = new Set(["activate", "deactivate"]);
const valueKinds = ts.SymbolFlags.Function | ts.SymbolFlags.Class | ts.SymbolFlags.Variable;
const isProduct = (file) => file.startsWith(product);

function nodeAt(node, position) {
  return ts.forEachChild(node, (child) =>
    child.pos <= position && position < child.end ? nodeAt(child, position) : undefined,
  ) ?? node;
}

function isProductUse(ref) {
  if (ref.isDefinition || !isProduct(ref.fileName)) return false;
  const source = program.getSourceFile(ref.fileName);
  for (let node = nodeAt(source, ref.textSpan.start); node; node = node.parent) {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node) ||
        ts.isTypeQueryNode(node) || ts.isImportTypeNode(node)) return false;
  }
  return true;
}

function externallyUsed(file, declaration) {
  const references = service.findReferences(file.fileName, declaration.name.getStart(file)) ?? [];
  return references.some(({ references }) => references.some((ref) =>
    isProductUse(ref) &&
    (ref.fileName !== file.fileName || ref.textSpan.start < declaration.getStart(file) ||
      ref.textSpan.start >= declaration.end),
  ));
}

function unusedExports(file) {
  const module = checker.getSymbolAtLocation(file);
  if (!module) return [];
  return checker.getExportsOfModule(module).flatMap((item) => {
    if (file.fileName === main && extensionHooks.has(item.name)) return [];
    const declaration = item.valueDeclaration;
    if (!(item.flags & valueKinds) || !declaration?.name || declaration.getSourceFile() !== file ||
        externallyUsed(file, declaration)) return [];
    const { line } = file.getLineAndCharacterOfPosition(declaration.name.getStart(file));
    return [`${relative(root, file.fileName)}:${line + 1}  ${item.name}`];
  });
}

// VS Code calls activate and deactivate on the manifest entry point.
// All other executable exports need a reference in shipped TypeScript.
const findings = program.getSourceFiles()
  .filter((file) => isProduct(file.fileName) && !file.isDeclarationFile)
  .flatMap(unusedExports);
if (findings.length) {
  console.error(findings.join("\n"));
  process.exitCode = 1;
}

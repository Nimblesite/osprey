// Tests for which TypeScript- and Python-flavor files the Osprey language
// server claims ([FLAVOR-TS-SELECT], [FLAVOR-TS-VERIFICATION]): a file listed
// in a project manifest or starting with its flavor marker, and nothing else,
// so an ordinary `.ts` or `.py` file stays the editor's own.

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import {
  claimedSources,
  flavorSelectors,
  listedSources,
  markedFlavor,
} from "../../client/src/flavor-documents";

suite("flavor documents", () => {
  const manifest = path.resolve("/work/shop/osprey.toml");

  test("a manifest's listed sources are claimed by absolute path", () => {
    const text = [
      "[project]",
      'name = "shop"',
      'source_roots = ["src"]',
      'typescript_sources = ["src/main.ts", "src/pricing.ts"]',
      "python_sources = [",
      '  "src/tax.py",',
      "]",
      'entry = "src/main.ts"',
    ].join("\n");
    assert.deepStrictEqual(listedSources(manifest, text), [
      { language: "typescript", path: path.resolve("/work/shop/src/main.ts") },
      {
        language: "typescript",
        path: path.resolve("/work/shop/src/pricing.ts"),
      },
      { language: "python", path: path.resolve("/work/shop/src/tax.py") },
    ]);
  });

  test("an entry opts in its shared extension only inside the project section", () => {
    const text = '[package]\nentry = "other.ts"\n[project] # root\nentry = "main.test.py"\n[dependencies]\npython_sources = ["helper.py"]\n';
    assert.deepStrictEqual(listedSources(manifest, text), [
      { language: "python", path: path.resolve("/work/shop/main.test.py") },
    ]);
  });

  test("a manifest without flavor lists claims nothing", () => {
    assert.deepStrictEqual(
      listedSources(manifest, '[project]\nname = "plain"\n'),
      [],
    );
    assert.deepStrictEqual(
      listedSources(manifest, '# typescript_sources = ["a.ts"]\n'),
      [],
    );
  });

  test("only a leading marker opts a file in", () => {
    assert.strictEqual(
      markedFlavor("// osprey: flavor=typescript\nconst x = 1;\n"),
      "typescript",
    );
    assert.strictEqual(
      markedFlavor("\n\n# osprey: flavor=python\nx = 1\n"),
      "python",
    );
    assert.strictEqual(
      markedFlavor("const x = 1;\n// osprey: flavor=typescript\n"),
      undefined,
    );
    assert.strictEqual(
      markedFlavor("import express from 'express';\n"),
      undefined,
    );
    assert.strictEqual(markedFlavor(""), undefined);
  });

  test("selectors name each claimed file once, in its own language", () => {
    const main = path.resolve("/work/shop/src/main.ts");
    const tax = path.resolve("/work/shop/src/tax.py");
    assert.deepStrictEqual(
      flavorSelectors([
        { language: "typescript", path: main },
        { language: "python", path: tax },
        { language: "typescript", path: main },
      ]),
      [
        { scheme: "file", language: "typescript", pattern: main },
        { scheme: "file", language: "python", pattern: tax },
      ],
    );
  });

  test("a workspace walk claims listed and marked files and skips dependencies", () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-flavors-"));
    const write = (relative: string, text: string) => {
      fs.mkdirSync(path.dirname(path.join(root, relative)), {
        recursive: true,
      });
      fs.writeFileSync(path.join(root, relative), text);
    };
    write(
      "shop/osprey.toml",
      '[project]\ntypescript_sources = ["src/main.ts"]\n',
    );
    write("shop/src/main.ts", "console.log(1);\n");
    write("shop/src/plain.ts", "export const x = 1;\n");
    write("tools/script.ts", "// osprey: flavor=typescript\nconsole.log(2);\n");
    write("tools/wrong.py", "// osprey: flavor=typescript\n");
    write("tools/model.py", "# osprey: flavor=python\nx = 1\n");
    write("node_modules/dep/index.ts", "// osprey: flavor=typescript\n");
    try {
      const claimed = claimedSources([root]).map((source) => [
        source.language,
        path.relative(root, source.path),
      ]);
      assert.deepStrictEqual(claimed.sort(), [
        ["python", path.join("tools", "model.py")],
        ["typescript", path.join("shop", "src", "main.ts")],
        ["typescript", path.join("tools", "script.ts")],
      ]);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});

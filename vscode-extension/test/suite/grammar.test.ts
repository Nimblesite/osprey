// TextMate grammar tests for both Osprey flavors. The grammars ship in the
// VSIX and decide every colour a user sees, yet nothing executed them until
// now: four rules were dead or wrong in a released extension and no check
// could notice, because a wrong scope still renders — just in the wrong
// colour. These tests tokenize with the SAME engine VS Code uses
// (vscode-textmate over vscode-oniguruma) so a passing assertion is a
// statement about the editor, not about a regex read by eye.
//
// Two layers, and both are load-bearing:
//
//   1. Named regressions — one assertion per defect found on real files, so a
//      reintroduced bug names itself.
//   2. A corpus invariant sweep over EVERY .osp/.ospml in the repository. Unit
//      fixtures only cover syntax someone thought to write down; the corpus is
//      the syntax that actually exists. It caught the missing `effect`,
//      `handle`, `perform` and `extern` keywords that no fixture would have.

import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import * as vsctm from "vscode-textmate";
import * as oniguruma from "vscode-oniguruma";
import { extensionRoot } from "./osprey-test-env";

const repoRoot = path.resolve(extensionRoot, "..");
const syntaxesDir = path.join(extensionRoot, "syntaxes");

const GRAMMARS: Record<string, string> = {
  "source.osprey": "osprey.tmGrammar.json",
  "source.osprey-ml": "osprey-ml.tmLanguage.json",
};

/** One token: its text and its scopes with the grammar's root scope dropped. */
interface Token {
  text: string;
  scopes: string[];
  line: number;
}

let registry: vsctm.Registry;

async function grammarFor(scope: string): Promise<vsctm.IGrammar> {
  const grammar = await registry.loadGrammar(scope);
  assert.ok(grammar, `grammar ${scope} loads`);
  return grammar;
}

/** Tokenize `source`, keeping the ruleStack across lines as an editor does. */
function tokenize(grammar: vsctm.IGrammar, source: string): Token[] {
  const tokens: Token[] = [];
  let stack = vsctm.INITIAL;
  source.split("\n").forEach((line, index) => {
    const result = grammar.tokenizeLine(line, stack);
    stack = result.ruleStack;
    for (const token of result.tokens) {
      const text = line.slice(token.startIndex, token.endIndex);
      if (text.trim()) {
        tokens.push({ text, scopes: token.scopes.slice(1), line: index + 1 });
      }
    }
  });
  return tokens;
}

/** The scopes of the first token whose text is exactly `text`. */
function scopesOf(tokens: Token[], text: string): string {
  const found = tokens.find((token) => token.text.trim() === text);
  assert.ok(found, `token ${JSON.stringify(text)} exists`);
  return found.scopes.join("|");
}

function assertScoped(tokens: Token[], text: string, expected: string): void {
  const scopes = scopesOf(tokens, text);
  assert.ok(
    scopes.includes(expected),
    `${JSON.stringify(text)} should be ${expected}, got ${scopes || "NONE"}`,
  );
}

/** Every Osprey source file in the repository, excluding build output. */
function corpusFiles(): string[] {
  const out: string[] = [];
  const walk = (dir: string): void => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        if (entry.name !== "build" && entry.name !== "node_modules") {
          walk(full);
        }
      } else if (/\.(osp|ospml)$/.test(entry.name)) {
        out.push(full);
      }
    }
  };
  for (const top of ["tests", "examples"]) {
    walk(path.join(repoRoot, top));
  }
  return out.sort();
}

suite("TextMate grammars", () => {
  suiteSetup(async () => {
    const wasmPath = path.join(
      path.dirname(require.resolve("vscode-oniguruma")),
      "onig.wasm",
    );
    const wasm = fs.readFileSync(wasmPath);
    await oniguruma.loadWASM(
      wasm.buffer.slice(
        wasm.byteOffset,
        wasm.byteOffset + wasm.byteLength,
      ) as ArrayBuffer,
    );
    registry = new vsctm.Registry({
      onigLib: Promise.resolve({
        createOnigScanner: (sources) => new oniguruma.OnigScanner(sources),
        createOnigString: (value) => new oniguruma.OnigString(value),
      }),
      loadGrammar: async (scope) => {
        const file = GRAMMARS[scope];
        if (!file) {
          return null;
        }
        const full = path.join(syntaxesDir, file);
        return vsctm.parseRawGrammar(fs.readFileSync(full, "utf8"), full);
      },
    });
  });

  test("both grammars declare the scopes package.json contributes", async () => {
    const manifest = JSON.parse(
      fs.readFileSync(path.join(extensionRoot, "package.json"), "utf8"),
    ) as { contributes: { grammars: { scopeName: string; path: string }[] } };
    for (const contributed of manifest.contributes.grammars) {
      const grammar = JSON.parse(
        fs.readFileSync(path.join(extensionRoot, contributed.path), "utf8"),
      ) as { scopeName: string };
      assert.strictEqual(
        grammar.scopeName,
        contributed.scopeName,
        `${contributed.path} declares the contributed scope`,
      );
      await grammarFor(contributed.scopeName);
    }
  });

  // A match arm is the single most common construct in an ML program, and
  // every one of them was mis-tokenized: `function-heads` ended on `(=)(?!=)`,
  // which excludes `==` but not `=>`, so `true => ...` parsed as a function
  // named `true` being defined at column 0. A column-0 match beats the boolean
  // and arrow rules on POSITION regardless of list order, so the arm's `=>`
  // was split into `=` and `>` and the whole arm lost its colours.
  test("ML match arms keep their boolean, arrow and pattern scopes", async () => {
    const grammar = await grammarFor("source.osprey-ml");
    const tokens = tokenize(
      grammar,
      [
        "find doc id index count = match index >= count",
        '    true => LocalNote(note = "", priority = "normal")',
        "    false => find doc id ((index + 1) ?: count) count",
      ].join("\n"),
    );
    assertScoped(tokens, "true", "constant.language.boolean");
    assertScoped(tokens, "false", "constant.language.boolean");
    assertScoped(tokens, "=>", "keyword.operator.arrow");
    assert.ok(
      !tokens.some((t) => t.text === "true" && t.scopes.join("|").includes("entity.name.function")),
      "a match arm is not a function definition",
    );
    // The real binding on line 1 still reads as one.
    assertScoped(tokens, "find", "entity.name.function");
    assertScoped(tokens, "match", "keyword.control");
  });

  test("ML `?:` is one error-propagation operator", async () => {
    const grammar = await grammarFor("source.osprey-ml");
    const tokens = tokenize(grammar, "doc = jsonParse source ?: 0");
    assertScoped(tokens, "?:", "keyword.operator");
    assert.ok(
      !tokens.some((t) => t.text === "?"),
      "`?:` must not split into a bare `?` and a separator",
    );
  });

  // `export` matched at the same column as the binding rule and won, so the
  // exported name — the one thing a reader scans for — stayed unhighlighted.
  test("ML export binding names both the modifier and the function", async () => {
    const grammar = await grammarFor("source.osprey-ml");
    const tokens = tokenize(grammar, "export get source id =\n    found");
    assertScoped(tokens, "export", "storage.modifier.export");
    assertScoped(tokens, "get", "entity.name.function");
    assertScoped(tokens, "source id", "variable.parameter");
  });

  // `mut counter = 0` is a mutable binding, not a function named `mut`.
  test("ML mutable bindings keep the keyword and the bound name apart", async () => {
    const grammar = await grammarFor("source.osprey-ml");
    const tokens = tokenize(grammar, "    mut counter = 0");
    assertScoped(tokens, "mut", "keyword.declaration");
    assertScoped(tokens, "counter", "entity.name.function");
  });

  // The `type` block ended at the first newline, so a record's fields — which
  // live on the following indented lines — were never inside it. The generic
  // `keywords` rule also consumed `type`, `namespace`, `module` and `import`
  // at the same column as the declaration rules, making those rules dead.
  test("ML type declarations highlight their name and field lines", async () => {
    const grammar = await grammarFor("source.osprey-ml");
    const tokens = tokenize(
      grammar,
      ["type LocalNote =", "    note : string", "    priority : string", "", "module Annotations"].join("\n"),
    );
    assertScoped(tokens, "type", "keyword.declaration.type");
    assertScoped(tokens, "LocalNote", "entity.name.type");
    assertScoped(tokens, "note", "variable.other.member");
    assertScoped(tokens, "string", "support.type.primitive");
    assertScoped(tokens, "module", "keyword.declaration.module");
    assertScoped(tokens, "Annotations", "entity.name.type.module");
  });

  test("ML namespace and import declarations name what follows them", async () => {
    const grammar = await grammarFor("source.osprey-ml");
    const tokens = tokenize(grammar, "namespace mobile\nimport bank::Json");
    assertScoped(tokens, "namespace", "keyword.declaration.namespace");
    assertScoped(tokens, "mobile", "entity.name.namespace");
    assertScoped(tokens, "import", "keyword.control.import");
    assertScoped(tokens, "bank::Json", "entity.name.type.module");
  });

  // Algebraic effects are a flagship feature and the Default grammar had no
  // rule for any of their keywords: `effect`, `handle`, `perform`, `resume`
  // and the `in` terminator all rendered as plain identifiers.
  test("Default flavor highlights the effect vocabulary", async () => {
    const grammar = await grammarFor("source.osprey");
    const tokens = tokenize(
      grammar,
      [
        "effect ArithmeticFailure {",
        "    decide: fn(string) -> int",
        "}",
        "fn preserveFailure(a, b) = handle ArithmeticFailure",
        "    decide message => resume(perform Other.ask())",
        "in addThroughPolicy(a, b)",
      ].join("\n"),
    );
    for (const [word, scope] of [
      ["effect", "keyword.declaration"],
      ["handle", "keyword.control"],
      ["resume", "keyword.control"],
      ["perform", "keyword.control"],
      ["in", "keyword.control"],
    ] as const) {
      assertScoped(tokens, word, scope);
    }
  });

  test("ML flavor highlights extern, resume and fiber syntax", async () => {
    const grammar = await grammarFor("source.osprey-ml");
    const tokens = tokenize(
      grammar,
      [
        "extern sqlite3_open (filename : string) (ppDb : Ptr) -> int",
        "answer = handle Prompt",
        "    ask => resume 7",
        "    in await (spawn (work ()))",
      ].join("\n"),
    );
    assertScoped(tokens, "extern", "keyword.declaration");
    assertScoped(tokens, "resume", "keyword.control");
    assertScoped(tokens, "await", "keyword.fiber");
    assertScoped(tokens, "spawn", "keyword.fiber");
  });

  // `once`, `many` and `replayable` qualify an operation; the SAME words are
  // legal operation names, which tests/effects/multiplicity pins deliberately.
  // The rule requires the qualified operation to follow, so both readings keep
  // their colours — a blanket keyword rule would have mis-coloured real code.
  test("multiplicity modifiers stay distinct from identically named operations", async () => {
    for (const scope of ["source.osprey", "source.osprey-ml"]) {
      const grammar = await grammarFor(scope);
      const modifier = tokenize(grammar, "    once charge: fn(int) -> int");
      assertScoped(modifier, "once", "storage.modifier.multiplicity");
      assertScoped(modifier, "charge", "entity.name.function.effect");

      const operationName = tokenize(grammar, "    once: fn() -> int");
      assert.ok(
        !scopesOf(operationName, "once").includes("storage.modifier"),
        `${scope}: an operation named \`once\` is not a modifier`,
      );
    }
  });

  test("`static` qualifies a staged effect at declaration and discharge", async () => {
    const grammar = await grammarFor("source.osprey");
    const declared = tokenize(grammar, "static effect Scale {");
    assertScoped(declared, "static", "storage.modifier.static");
    const discharged = tokenize(grammar, "let total = handle static Scale");
    assertScoped(discharged, "static", "storage.modifier.static");
  });

  // The sweep. Every invariant here is one a reader can state without knowing
  // the grammar: an arrow is an arrow, a boolean is a boolean, and a word that
  // can only ever be a keyword is highlighted as one. Words that are BOTH
  // keyword and identifier in real programs are excluded by name below,
  // because the grammar cannot decide them from a regex and a wrong guess
  // would mis-colour working code.
  test("no Osprey file in the repository tokenizes with a broken operator or keyword", async () => {
    // Contextual words: legal identifiers in the corpus today. `stage`, `out`
    // and `extra` are record fields and GPU variables; `abort`, `alias`,
    // `once`, `many` and `replayable` are operation names in
    // tests/effects/multiplicity. Their keyword positions are covered by the
    // contextual rules asserted above.
    const CONTEXTUAL = new Set([
      "abort", "alias", "extra", "kernel", "many", "once", "out",
      "replayable", "stage", "static",
    ]);
    const ALWAYS_KEYWORD =
      /^(fn|let|match|type|module|namespace|import|effect|handle|perform|signature|export|extern|resume|in|spawn|await|yield|send|recv|mut|handler|do|opaque|state)$/;

    const files = corpusFiles();
    assert.ok(files.length > 100, `corpus is present (${files.length} files)`);
    const problems: string[] = [];
    for (const file of files) {
      const grammar = await grammarFor(
        file.endsWith(".ospml") ? "source.osprey-ml" : "source.osprey",
      );
      const relative = path.relative(repoRoot, file);
      const lines = fs.readFileSync(file, "utf8").split("\n");
      let stack = vsctm.INITIAL;
      lines.forEach((line, index) => {
        const result = grammar.tokenizeLine(line, stack);
        stack = result.ruleStack;
        for (const token of result.tokens) {
          const text = line.slice(token.startIndex, token.endIndex);
          const scopes = token.scopes.join("|");
          if (!text.trim() || /string|comment/.test(scopes)) {
            continue;
          }
          const at = `${relative}:${index + 1}`;
          const report = (why: string): void => {
            if (problems.length < 20) {
              problems.push(`${at} ${why}: ${JSON.stringify(text)} → ${token.scopes.slice(1).join("|") || "NONE"}`);
            }
          };
          const ahead = line.slice(token.startIndex, token.startIndex + 2);
          if (text === "=>" && !scopes.includes("arrow")) {
            report("arrow lost its scope");
          }
          if ((text === "=" || text === ">") && ahead === "=>") {
            report("arrow split in two");
          }
          if (text === "?" && ahead === "?:") {
            report("`?:` split in two");
          }
          if (text === "?:" && !scopes.includes("operator")) {
            report("`?:` lost its scope");
          }
          if (/^(true|false)$/.test(text) && !scopes.includes("boolean")) {
            report("boolean lost its scope");
          }
          if (
            ALWAYS_KEYWORD.test(text) &&
            !CONTEXTUAL.has(text) &&
            !/keyword|storage/.test(scopes)
          ) {
            report("keyword read as an identifier");
          }
        }
      });
    }
    assert.deepStrictEqual(problems, [], `\n${problems.join("\n")}\n`);
  });
});

// The corpus invariant sweep over EVERY .osp/.ospml in the repository. Unit
// fixtures only cover syntax someone thought to write down; the corpus is the
// syntax that actually exists. It caught the missing `effect`, `handle`,
// `perform` and `extern` keywords that no fixture would have, and the type
// names, generic arguments, `?`, `%` and brackets that rendered with no
// colour at all.
//
// Every invariant here is one a reader can state without knowing the grammar:
// an arrow is an arrow, a boolean is a boolean, a word that can only ever be a
// keyword is highlighted as one, a capitalised name is a type, and nothing
// renders in the editor's uncoloured default.

import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import {
  corpusFiles,
  grammarFor,
  grammarScopeOf,
  repoRoot,
  tokenize,
  Token,
} from "./grammar-harness";

// Contextual words: legal identifiers in the corpus today. `stage`, `out` and
// `extra` are record fields and GPU variables; `abort`, `alias`, `once`,
// `many` and `replayable` are operation names in tests/effects/multiplicity.
// Their keyword positions are covered by the contextual rules asserted in
// grammar.test.ts.
const CONTEXTUAL = new Set([
  "abort",
  "alias",
  "extra",
  "kernel",
  "many",
  "once",
  "out",
  "replayable",
  "stage",
  "static",
]);
const ALWAYS_KEYWORD =
  /^(fn|let|match|type|module|namespace|import|effect|handle|perform|signature|export|extern|resume|in|spawn|await|yield|send|recv|mut|handler|do|opaque|state)$/;
const PRIMITIVE_TYPE = /^(int|float|string|bool|any|Unit)$/;
// Multi-character operators a tokenizer must never split into their first
// character and a remainder.
const COMPOUND_OPERATORS = [
  "==",
  "!=",
  "<=",
  ">=",
  "=>",
  "->",
  "|>",
  "?:",
  "::",
  "&&",
  "||",
];
const MAX_PROBLEMS = 20;

/** Why `token` breaks an invariant, or undefined when it keeps them all. */
function violation(token: Token): string | undefined {
  const text = token.text.trim();
  const scopes = token.scopes.join("|");
  // ML permits an exported function named `string`; its declaration is not a
  // type position. Keep primitive checks for type positions and other uses.
  const mlExportDeclaration = scopes.includes("meta.export.binding.osprey-ml") &&
    scopes.includes("entity.name.function.osprey-ml");
  const checks: [boolean, string][] = [
    // A `meta.*` scope groups tokens but no theme colours it.
    [
      token.scopes.every((scope) => scope.startsWith("meta.")),
      "rendered with no colour",
    ],
    [/string|comment/.test(scopes), ""],
    // `|| => body` is an empty lambda parameter list, not a split `||`.
    [
      COMPOUND_OPERATORS.some(
        (op) => text === op[0] && token.rest.startsWith(op),
      ) && !scopes.includes("punctuation.definition.parameters"),
      "operator split in two",
    ],
    [text === "=>" && !scopes.includes("arrow"), "arrow lost its scope"],
    [text === "?:" && !scopes.includes("operator"), "`?:` lost its scope"],
    [
      /^(true|false)$/.test(text) && !scopes.includes("boolean"),
      "boolean lost its scope",
    ],
    // ML frees every reserved word in an operation-name position
    // [FLAVOR-ML-EFFECT-OP-NAME]; Default reserves them everywhere.
    [
      ALWAYS_KEYWORD.test(text) &&
        !CONTEXTUAL.has(text) &&
        !/keyword|storage/.test(scopes) &&
        !scopes.includes("entity.name.function.effect.osprey-ml"),
      "keyword read as an identifier",
    ],
    [
      PRIMITIVE_TYPE.test(text) && !scopes.includes("support.type.primitive") && !mlExportDeclaration,
      "primitive type read as a value",
    ],
    [
      /^[A-Z]\w*$/.test(text) && !/entity\.name|support\.type/.test(scopes),
      "type name read as a value",
    ],
  ];
  const broken = checks.find(([applies]) => applies);
  return broken?.[1] || undefined;
}

suite("TextMate grammar corpus", () => {
  test("no Osprey file in the repository tokenizes with a broken or uncoloured token", async () => {
    const files = corpusFiles();
    assert.ok(files.length > 100, `corpus is present (${files.length} files)`);
    const problems: string[] = [];
    for (const file of files) {
      const grammar = await grammarFor(grammarScopeOf(file));
      const relative = path.relative(repoRoot, file);
      for (const token of tokenize(grammar, fs.readFileSync(file, "utf8"))) {
        const why = violation(token);
        if (why && problems.length < MAX_PROBLEMS) {
          problems.push(
            `${relative}:${token.line} ${why}: ${JSON.stringify(token.text)} → ${token.scopes.join("|") || "NONE"}`,
          );
        }
      }
    }
    assert.deepStrictEqual(problems, [], `\n${problems.join("\n")}\n`);
  });
});

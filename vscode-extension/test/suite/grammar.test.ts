// TextMate grammar tests for both Osprey flavors: one named regression per
// defect found on real files, so a reintroduced bug names itself. Nothing
// executed the grammars until these tests existed, and a wrong scope still
// renders — just in the wrong colour — so only an assertion notices. The
// corpus-wide invariant sweep lives in grammar-corpus.test.ts.

import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { extensionRoot } from "./osprey-test-env";
import {
  assertEveryScoped,
  assertScoped,
  grammarFor,
  scopesOf,
  tokenize,
} from "./grammar-harness";

suite("TextMate grammars", () => {
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
        "    false => find doc id (index + 1) count",
      ].join("\n"),
    );
    assertScoped(tokens, "true", "constant.language.boolean");
    assertScoped(tokens, "false", "constant.language.boolean");
    assertScoped(tokens, "=>", "keyword.operator.arrow");
    assert.ok(
      !tokens.some(
        (t) =>
          t.text === "true" &&
          t.scopes.join("|").includes("entity.name.function"),
      ),
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
    // An exported function can be named `string`.
    const primitive = tokenize(grammar, "export string text = text");
    assertScoped(primitive, "string", "entity.name.function");
    assertScoped(primitive, "string", "meta.export.binding");
    assertScoped(primitive, "text", "variable.parameter");
    // Parameter and return type positions retain their primitive scope.
    assertEveryScoped(
      tokenize(grammar, "oobMessage : (string, int) -> string"),
      "string",
      "support.type.primitive",
    );
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
      [
        "type LocalNote =",
        "    note : string",
        "    priority : string",
        "",
        "module Annotations",
      ].join("\n"),
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

  // The effect's own name followed `effect` with no rule of its own, so
  // `Tensor` in `export static effect Tensor<B> {` fell through to the plain
  // identifier colour while every other declared type is a type. The same
  // name at the discharge (`handle static Tensor<GpuBuffer<float>>`) and in
  // a qualified `handle Backends::Tensor` reads as that type too.
  test("an effect's name is a type at its declaration and its discharge in both flavors", async () => {
    for (const [scope, declaration, discharge] of [
      [
        "source.osprey",
        "export static effect Tensor<B> {",
        "handle static Tensor<GpuBuffer<float>> {",
      ],
      [
        "source.osprey-ml",
        "export static effect Tensor B",
        "handle static Tensor<GpuBuffer<float>>",
      ],
    ] as const) {
      const grammar = await grammarFor(scope);
      const declared = tokenize(grammar, declaration);
      assertScoped(declared, "effect", "keyword.declaration.effect");
      assertScoped(declared, "static", "storage.modifier.static");
      assertScoped(declared, "Tensor", "entity.name.type.effect");
      const discharged = tokenize(grammar, discharge);
      assertScoped(discharged, "handle", "keyword.control");
      assertScoped(discharged, "Tensor", "entity.name.type.effect");
      assertScoped(discharged, "GpuBuffer", "entity.name.type");
      const qualified = tokenize(grammar, "handle Backends::Tensor");
      assertScoped(qualified, "Backends::Tensor", "entity.name.type.effect");
    }
  });

  // examples/gpt2/model/backends.osp verbatim. The `type` declaration rule
  // sat after `keywords`, whose `type` alternative matched at the same column
  // and won, so every alias name rendered uncoloured. The rule that coloured
  // capitalised names was never included anywhere, so `B`, `List` and
  // `Kernel` in a signature had no scope at all; primitives read as
  // variables; `->` split into a minus and a greater-than; and an operation
  // name read as a variable.
  test("Default type aliases, effect operations and their signatures are coloured", async () => {
    const grammar = await grammarFor("source.osprey");
    const tokens = tokenize(
      grammar,
      [
        "export type Kernel = fn(int) -> float",
        "export static effect Tensor<B> {",
        "    build: fn(int, Kernel) -> B",
        "    fromList: fn(List<float>) -> B",
        "}",
      ].join("\n"),
    );
    assertEveryScoped(tokens, "Kernel", "entity.name.type");
    assertEveryScoped(tokens, "B", "entity.name.type");
    assertEveryScoped(tokens, "List", "entity.name.type");
    assertEveryScoped(tokens, "int", "support.type.primitive");
    assertEveryScoped(tokens, "float", "support.type.primitive");
    assertEveryScoped(tokens, "->", "keyword.operator.arrow");
    assertEveryScoped(tokens, "<", "punctuation.definition.typeparameters");
    assertScoped(tokens, "build", "entity.name.function.effect");
    assertScoped(tokens, "fromList", "entity.name.function.effect");
  });

  test("Default generic declarations, record fields and handler arms name their parts", async () => {
    const grammar = await grammarFor("source.osprey");
    const tokens = tokenize(
      grammar,
      [
        "type Box<T> = { value: T }",
        "type Point = {",
        "    x: int,",
        "    y: int",
        "}",
        "fn groupAt<T>(xs: List<T>, at) -> T = xs",
        "handle static Tensor<List<float>> {",
        "    build n f => range(0, n) |> map(f)",
        '    device => "lists"',
        "}",
      ].join("\n"),
    );
    assertScoped(tokens, "Box", "entity.name.type");
    assertScoped(tokens, "Point", "entity.name.type");
    assertEveryScoped(tokens, "T", "entity.name.type");
    assertScoped(tokens, "value", "variable.other.member");
    assertScoped(tokens, "x", "variable.other.member");
    assertScoped(tokens, "groupAt", "entity.name.function");
    assertScoped(tokens, "xs", "variable.parameter");
    assertScoped(tokens, "at", "variable.parameter");
    assertScoped(tokens, "build", "entity.name.function.effect");
    assertScoped(tokens, "n f", "variable.parameter");
    assertScoped(tokens, "device", "entity.name.function.effect");
  });

  // `keywords` claimed `fn`, `namespace`, `module`, `signature` and `import`
  // at the same column as their declaration rules, which were therefore dead:
  // the declared name of every one rendered as a plain identifier.
  test("Default declarations name what follows their keyword", async () => {
    const grammar = await grammarFor("source.osprey");
    const tokens = tokenize(
      grammar,
      [
        "namespace gpt2;",
        "import gpt2::Tokenizer",
        "module Backends {",
        "    fn add(a, b) = a + b",
        "}",
        "signature Store {",
        "}",
      ].join("\n"),
    );
    assertScoped(tokens, "gpt2", "entity.name.namespace");
    assertScoped(tokens, "gpt2::Tokenizer", "entity.name.type.module");
    assertScoped(tokens, "Backends", "entity.name.type.module");
    assertScoped(tokens, "add", "entity.name.function");
    assertScoped(tokens, "a", "variable.parameter");
    assertScoped(tokens, "Store", "entity.name.type.interface");
  });

  // `strings` matched the same `"` as `interpolated-strings` and came first,
  // so `${…}` never highlighted; the integer rule preceded the float rule, so
  // `0.5` split in three; `=` preceded `==`; and `?`, `?:` and `%` had no rule.
  test("Default operators, floats and interpolations stay whole", async () => {
    const grammar = await grammarFor("source.osprey");
    const tokens = tokenize(
      grammar,
      'let ok = a == b && n % 2 != 0 ? 0.5 : total ?: 1\nprint("sum ${count + 1}")',
    );
    for (const [text, scope] of [
      ["==", "keyword.operator.comparison"],
      ["!=", "keyword.operator.comparison"],
      ["%", "keyword.operator.arithmetic"],
      ["?", "keyword.operator.ternary"],
      ["?:", "keyword.operator.error-propagation"],
      ["0.5", "constant.numeric.float"],
      ["count", "meta.embedded.expression"],
    ] as const) {
      assertScoped(tokens, text, scope);
    }
  });

  // The ML flavor coloured primitives only inside a `type` block and had no
  // rule for brackets or generic arguments, so a signature line rendered its
  // types as variables and `<` as a comparison.
  test("ML signatures colour primitives, generic arguments and brackets", async () => {
    const grammar = await grammarFor("source.osprey-ml");
    const tokens = tokenize(
      grammar,
      "oobMessage : (GpuBuffer<int>, int) -> string\nxs = [[1], []]",
    );
    assertEveryScoped(tokens, "int", "support.type.primitive");
    assertScoped(tokens, "string", "support.type.primitive");
    assertScoped(tokens, "GpuBuffer", "entity.name.type");
    assertScoped(tokens, "<", "punctuation.definition.typeparameters");
    assertScoped(tokens, "[", "punctuation");
  });
});

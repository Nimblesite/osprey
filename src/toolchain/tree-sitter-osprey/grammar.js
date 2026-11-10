/**
 * tree-sitter-osprey — the single canonical grammar for the Osprey language.
 *
 * This grammar replaced three earlier copies — the original parser grammar,
 * the VSCode TextMate grammar, and the website Monaco/Monarch grammar —
 * collapsed to one. It is the compiler
 * front-end's parser: src/toolchain/crates/osprey-syntax lowers this CST to the AST.
 *
 * Precedence climbs or < and < cmp < add < mul < pipe < unary < call via
 * tree-sitter prec.left/right with the PREC table below (higher = binds
 * tighter), preserving the associativity the language has always had.
 */

const { sep1 } = require('./grammar/common');
const declarations = require('./grammar/declarations');
const expressions = require('./grammar/expressions');
const patterns = require('./grammar/patterns');

module.exports = grammar({
  name: 'osprey',

  // Whitespace + comments are skipped between tokens. DOC_COMMENT (///) is NOT
  // extra — it is consumed by declarations.
  extras: ($) => [/\s/, $.line_comment, $._interpolation_space],

  word: ($) => $.identifier,

  // Two zero-width markers lexed by src/scanner.c, each answering a question
  // about the line a token sits on — the one thing the parser cannot see,
  // because whitespace is an extra.
  //
  //   _call_open_gap   holds only when the next `(` sits on the SAME LINE as
  //                    the callee — the one thing that tells a postfix call
  //                    from the following match arm's tuple pattern, and a
  //                    decision no precedence can encode [PATTERN-TUPLE].
  //   _statement_break holds only where the statement's line ends — the
  //                    statement terminator. Without it nothing delimited two
  //                    statements, so `let r = add 2 3` silently split into
  //                    `let r = add` plus the orphans `2` and `3`
  //                    [LEX-STATEMENT-BREAK].
  externals: ($) => [
    $._call_open_gap, $._statement_break, $._type_application_ahead,
    $._string_start, $._string_end, $._interpolation_start, $._interpolation_end,
    $.string_content, $.escape_sequence, $._interpolation_space,
  ],

  conflicts: ($) => [
    // `control` / `abort` / `once` / `many` / `replayable` opening an operation line are
    // either the modifier or the operation's own name; only the token after
    // them tells which. Implements [MULTI-DECL].
    [$.operation_declaration],
    // `ID { ... }` is ambiguous between an update/type-constructor expression and
    // an object/map literal until the brace body is seen; GLR resolves it.
    [$.update_expression, $.type_constructor],
    [$.primary_expression, $.pattern],
    [$.handler_value_expression, $.primary_expression],
    [$.call_expression, $.pattern],
    // `ID <` is ambiguous between a variable reference followed by `<` (comparison)
    // and the start of a generic type constructor `ID<T>{...}`; GLR resolves it.
    [$.primary_expression, $.type_constructor],
    [$.primary_expression, $.generic_type],
    [$.type_identifier, $.type_constructor],
    // `import a::b::{x}` must keep both parses alive until the token following
    // `::` distinguishes another target segment from an import tail.
    [$.import_target],
    // `ID {` — bare variable ref vs type-constructor vs record update.
    [$.primary_expression, $.type_constructor, $.update_expression],
    // `{` opens a block, a map literal, or an object literal; brace body decides.
    [$.block, $.map_literal],
    [$.block, $.object_literal],
    [$.map_literal, $.object_literal],
    [$.block, $.map_literal, $.object_literal],
    // `{ ID :` — object field assignment vs map entry with an identifier key.
    [$.primary_expression, $.field_assignment],
    [$.map_entry, $.field_assignment],
    // `{ expr }` — trailing block value vs a lone expression-statement in a block.
    [$.expression_statement, $.block],
    // `if cond { ID }` — the brace opens the `if` consequence (a bare identifier
    // expression), NOT a structural-ternary field-pattern on the condition; the
    // `}` with no trailing `?` rules the field-pattern out, GLR resolves it.
    [$.primary_expression, $.field_pattern],
    // `… => body` then `{ x, y }` — an arm body followed by a brace opens the
    // NEXT arm's structural pattern, not a structural ternary extending the
    // body; the token after the closing brace (`=>` vs `?`) decides, GLR
    // resolves it. The field_pattern self-conflict keeps the fork alive at
    // the `identifier ,` reduce, where `..` is not yet visible
    // [PATTERN-STRUCTURAL].
    [$.primary_expression, $.structural_pattern],
    [$.field_pattern],
    [$.ternary_expression, $.match_arm],
    [$.expression_statement, $.ternary_expression],
    [$.let_declaration, $.ternary_expression],
    [$.lambda_expression, $.ternary_expression],
    [$.handler_arm, $.ternary_expression],
    [$.function_declaration, $.ternary_expression],
    // `await ( x )` — the await_call form vs unary `await` over a parenthesized expr.
    [$.primary_expression, $.await_call],
    // `Name [` / `Name <` — array/generic type vs a bare type identifier.
    [$.array_type, $.type_identifier],
    [$.generic_type, $.type_identifier],
    // A single bare identifier remains an identifier; qualified paths require
    // at least one `::`, so these only overlap while the lexer has not yet seen
    // the separator.
    // `type Alias = Name` is intentionally kept as the historical one-variant
    // union spelling. Unambiguous function/generic/array aliases use type_alias;
    // opaque aliases are disambiguated by their module-item prefix.
  ],

  rules: {
    // ---------- TOP LEVEL ----------
    // A leading `//!` block documents the FILE, not the first declaration in
    // it, so it is consumed here rather than by `statement` [DOC-SIGIL-INNER].
    source_file: ($) => seq(optional($.inner_doc_comment), repeat($.statement)),

    // Every statement ends at its line's end: the trailing `_statement_break`
    // (zero-width, scanner.c) demands a newline, `//` comment, `}`, or EOF
    // before anything else follows. Two constructs on one line are therefore a
    // parse error, never a silent split [LEX-STATEMENT-BREAK].
    statement: ($) =>
      seq(
        choice(
          $.import_statement,
          $.namespace_declaration,
          $.let_declaration,
          $.assignment,
          $.function_declaration,
          $.extern_declaration,
          $.type_declaration,
          $.effect_declaration,
          $.module_declaration,
          $.signature_declaration,
          $.expression_statement,
        ),
        $._statement_break,
      ),

    import_statement: ($) =>
      seq(
        'import',
        choice(
          // Compatibility with the original Default spelling. It lowers to
          // the same namespace + SymbolPath model as `::` imports.
          field('legacy_target', $.legacy_import_path),
          seq(
            field('target', $.import_target),
            optional(field('tail', $.import_tail)),
          ),
        ),
      ),

    legacy_import_path: ($) =>
      seq($.identifier, repeat1(seq('.', $.identifier))),

    import_target: ($) =>
      seq(
        field('namespace', $.namespace_name),
        repeat(seq('::', field('segment', $.identifier))),
      ),

    import_tail: ($) =>
      choice(
        seq('as', field('alias', $.identifier)),
        seq('::', '{', optional($.import_member_list), '}'),
        seq('::', '*'),
      ),

    import_member_list: ($) => sep1(',', $.import_member),
    import_member: ($) =>
      seq(
        field('name', $.identifier),
        optional(seq('as', field('alias', $.identifier))),
      ),

    namespace_name: ($) => choice($.identifier, $.string),

    namespace_declaration: ($) =>
      seq(
        optional($.doc_comment),
        field('keyword', 'namespace'),
        field('name', $.namespace_name),
        choice(';', field('body', $.namespace_body)),
      ),

    namespace_body: ($) =>
      seq('{', optional($.inner_doc_comment), repeat($.statement), '}'),

    ...declarations,
    ...expressions,
    ...patterns,
  },
});

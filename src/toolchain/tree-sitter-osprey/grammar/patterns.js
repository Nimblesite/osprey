// Patterns, modules, signatures and terminals of the Osprey grammar, spread into grammar.js's rules in source order.
const { sep1 } = require('./common');

module.exports = {
  // ---------- PATTERNS ----------
  // The `pattern` forms: scalar literal/number (incl. negative), list
  // destructuring, constructor & destructuring forms, type-annotation,
  // structural, wildcard. Collection (list/map) literals are *not* pattern
  // literals — `[…]` is a list_pattern, so `[head, tail]` never collides with
  // a two-element list literal. Implements [TYPE-LIST-PATTERNS].
  pattern: ($) =>
    choice(
      $.integer, $.float, $.string, $.interpolated_string, $.boolean, // 0, 1.5, "x", true
      prec.right(seq(field('operator', choice('-', '+')), choice($.integer, $.float))), // -1, +42
      $.list_pattern, // [], [x], [a, b], [head, ...tail]
      seq(field('name', choice($.qualified_path, $.identifier)), '{', $.field_pattern, '}'), // Ok { value }
      seq(field('name', choice($.qualified_path, $.identifier)), '(', sep1(',', $.pattern), ')'), // Some(x)
      seq(field('name', $.identifier), ':', '{', $.field_pattern, '}'), // p: { x, y }
      seq(field('name', $.identifier), ':', field('type', $._type)), // value: Int
      seq(field('name', $.identifier), optional(field('binding', $.identifier))), // bare var / capture
      $.structural_pattern, // { name, age } / { name, .. }
      $.tuple_pattern, // (a, b) — positional row [PATTERN-TUPLE]
      '_',
    ),
  // `{ name, age }` closed row / `{ name, .. }` open row. The `..` marker
  // lives here, not in field_pattern, so the structural ternary and the
  // `Ctor { … }` forms — which share field_pattern — cannot spell an open
  // row [PATTERN-STRUCTURAL].
  structural_pattern: ($) => seq('{', $.field_pattern, optional(seq(',', '..')), '}'),
  // Two or more comma-separated slots; `(x)` stays expression grouping and
  // never reaches pattern position, so a one-element tuple is unspellable.
  // A slot is a binder or `_` — a nested pattern has no fall-through
  // spelling, exactly as in positional constructor slots; the ML parser
  // rejects the same shape ([FLAVOR-ML-PATTERN-GROUP], [PATTERN-TUPLE]).
  tuple_pattern: ($) =>
    seq(
      '(',
      field('element', choice($.identifier, '_')),
      repeat1(seq(',', field('element', choice($.identifier, '_')))),
      ')',
    ),
  // The rest binder `...id` is allowed only at the tail; a fixed-length form
  // (`[]`, `[a, b]`) omits it. Mid-list rest (`[a, ...m, b]`) is unrepresentable.
  list_pattern: ($) =>
    seq(
      '[',
      optional(
        seq(
          field('element', $.pattern),
          repeat(seq(',', field('element', $.pattern))),
          optional(seq(',', '...', field('rest', $.identifier))),
        ),
      ),
      ']',
    ),
  field_pattern: ($) => sep1(',', $.identifier),

  // ---------- MODULES ----------
  module_declaration: ($) =>
    seq(
      optional($.doc_comment),
      optional(field('state', 'state')),
      field('keyword', 'module'),
      field('path', $.symbol_path),
      optional(field('signature', $.signature_ascription)),
      '{',
      optional($.inner_doc_comment),
      repeat($.module_item),
      '}',
    ),

  signature_ascription: ($) =>
    seq(
      ':',
      field('path', $.symbol_path),
      optional(seq('+', field('extra', 'extra'))),
    ),

  module_item: ($) =>
    choice($.export_declaration, $._module_declaration),

  export_declaration: ($) =>
    seq(
      optional($.doc_comment),
      'export',
      choice(
        seq(field('opaque', 'opaque'), field('declaration', $.type_declaration)),
        field('declaration', $._module_declaration),
      ),
    ),

  _module_declaration: ($) =>
    choice(
      $.let_declaration,
      $.function_declaration,
      $.extern_declaration,
      $.type_declaration,
      $.effect_declaration,
      $.module_declaration,
      $.signature_declaration,
    ),

  // ---------- SIGNATURES ----------
  signature_declaration: ($) =>
    seq(
      optional($.doc_comment),
      field('keyword', 'signature'),
      field('name', $.identifier),
      '{',
      repeat($.signature_item),
      '}',
    ),

  signature_item: ($) =>
    choice(
      $.signature_value,
      $.signature_function,
      $.signature_type,
      $.effect_declaration,
      $.signature_module,
    ),

  signature_value: ($) =>
    seq('let', field('name', $.identifier), ':', field('type', $._type)),

  signature_function: ($) =>
    seq(
      'fn',
      field('name', $.identifier),
      optional(seq('<', field('type_parameters', $.type_parameter_list), '>')),
      '(',
      optional(field('parameters', $.extern_parameter_list)),
      ')',
      optional(seq('->', field('return_type', $._type))),
      optional(field('effects', $.effect_set)),
    ),

  signature_type: ($) =>
    seq(
      optional(field('opaque', 'opaque')),
      'type',
      field('name', $.identifier),
      optional(seq('<', field('type_parameters', $.type_parameter_list), '>')),
      optional(seq('=', field('definition', $._type))),
    ),

  signature_module: ($) =>
    seq(
      'module',
      field('path', $.symbol_path),
      field('signature', $.signature_ascription),
    ),

  // One-or-more `::` separators distinguishes an expression/type path from
  // an ordinary identifier without flavor-dependent semantic lookahead.
  qualified_path: ($) =>
    seq($.identifier, repeat1(seq('::', $.identifier))),
  symbol_path: ($) => sep1('::', $.identifier),

  // ---------- TERMINALS ----------
  boolean: ($) => choice('true', 'false'),
  float: ($) => /[0-9]+\.[0-9]+/,
  integer: ($) => /[0-9]+/,
  // Implements [STRING-INTERPOLATION]: embedded code uses ordinary expressions.
  string: ($) => seq(alias($._string_start, '"'), repeat($._string_text), alias($._string_end, '"')),
  interpolated_string: ($) =>
    seq(alias($._string_start, '"'), repeat($._string_text), $.interpolation,
      repeat(choice($._string_text, $.interpolation)), alias($._string_end, '"')),
  interpolation: ($) =>
    seq(alias($._interpolation_start, '${'), field('expression', $.expression), alias($._interpolation_end, '}')),
  _string_text: ($) => choice($.string_content, $.escape_sequence),

  identifier: ($) => /[a-zA-Z_][a-zA-Z0-9_]*/,

  doc_comment: ($) => repeat1($._doc_comment_line),
  // `///` doc comment must out-prioritise `//` line comment on the shared
  // prefix; both must out-prioritise `/` (division) by maximal munch.
  _doc_comment_line: ($) => token(prec(1, seq('///', /[^\r\n]*/))),
  // `//!` documents the scope that ENCLOSES it — the file, a namespace body
  // or a module body. It takes the same `prec(1)` trick as `///` so the
  // shared `//` prefix does not lex it as an ordinary line comment. Because
  // it is not an extra, a `//!` written anywhere else is a syntax error
  // rather than silent trivia [DOC-SIGIL-INNER].
  inner_doc_comment: ($) => repeat1($._inner_doc_comment_line),
  _inner_doc_comment_line: ($) => token(prec(1, seq('//!', /[^\r\n]*/))),
  // A line comment may not begin `//!`: that spelling belongs to
  // `_inner_doc_comment_line`, and leaving it matchable here gave the lexer a
  // silent fallback — a `//!` written where no scope can hold one lexed as an
  // ordinary comment and the documentation vanished without a word. Excluding
  // it makes that position a syntax error, exactly as a stray `///` already
  // is [DOC-SIGIL-INNER].
  line_comment: ($) =>
    token(seq('//', optional(seq(/[^!\r\n]/, /[^\r\n]*/)))),
};

/**
 * tree-sitter-osprey — the single canonical grammar for the Osprey language.
 *
 * This grammar replaced three earlier copies — the original parser grammar,
 * the VSCode TextMate grammar, and the website Monaco/Monarch grammar —
 * collapsed to one (docs/plans/go-to-rust-migration.md). It is the compiler
 * front-end's parser: crates/osprey-syntax lowers this CST to the AST.
 *
 * Precedence climbs or < and < cmp < add < mul < unary < pipe < call via
 * tree-sitter prec.left/right with the PREC table below (higher = binds
 * tighter), preserving the associativity the language has always had.
 */

const PREC = {
  ternary: 1,
  or: 2,
  and: 3,
  compare: 4,
  add: 5,
  mul: 6,
  unary: 7,
  pipe: 8,
  call: 9,
  member: 10,
};

module.exports = grammar({
  name: 'osprey',

  // Whitespace + comments are skipped between tokens. DOC_COMMENT (///) is NOT
  // extra — it is consumed by declarations.
  extras: ($) => [/\s/, $.line_comment],

  word: ($) => $.identifier,

  conflicts: ($) => [
    // `ID { ... }` is ambiguous between an update/type-constructor expression and
    // an object/map literal until the brace body is seen; GLR resolves it.
    [$.update_expression, $.type_constructor],
    [$.primary_expression, $.pattern],
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
    source_file: ($) => repeat($.statement),

    statement: ($) =>
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

    namespace_body: ($) => seq('{', repeat($.statement), '}'),

    // ---------- DECLARATIONS ----------
    let_declaration: ($) =>
      seq(
        optional($.doc_comment),
        field('keyword', choice('let', 'mut')),
        field('name', $.identifier),
        optional(seq(':', field('type', $._type))),
        '=',
        field('value', $.expression),
      ),

    assignment: ($) =>
      prec(-1, seq(field('name', $.identifier), '=', field('value', $.expression))),

    function_declaration: ($) =>
      seq(
        optional($.doc_comment),
        'fn',
        field('name', $.identifier),
        // `fn map<T, U>(...)` — declared type parameters. Implements
        // [TYPE-GENERICS-FN].
        optional(seq('<', field('type_parameters', $.type_parameter_list), '>')),
        '(',
        optional(field('parameters', $.parameter_list)),
        ')',
        optional(seq('->', field('return_type', $._type))),
        optional(field('effects', $.effect_set)),
        choice(
          seq('=', field('body', $.expression)),
          field('body', $.block),
        ),
      ),

    extern_declaration: ($) =>
      seq(
        optional($.doc_comment),
        'extern',
        'fn',
        field('name', $.identifier),
        '(',
        optional(field('parameters', $.extern_parameter_list)),
        ')',
        optional(seq('->', field('return_type', $._type))),
      ),

    extern_parameter_list: ($) => sep1(',', $.extern_parameter),
    extern_parameter: ($) => seq(field('name', $.identifier), ':', field('type', $._type)),

    parameter_list: ($) => sep1(',', $.parameter),
    // `_` declares an argument the body ignores. Shared core, not ML sugar:
    // without it ML's `\(acc, _) => …` has no `|acc, _| => …` twin and the pair
    // breaks [FLAVOR-IR-EQUIV]. Implements [PARAM-WILDCARD].
    parameter: ($) =>
      seq(
        field('name', choice($.identifier, '_')),
        optional(seq(':', field('type', $._type))),
      ),

    type_declaration: ($) =>
      seq(
        optional($.doc_comment),
        'type',
        field('name', $.identifier),
        optional(seq('<', field('type_parameters', $.type_parameter_list), '>')),
        '=',
        field('definition', choice($.record_type, $.union_type, $.type_alias)),
        optional($.type_validation),
      ),

    // A type parameter optionally carries declaration-site variance:
    // `out T` (covariant) / `in T` (contravariant). `out`/`in` are contextual
    // keywords reserved only inside `<...>` parameter lists. Implements
    // [TYPE-VARIANCE-DECL].
    type_parameter_list: ($) => sep1(',', $.type_parameter),
    type_parameter: ($) =>
      seq(optional(field('variance', choice('in', 'out'))), field('name', $.identifier)),

    union_type: ($) => prec.right(sep1('|', $.variant)),
    type_alias: ($) => prec.dynamic(-1, $._type),
    // A variant's payload is named (`Node { l: Tree }`) or POSITIONAL
    // (`Node(Tree, Tree)`), the latter having no field names to supply and so
    // resolved by slot. Implements [TYPE-UNION-POSITIONAL].
    variant: ($) =>
      prec.right(
        1,
        seq(
          field('name', choice($.qualified_path, $.identifier)),
          optional(
            choice(
              seq('{', $.field_declarations, '}'),
              seq('(', field('positional', $.positional_payload), ')'),
            ),
          ),
        ),
      ),

    positional_payload: ($) => sep1(',', $._type),

    record_type: ($) => seq('{', $.field_declarations, '}'),

    field_declarations: ($) => sep1(',', $.field_declaration),
    field_declaration: ($) =>
      seq(
        field('name', $.identifier),
        ':',
        field('type', $._type),
        optional(seq('where', $.call_expression)),
      ),

    type_validation: ($) => seq('where', $.identifier),

    // ---------- EFFECTS ----------
    // `effect State<T> { ... }` — effects accept type parameters (with
    // variance) for full polymorphism. Implements [EFFECTS-GENERIC-DECL].
    effect_declaration: ($) =>
      seq(
        optional($.doc_comment),
        'effect',
        field('name', $.identifier),
        optional(seq('<', field('type_parameters', $.type_parameter_list), '>')),
        '{',
        repeat($.operation_declaration),
        '}',
      ),
    operation_declaration: ($) =>
      seq(field('name', $.identifier), ':', field('type', $._type)),

    // Effect rows reference effects optionally applied to type arguments:
    // `!State<int>`, `![State<int>, Log]`. Implements [EFFECTS-GENERIC-ROWS].
    effect_set: ($) =>
      choice(
        seq('!', $.effect_ref),
        seq('!', '[', $.effect_list, ']'),
      ),
    effect_list: ($) => sep1(',', $.effect_ref),
    effect_ref: ($) => seq(field('name', choice($.qualified_path, $.identifier)), optional($.type_arguments)),

    // ---------- TYPES ----------
    _type: ($) =>
      choice(
        $.function_type,
        $.generic_type,
        $.array_type,
        $.type_identifier,
      ),

    function_type: ($) =>
      choice(
        seq('(', optional($.type_list), ')', '->', $._type),
        seq('fn', '(', optional($.type_list), ')', '->', $._type),
      ),
    generic_type: ($) =>
      seq(field('name', choice($.qualified_path, $.identifier)), '<', $.type_list, '>'),
    array_type: ($) => seq(field('name', choice($.qualified_path, $.identifier)), '[', $._type, ']'),
    type_identifier: ($) => choice($.qualified_path, $.identifier),
    type_list: ($) => sep1(',', $._type),

    // ---------- EXPRESSIONS ----------
    expression_statement: ($) => $.expression,

    expression: ($) =>
      choice(
        $.match_expression,
        $.if_expression,
        $.handler_expression,
        $.select_expression,
        $.ternary_expression,
        $.binary_expression,
        $.unary_expression,
        $.pipe_expression,
        $.call_expression,
        $.primary_expression,
      ),

    match_expression: ($) =>
      prec.dynamic(2, seq('match', field('value', $.expression), '{', repeat($.match_arm), '}')),

    match_arm: ($) => seq(field('pattern', $.pattern), '=>', field('body', $.expression)),

    // Populist Default-flavor conditional [GRAMMAR-IF-ELSE]
    // (Kotlin/Swift/Rust shape). Osprey is
    // expression-oriented, so `if` yields a value and the `else` branch is
    // required; each branch is a single expression wrapped in braces. `else if`
    // chains nest into `alternative`. Lowers to the same boolean `match` the
    // ternary desugars to — no new AST node, no type/codegen changes
    // ([FLAVOR-BOUNDARY]). The ML flavor keeps its layout `match`; this
    // spelling is Default-only. The braces here are literal delimiters of the
    // `if`, distinct from a `block` expression, which sidesteps the structural
    // ternary's `cond { field } ? …` form.
    if_expression: ($) =>
      prec.right(
        PREC.ternary,
        seq(
          'if',
          field('condition', $.expression),
          '{',
          field('consequence', $.expression),
          '}',
          'else',
          choice(
            seq('{', field('alternative', $.expression), '}'),
            field('alternative', $.if_expression),
          ),
        ),
      ),

    handler_expression: ($) =>
      prec.right(seq('handle', field('effect', choice($.qualified_path, $.identifier)), repeat1($.handler_arm), 'in', field('body', $.expression))),
    handler_arm: ($) =>
      seq(field('operation', $.identifier), optional($.handler_params), '=>', field('body', $.expression)),
    handler_params: ($) => repeat1($.identifier),

    select_expression: ($) => seq('select', '{', repeat1($.select_arm), '}'),
    select_arm: ($) =>
      seq(field('pattern', $.pattern), '=>', field('body', $.expression)),

    ternary_expression: ($) =>
      prec.right(
        PREC.ternary,
        choice(
          seq(field('condition', $.expression), '{', $.field_pattern, '}', '?', $.expression, ':', $.expression),
          seq(field('condition', $.expression), '?', field('then', $.expression), ':', field('else', $.expression)),
          seq(field('condition', $.expression), '?', ':', field('else', $.expression)),
        ),
      ),

    binary_expression: ($) => {
      const table = [
        ['||', PREC.or],
        ['&&', PREC.and],
        ['==', PREC.compare],
        ['!=', PREC.compare],
        ['<', PREC.compare],
        ['>', PREC.compare],
        ['<=', PREC.compare],
        ['>=', PREC.compare],
        ['+', PREC.add],
        ['-', PREC.add],
        ['*', PREC.mul],
        ['/', PREC.mul],
        ['%', PREC.mul],
      ];
      return choice(
        ...table.map(([op, p]) =>
          prec.left(p, seq(field('left', $.expression), field('operator', op), field('right', $.expression))),
        ),
      );
    },

    unary_expression: ($) =>
      prec.right(PREC.unary, seq(field('operator', choice('+', '-', '!', 'await')), field('operand', $.expression))),

    pipe_expression: ($) =>
      prec.left(PREC.pipe, seq(field('left', $.expression), '|>', field('right', $.expression))),

    // Left-recursive postfix chain: field access (obj.f), method/function call
    // (f(args)), and index (a[i]) — composes to obj.f.m()[0] naturally.
    call_expression: ($) =>
      prec.left(
        PREC.member,
        seq(
          field('callee', $.expression),
          choice(
            seq('.', field('member', $.identifier)),
            seq('(', optional($.argument_list), ')'),
            // The index `[` must immediately follow the callee (no whitespace),
            // so a match-arm body never swallows the next arm's `[…]` list
            // pattern as an index (`=> 0  [head, ...t] => …`). Implements
            // [TYPE-LIST-PATTERNS] coexistence with postfix indexing.
            seq(token.immediate('['), field('index', $.expression), ']'),
          ),
        ),
      ),

    argument_list: ($) =>
      choice(
        $.named_argument_list,
        sep1(',', $.expression),
      ),
    named_argument_list: ($) => sep1(',', $.named_argument),
    named_argument: ($) => seq(field('name', $.identifier), ':', field('value', $.expression)),

    primary_expression: ($) =>
      choice(
        $.spawn_expression,
        $.yield_expression,
        $.await_call,
        $.send_call,
        $.recv_call,
        $.perform_expression,
        $.resume_expression,
        $.type_constructor,
        $.update_expression,
        $.block,
        $.object_literal,
        $.literal,
        $.lambda_expression,
        $.qualified_path,
        $.identifier,
        seq('(', $.expression, ')'),
      ),

    spawn_expression: ($) => prec.right(seq('spawn', $.expression)),
    yield_expression: ($) => prec.right(seq('yield', optional($.expression))),
    await_call: ($) => seq('await', '(', $.expression, ')'),
    send_call: ($) => seq('send', '(', $.expression, ',', $.expression, ')'),
    recv_call: ($) => seq('recv', '(', $.expression, ')'),
    perform_expression: ($) =>
      seq('perform', field('effect', choice($.qualified_path, $.identifier)), '.', field('operation', $.identifier), '(', optional($.argument_list), ')'),
    // `resume(v)` resumes the performer's delimited continuation with `v`;
    // `resume()` resumes with Unit. Only legal inside a handler arm body.
    // Implements [EFFECTS-RESUME].
    resume_expression: ($) =>
      seq('resume', '(', field('value', optional($.expression)), ')'),

    type_constructor: ($) =>
      prec.dynamic(1, seq(field('name', choice($.qualified_path, $.identifier)), optional($.type_arguments), '{', $.field_assignments, '}')),
    type_arguments: ($) => seq('<', $.type_list, '>'),

    update_expression: ($) =>
      prec.dynamic(0, seq(field('record', $.identifier), '{', $.field_assignments, '}')),

    field_assignments: ($) => sep1(',', $.field_assignment),
    field_assignment: ($) => seq(field('name', $.identifier), ':', field('value', $.expression)),

    block: ($) => seq('{', repeat($.statement), optional($.expression), '}'),

    object_literal: ($) => prec(-1, seq('{', $.field_assignments, '}')),

    lambda_expression: ($) =>
      choice(
        seq('fn', '(', optional($.parameter_list), ')', optional(seq('->', $._type)), '=>', field('body', $.expression)),
        seq('|', optional($.parameter_list), '|', '=>', field('body', $.expression)),
      ),

    // ---------- LITERALS ----------
    literal: ($) =>
      choice($.float, $.integer, $.string, $.interpolated_string, $.boolean, $.list_literal, $.map_literal),

    list_literal: ($) => seq('[', optional(sep1(',', $.expression)), ']'),
    map_literal: ($) =>
      choice(seq('{', sep1(',', $.map_entry), '}'), seq('{', '}')),
    map_entry: ($) => seq(field('key', $.expression), ':', field('value', $.expression)),

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
        seq('{', $.field_pattern, '}'), // { name, age }
        '_',
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
    string: ($) => /"(\\.|[^"\\])*"/,
    // Interpolated string: contains at least one ${...}. Kept as a single token
    // (the AST builder splits it).
    interpolated_string: ($) =>
      token(
        seq(
          '"',
          repeat(choice(/[^"\\$]/, /\\./, /\$[^{]/)),
          repeat1(seq('${', /[^}]*/, '}', repeat(choice(/[^"\\$]/, /\\./, /\$[^{]/)))),
          '"',
        ),
      ),

    identifier: ($) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    doc_comment: ($) => repeat1($._doc_comment_line),
    // `///` doc comment must out-prioritise `//` line comment on the shared
    // prefix; both must out-prioritise `/` (division) by maximal munch.
    _doc_comment_line: ($) => token(prec(1, seq('///', /[^\r\n]*/))),
    line_comment: ($) => token(seq('//', /[^\r\n]*/)),
  },
});

function sep1(separator, rule) {
  return seq(rule, repeat(seq(separator, rule)));
}

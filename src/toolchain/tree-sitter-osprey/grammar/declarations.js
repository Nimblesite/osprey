// Declarations, effects and types of the Osprey grammar, spread into grammar.js's rules in source order.
const { sep1 } = require('./common');

module.exports = {
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
      optional(field('effects', $.closed_effect_set)),
      optional(seq('where', $.call_expression)),
    ),

  type_validation: ($) => seq('where', $.identifier),

  // ---------- EFFECTS ----------
  // `effect State<T> { ... }` — effects accept type parameters (with
  // variance) for full polymorphism. Implements [EFFECTS-GENERIC-DECL].
  // A `static effect` is a compile-time dialect: its operations are rewritten
  // away and never reach the runtime. Implements [STAGE-DECL].
  static_stage: ($) => 'static',
  effect_declaration: ($) =>
    seq(
      optional($.doc_comment),
      optional(field('stage', $.static_stage)),
      'effect',
      field('name', $.identifier),
      optional(seq('<', field('type_parameters', $.type_parameter_list), '>')),
      '{',
      repeat($.operation_declaration),
      '}',
    ),
  // `control` declares that answering this operation takes the performer's
  // continuation: the arm returns the HANDLER's answer and may `resume`. An
  // undecorated operation is a value operation whose arm returns the
  // operation's result. Contextual, like the modifiers below.
  // Implements [EFFECTS-HANDLER-ARMS].
  operation_mode: ($) => 'control',
  // How many times an operation's request may be answered — `abort` (never),
  // `once` (the default, at most one) or `many` (any number). A bare keyword
  // node in `static_stage`'s shape, not a general modifier list. Both this
  // and `replayable` are CONTEXTUAL: `word: $.identifier` above means the
  // lexer only reads them as keywords where the parser accepts one, so
  // `abort: fn() -> Unit` still declares an operation called `abort`.
  // Implements [MULTI-DECL].
  multiplicity: ($) => choice('abort', 'once', 'many'),
  // Asserts that performing the operation twice with the same arguments in
  // the same handler context is acceptable to the program — the property a
  // multi-shot handler's replay check reads. Implements [MULTI-REPLAY].
  replayable: ($) => 'replayable',
  // Each operation carries its OWN `///` docs — an effect's operations are
  // independent entry points, so `Prompt.ask` and `Prompt.tell` must hover
  // with their own prose, not the effect's. Implements [DOC-EFFECT-OP].
  operation_declaration: ($) =>
    seq(
      optional($.doc_comment),
      optional(seq(
        field('mode', $.operation_mode),
        optional(field('multiplicity', $.multiplicity)),
      )),
      optional(field('replayable', $.replayable)),
      // The modifier words are also legal operation NAMES. The lexer emits
      // the keyword token wherever a modifier is acceptable, so recovering
      // `abort: fn() -> Unit` cannot be a lexical decision — it is a parse
      // one, and the alias lets GLR carry both readings until `:` (a name)
      // or an identifier (a modifier) settles it. Implements [MULTI-DECL].
      field(
        'name',
        choice(
          $.identifier,
          alias($.operation_mode, $.identifier),
          alias($.multiplicity, $.identifier),
          alias($.replayable, $.identifier),
        ),
      ),
      ':',
      field('type', $._type),
    ),

  // Effect rows reference effects optionally applied to type arguments:
  // `!State<int>`, `![State<int>, Log]`. Implements [EFFECTS-GENERIC-ROWS].
  effect_set: ($) =>
    choice(
      seq('!', $.effect_ref),
      seq('!', '[', optional($.effect_list), optional(seq('|', field('tail', $.identifier))), ']'),
    ),
  // A function-typed field's row names exactly what a call through the
  // field may perform, so it has no open tail. Implements [EFFECTS-FIELD-ROWS].
  closed_effect_set: ($) =>
    choice(seq('!', $.effect_ref), seq('!', '[', optional($.effect_list), ']')),
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
};

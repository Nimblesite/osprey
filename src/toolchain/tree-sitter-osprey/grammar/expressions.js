// Expressions and literals of the Osprey grammar, spread into grammar.js's rules in source order.
const { PREC, sep1 } = require('./common');

module.exports = {
  // ---------- EXPRESSIONS ----------
  // A leading `///` block documents the statement it precedes — the only way
  // to document a `test("name", …)` case, which is a call expression rather
  // than a declaration. Implements [DOC-ATTACH], [TESTING-DOC].
  expression_statement: ($) => seq(optional($.doc_comment), $.expression),

  expression: ($) =>
    choice(
      $.match_expression,
      $.if_expression,
      $.handler_expression,
      $.handler_value_expression,
      $.kernel_expression,
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

  // A block-scoped handler governs the following statements. Static selection
  // discharges the same region during compilation. Explicit computations use
  // callable handler values. Implements [EFFECTS-HANDLE-REST], [STAGE-HANDLE-STATIC].
  handler_expression: ($) =>
    seq('handle', optional(field('stage', $.static_stage)), field('effect', choice($.qualified_path, $.identifier)), optional(field('instantiation', $.type_arguments)), '{', $._handler_clauses, '}'),
  // `handler E { arm… }` is the handler ITSELF, with no region attached: a
  // value that can be bound, passed and called. Calling it with a
  // zero-argument computation runs that computation under these arms, so one
  // handler serves many regions. Implements [EFFECTS-HANDLER-VALUE].
  handler_value_expression: ($) =>
    prec.right(seq('handler', optional(field('stage', $.static_stage)), field('effect', choice($.qualified_path, $.identifier)), optional(field('instantiation', $.type_arguments)), '{', $._handler_clauses, '}')),
  // Arms plus at most one `return` clause and at most one `finally` clause,
  // in any order. Implements [EFFECTS-HANDLER-ARMS], [EFFECTS-FINALIZATION].
  _handler_clauses: ($) =>
    choice(
      repeat1($.handler_arm),
      seq(repeat($.handler_arm), $.handler_return, repeat($.handler_arm), optional(seq($.handler_finally, repeat($.handler_arm)))),
      seq(repeat($.handler_arm), $.handler_finally, repeat($.handler_arm), optional(seq($.handler_return, repeat($.handler_arm)))),
    ),
  handler_return: ($) => seq('return', field('parameter', $.identifier), '=>', field('body', $.expression)),
  // `finally => cleanup` runs once when the activation ends.
  handler_finally: ($) => seq('finally', '=>', field('body', $.expression)),
  handler_arm: ($) =>
    seq(field('operation', $.identifier), optional($.handler_params), '=>', field('body', $.expression)),
  handler_params: ($) => repeat1($.identifier),

  // `kernel E op ... in body` is not a magic block: it is a handler region
  // that supplies the STATIC handlers for the device dialects its body may
  // use, and admits only bodies whose residual dynamic row is empty. Each arm
  // names its effect because one kernel answers several dialects — `Parallel`,
  // `Alloc`, `Tensor` — where `handle` answers exactly one.
  // Implements [STAGE-GPU-KERNEL], [STAGE-GPU-LEGAL].
  kernel_expression: ($) =>
    prec.right(seq('kernel', repeat1($.kernel_arm), 'in', field('body', $.expression))),
  kernel_arm: ($) =>
    seq(field('effect', $.identifier), field('operation', $.identifier), optional($.handler_params), '=>', field('body', $.expression)),

  select_expression: ($) => seq('select', '{', repeat1($.select_arm), '}'),
  select_arm: ($) =>
    choice(
      seq(field('binding', choice($.identifier, '_')), '=', field('operation', $.recv_call), '=>', field('body', $.expression)),
      seq(field('operation', choice($.recv_call, $.send_call, $.select_timeout)), '=>', field('body', $.expression)),
      seq('_', '=>', field('body', $.expression)),
    ),
  select_timeout: ($) => seq('timeout', '(', $.expression, ')'),

  ternary_expression: ($) =>
    choice(
      // The structural form carries NO static precedence: after a match-arm
      // body, `{ f }` is equally the next arm's structural pattern, and a
      // static winner would make one of the two spellings unparseable. The
      // declared conflict with match_arm forks instead, and the token after
      // `}` (`?` vs `=>`) settles it [PATTERN-STRUCTURAL].
      prec.right(seq(field('condition', $.expression), '{', $.field_pattern, '}', '?', $.expression, ':', $.expression)),
      prec.right(
        PREC.ternary,
        seq(field('condition', $.expression), '?', field('then', $.expression), ':', field('else', $.expression)),
      ),
      prec.right(
        PREC.ternary,
        // `?:` is one token ([LEX-RESULT-DEFAULT-TOKEN]).
        seq(field('condition', $.expression), '?:', field('else', $.expression)),
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
          // `f(args)` and `f (args)` both — the argument `(` may be preceded
          // by horizontal space but must stay on the callee's line, which is
          // what `_call_open_gap` (zero-width, src/scanner.c) asserts. That
          // keeps the call binding at PREC.member, so `1 + id (2)` is still
          // `1 + id(2)`, while a `(` opening the next line belongs to that
          // arm's `(a, b)` tuple pattern instead. Implements [PATTERN-TUPLE]
          // coexistence with postfix calls.
          seq($._call_open_gap, '(', optional($.argument_list), ')'),
          // [TYPE-GENERICS-APPLY]: both delimiters abut the call. Alias the
          // immediate opening form to the shared construction-site CST.
          seq(field('type_arguments', alias($._call_type_arguments, $.type_arguments)),
            token.immediate('('), optional($.argument_list), ')'),
          // The index `[` must IMMEDIATELY follow its target — a stricter rule
          // than the call's same-line one, and deliberately so. List-pattern
          // arms are written on ONE line in real source:
          //
          //     match xs { [] => 0  [head, ...tail] => add(head, sumL(tail)) }
          //
          // so a same-line `[` gap would read `0  [head` as an index and
          // break src/toolchain/benchmarks/cases/listops/listops.osp. Tuple-pattern arms
          // have no such single-line usage, which is why `(` can afford the
          // looser rule and `[` cannot. Implements [TYPE-LIST-PATTERNS]
          // coexistence with postfix indexing.
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
      $.mask_expression,
      $.instance_expression,
      $.resume_expression,
      $.type_constructor,
      $.update_expression,
      $.block,
      $.object_literal,
      $.literal,
      $.lambda_expression,
      $.qualified_path,
      $.identifier,
      // A handler value needs an effect name and arms. Existing callbacks
      // named `handler` remain ordinary references/calls in other contexts.
      alias('handler', $.identifier),
      seq('(', $.expression, ')'),
    ),

  spawn_expression: ($) => prec.right(seq('spawn', $.expression)),
  yield_expression: ($) => prec.right(seq('yield', optional($.expression))),
  await_call: ($) => seq('await', '(', $.expression, ')'),
  send_call: ($) => seq('send', '(', $.expression, ',', $.expression, ')'),
  recv_call: ($) => seq('recv', '(', $.expression, ')'),
  // `mask E { body }` skips one enclosing activation of E (or of a named
  // capability) while `body` runs. Implements [EFFECTS-MASK].
  mask_expression: ($) =>
    seq('mask', field('effect', choice($.qualified_path, $.identifier)), optional(field('instantiation', $.type_arguments)), field('body', $.block)),
  // `instance E` creates a fresh capability identity. Implements [EFFECTS-INSTANCE].
  instance_expression: ($) =>
    prec.right(seq('instance', field('effect', choice($.qualified_path, $.identifier)), optional(field('instantiation', $.type_arguments)))),
  perform_expression: ($) =>
    seq('perform', field('effect', choice($.qualified_path, $.identifier)), optional(field('instantiation', $.type_arguments)), '.', field('operation', $.identifier), '(', optional($.argument_list), ')'),
  // `resume(v)` resumes the performer's delimited continuation with `v`;
  // `resume()` resumes with Unit. Only legal inside a handler arm body.
  // Implements [EFFECTS-RESUME].
  resume_expression: ($) =>
    seq('resume', '(', field('value', optional($.expression)), ')'),

  type_constructor: ($) =>
    prec.dynamic(1, seq(field('name', choice($.qualified_path, $.identifier)), optional($.type_arguments), '{', $.field_assignments, '}')),
  type_arguments: ($) => seq('<', $.type_list, '>'),
  // Implements [SYNTAX-TYPE-ARGUMENTS]: `<` touches the callee.
  _call_type_arguments: ($) => seq($._type_application_ahead, token.immediate('<'), alias($._call_type_list, $.type_list), '>'),
  // Keep misplaced declaration markers visible for a precise diagnostic.
  _call_type_list: ($) => sep1(',', seq(optional(field('variance', choice('in', 'out'))), $._type)),

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
};

//! ML expressions lowering.
use super::{
    curry_position, flat_params, in_scope, is_bound, lower_interpolation, lower_items,
    lower_lambda, params_scope, positional_construction, render_type, scope_of, type_expr, unquote,
    Expr, FieldAssignment, HandlerArm, MapEntry, MatchArm, MlArm, MlExpr, MlField, MlHandleArm,
    MlItem, MlParam, MlPattern, MlType, Pattern, Position, SymbolPath, TypeExpr,
};

/// Lower one CST expression to a canonical [`Expr`].
pub(in crate::ml) fn lower_expr(expr: MlExpr) -> Expr {
    match expr {
        MlExpr::Int(n) => Expr::Integer(n),
        MlExpr::Float(f) => Expr::Float(f),
        MlExpr::Bool(b) => Expr::Bool(b),
        MlExpr::Str { raw, pos } => lower_string(&raw, Some(pos)),
        MlExpr::Ident(name) => Expr::Identifier(name),
        MlExpr::Path(path) => Expr::Path(SymbolPath {
            segments: path.segments,
        }),
        MlExpr::Paren(inner) => lower_expr(*inner),
        MlExpr::TypeApply { func, args, pos } => Expr::TypeApply {
            function: Box::new(lower_expr(*func)),
            type_args: args.iter().filter_map(type_expr).collect(),
            position: Some(pos),
        },
        // `-literal` folds to the literal so both flavors agree that `-1` is an
        // `int`, not a fallible `Result` ([ARITH-NEG-LITERAL], [FLAVOR-IR-EQUIV]).
        MlExpr::Unary { op, operand } if op == "-" => Expr::negated(lower_expr(*operand)),
        MlExpr::Unary { op, operand } => Expr::Unary {
            op,
            operand: Box::new(lower_expr(*operand)),
        },
        MlExpr::Binary {
            op,
            left,
            right,
            pos,
        } => lower_binary(&op, *left, *right, pos),
        MlExpr::App { func, arg } => lower_application(*func, *arg),
        // `func (a, b, …)` — the uncurried saturated call lowers to one flat
        // multi-argument `Call`, byte-identical to the Default `func(a, b, …)`
        // ([FLAVOR-ML-CALL]).
        MlExpr::AppMulti { func, args } => lower_multi_application(*func, args),
        MlExpr::UnitApp { func } => call(lower_expr(*func), Vec::new()),
        MlExpr::List(items, pos) => {
            Expr::List(items.into_iter().map(lower_expr).collect(), Some(pos))
        }
        MlExpr::Map(entries) => Expr::Map(entries.into_iter().map(lower_map_entry).collect()),
        MlExpr::Index { target, index } => Expr::Index {
            target: Box::new(lower_expr(*target)),
            index: Box::new(lower_expr(*index)),
        },
        MlExpr::Field { target, name } => Expr::FieldAccess {
            target: Box::new(lower_expr(*target)),
            field: name,
        },
        // A multi-parameter lambda `\x y => body` curries into nested
        // one-parameter lambdas ([FLAVOR-ML-CURRY]); an empty/unit head stays a
        // single zero-parameter lambda.
        // `\(x, y) => body` (uncurried) is one flat multi-parameter lambda;
        // `\x y => body` (curried) nests one-parameter lambdas ([FLAVOR-ML-CURRY]).
        MlExpr::Lambda {
            params,
            uncurried,
            body,
            pos,
        } => lower_lambda_node(params, uncurried, *body, pos),
        MlExpr::Match { scrutinee, arms } => Expr::Match {
            value: Box::new(lower_expr(*scrutinee)),
            arms: arms.into_iter().map(lower_arm).collect(),
        },
        MlExpr::Object(fields) => Expr::Object(fields.into_iter().map(lower_field).collect()),
        MlExpr::Record {
            name,
            type_args,
            fields,
        } => lower_record(name, &type_args, fields),
        MlExpr::Block { items, value, pos } => lower_block(items, value, pos),
        MlExpr::Spawn(body) => Expr::Spawn(Box::new(lower_expr(*body))),
        MlExpr::Perform {
            effect,
            operation,
            args,
            pos,
        } => Expr::Perform {
            effect,
            operation,
            arguments: args.into_iter().map(lower_expr).collect(),
            named_arguments: Vec::new(),
            position: Some(pos),
        },
        MlExpr::HandlerValue {
            stage,
            effect,
            arms,
            return_clause,
            pos,
        } => lower_handler(stage, effect, arms, return_clause, None, pos),
        MlExpr::Handle {
            stage,
            effect,
            arms,
            return_clause,
            body,
            pos,
        } => lower_handler(stage, effect, arms, return_clause, Some(*body), pos),
        MlExpr::Resume(value) => Expr::Resume(value.map(|e| Box::new(lower_expr(*e)))),
        MlExpr::Await(inner) => Expr::Await(Box::new(lower_expr(*inner))),
        MlExpr::Yield(value) => Expr::Yield(value.map(|e| Box::new(lower_expr(*e)))),
        MlExpr::Send { channel, value } => Expr::Send {
            channel: Box::new(lower_expr(*channel)),
            value: Box::new(lower_expr(*value)),
        },
        MlExpr::Recv(inner) => Expr::Recv(Box::new(lower_expr(*inner))),
        MlExpr::Select(arms) => Expr::Select {
            arms: arms.into_iter().map(lower_arm).collect(),
        },
    }
}

pub(in crate::ml) fn lower_handler(
    stage: osprey_ast::Stage,
    effect: String,
    arms: Vec<MlHandleArm>,
    return_clause: Option<Box<MlExpr>>,
    body: Option<MlExpr>,
    pos: Position,
) -> Expr {
    let arms = arms.into_iter().map(lower_handle_arm).collect();
    let return_clause = return_clause.map(|clause| Box::new(lower_expr(*clause)));
    match body {
        Some(body) => Expr::Handler {
            stage,
            effect,
            arms,
            return_clause,
            body: Box::new(lower_expr(body)),
            position: Some(pos),
        },
        None => osprey_ast::handler_value(stage, effect, arms, return_clause, Some(pos)),
    }
}

pub(in crate::ml) fn lower_multi_application(func: MlExpr, args: Vec<MlExpr>) -> Expr {
    positional_construction(&func, &args)
        .unwrap_or_else(|| call(lower_expr(func), args.into_iter().map(lower_expr).collect()))
}

/// Uppercase heads construct a type; lowercase heads update a bound record.
/// This mirrors the Default flavor's distinct `TypeConstructor`/`Update` nodes.
pub(in crate::ml) fn lower_record(
    name: String,
    type_args: &[MlType],
    fields: Vec<MlField>,
) -> Expr {
    let fields = fields.into_iter().map(lower_field).collect();
    if !osprey_ast::is_record_constructor(&name, !type_args.is_empty()) {
        return Expr::Update {
            record: name,
            fields,
        };
    }
    Expr::TypeConstructor {
        name,
        // Explicit type arguments follow the same path as Default construction.
        // Implements [FLAVOR-ML-GENERICS].
        type_args: type_args
            .iter()
            .map(|arg| type_expr(arg).unwrap_or_else(|| TypeExpr::named(render_type(arg))))
            .collect(),
        fields,
    }
}

pub(in crate::ml) fn lower_lambda_node(
    params: Vec<MlParam>,
    uncurried: bool,
    body: MlExpr,
    pos: Position,
) -> Expr {
    let owner = if uncurried || params.iter().all(|p| matches!(p, MlParam::Unit)) {
        pos
    } else {
        curry_position(pos, params.len().saturating_sub(1))
    };
    let body = crate::ml::binding_ranges::with_owner(owner, || {
        in_scope(params_scope(&params), move || lower_function_body(body))
    });
    if !uncurried {
        return lower_lambda(params, body, pos);
    }
    Expr::Lambda {
        parameters: flat_params(params, pos),
        return_type: None,
        body: Box::new(body),
        position: Some(pos),
    }
}

/// Lower one `op param* => body` handle arm to the canonical [`HandlerArm`] —
/// byte-identical to the Default handler arm ([FLAVOR-ML-EFFECT]).
pub(in crate::ml) fn lower_handle_arm(arm: MlHandleArm) -> HandlerArm {
    HandlerArm {
        operation: arm.operation,
        params: arm
            .params
            .into_iter()
            .map(|binder| crate::ml::binding_ranges::handler(binder, arm.pos))
            .collect(),
        body: crate::ml::binding_ranges::with_owner(arm.pos, || lower_expr(arm.body)),
        position: Some(arm.pos),
    }
}

/// `|>` desugars to a call (the pipe is invisible downstream); every other
/// operator is a canonical [`Expr::Binary`].
pub(in crate::ml) fn lower_binary(op: &str, left: MlExpr, right: MlExpr, pos: Position) -> Expr {
    let left = lower_expr(left);
    let right = lower_expr(right);
    if op == "|>" {
        return pipe_into(left, right);
    }
    if op == crate::ml::parser::ELVIS_OP {
        return result_default(left, right);
    }
    Expr::Binary {
        position: Some(pos),
        op: op.to_owned(),
        left: Box::new(left),
        right: Box::new(right),
    }
}

/// A function's return-only layout body retains its own executable source line.
/// Value bindings keep the canonical expression shape. [DEBUGGER-SOURCE-MAP]
pub(in crate::ml) fn lower_function_body(body: MlExpr) -> Expr {
    let position = match &body {
        MlExpr::Block { pos, .. } => *pos,
        _ => None,
    };
    match (lower_expr(body), position) {
        (body @ (Expr::Block { .. } | Expr::Handler { .. }), _) | (body, None) => body,
        (body, position) => Expr::Block {
            statements: Vec::new(),
            value: Some(Box::new(body)),
            position,
        },
    }
}

/// A block lowers to the flavor-neutral block shape ([`crate::desugar::block`]).
pub(in crate::ml) fn lower_block(
    items: Vec<MlItem>,
    value: Option<Box<MlExpr>>,
    position: Option<Position>,
) -> Expr {
    let (statements, value) = in_scope(scope_of(&items), move || {
        (lower_items(items), value.map(|v| Box::new(lower_expr(*v))))
    });
    crate::desugar::block(statements, value, position)
}

/// `e ?: d` — the explicit Result default ([PATTERN-RESULT-DEFAULT]). Both
/// flavors emit the same exhaustive Success/Error match.
pub(in crate::ml) fn result_default(scrutinee: Expr, fallback: Expr) -> Expr {
    crate::desugar::result_default(scrutinee, fallback)
}

pub(in crate::ml) fn lower_arm(arm: MlArm) -> MatchArm {
    MatchArm {
        pattern: lower_pattern(arm.pattern),
        body: lower_expr(arm.body),
    }
}

pub(in crate::ml) fn lower_pattern(pattern: MlPattern) -> Pattern {
    match pattern {
        MlPattern::Wildcard => Pattern::Wildcard,
        MlPattern::Int(n) => Pattern::Literal(Box::new(Expr::Integer(n))),
        MlPattern::Str(raw) => Pattern::Literal(Box::new(lower_string(&raw, None))),
        MlPattern::Bool(b) => Pattern::Literal(Box::new(Expr::Bool(b))),
        MlPattern::Bind(name) => Pattern::Binding(crate::ml::binding_ranges::pattern(name)),
        MlPattern::Structural { fields, open } => Pattern::Structural {
            fields: fields
                .into_iter()
                .map(crate::ml::binding_ranges::pattern)
                .map(|f| (f.clone(), f))
                .collect(),
            open,
        },
        // The parser guarantees every slot is a binder or `_`; `_` binds
        // nothing, spelled as the empty binder.
        MlPattern::Tuple(elements) => osprey_ast::tuple_pattern(
            elements
                .into_iter()
                .map(|p| match p {
                    MlPattern::Bind(name) => crate::ml::binding_ranges::pattern(name),
                    _ => String::new(),
                })
                .collect(),
        ),
        MlPattern::Ctor { name, fields } => crate::desugar::ctor_pattern(
            name,
            fields
                .into_iter()
                .map(crate::ml::binding_ranges::pattern)
                .collect(),
        ),
        MlPattern::List { elements, rest } => Pattern::List {
            elements: elements.into_iter().map(lower_pattern).collect(),
            rest: rest.map(crate::ml::binding_ranges::pattern),
        },
    }
}

pub(in crate::ml) fn lower_field(field: MlField) -> FieldAssignment {
    FieldAssignment {
        name: field.name,
        value: lower_expr(field.value),
    }
}

/// Lower one `key => value` map entry to a canonical [`MapEntry`] — byte-identical
/// to the Default `{ key: value }` entry ([FLAVOR-ML-MAP]).
pub(in crate::ml) fn lower_map_entry((key, value): (MlExpr, MlExpr)) -> MapEntry {
    MapEntry {
        key: lower_expr(key),
        value: lower_expr(value),
    }
}

/// `x |> f a` → `f x a`: prepend the piped value as the first argument of the
/// right-hand call, or wrap a bare callee in a one-argument call.
/// Implements [BUILTIN-ITER-PIPE].
pub(in crate::ml) fn pipe_into(left: Expr, right: Expr) -> Expr {
    match right {
        Expr::Call {
            function,
            mut arguments,
            named_arguments,
        } => {
            arguments.insert(0, left);
            Expr::Call {
                function,
                arguments,
                named_arguments,
            }
        }
        callee => call(callee, vec![left]),
    }
}

/// Build a single positional [`Expr::Call`] node.
pub(in crate::ml) fn call(function: Expr, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function: Box::new(function),
        arguments,
        named_arguments: Vec::new(),
    }
}

/// Lower a whitespace-application spine ([FLAVOR-ML-CURRY]). The spine
/// `((head a) b) c` is collected into `(head, [a, b, c])`, then:
/// - if `head` is a user binding (or a non-identifier callee like a closure
///   value), the spine stays CURRIED — nested one-argument calls
///   `Call(Call(Call(head,[a]),[b]),[c])` — so partial application works and the
///   form is byte-identical to the Default explicit-curry `head(a)(b)(c)`;
/// - otherwise `head` is a multi-argument builtin or `extern` that cannot be
///   partially applied, so the SATURATED spine folds to ONE flat call
///   `Call(head, [a, b, c])` — the saturated-call optimisation the spec assigns
///   to the backend, applied here while the surface spine is still visible.
pub(in crate::ml) fn lower_application(func: MlExpr, arg: MlExpr) -> Expr {
    let mut args = vec![arg];
    let mut head = func;
    while let MlExpr::App { func, arg } = head {
        args.push(*arg);
        head = *func;
    }
    args.reverse();
    if let Some(built) = positional_construction(&head, &args) {
        return built;
    }
    let curried = match &head {
        MlExpr::Ident(name) => is_bound(name),
        // Qualified and higher-order callables preserve ML's curry-by-default
        // spine; `(a, b)` explicitly selects a flat interop call.
        _ => true,
    };
    if curried {
        args.into_iter()
            .fold(lower_expr(head), |acc, a| call(acc, vec![lower_expr(a)]))
    } else {
        call(lower_expr(head), args.into_iter().map(lower_expr).collect())
    }
}

/// Lower a raw string token to a plain or interpolated string expression,
/// reusing the Default frontend's escape/`${…}` handling with an ML fragment
/// parser ([FLAVOR-FRONTEND]).
pub(in crate::ml) fn lower_string(raw: &str, pos: Option<Position>) -> Expr {
    crate::ml::binding_ranges::literal(pos);
    if raw.contains("${") {
        Expr::InterpolatedStr(lower_interpolation(
            &format!("\"{raw}\""),
            pos,
            crate::Flavor::Ml,
            parse_fragment,
        ))
    } else {
        Expr::Str(unquote(raw))
    }
}

/// Parse a `${…}` fragment as an ML expression (`${toString id}` is ML
/// application), threading the flavor through interpolation re-entry.
pub(in crate::ml) fn parse_fragment(frag: &str) -> Expr {
    let (items, _) = crate::ml::parser::parse(&format!(
        "{}{frag}\n",
        crate::strings::fragment_binding(crate::Flavor::Ml)
    ));
    match items.into_iter().next() {
        Some(MlItem::Binding { body, .. }) => {
            crate::ml::binding_ranges::isolated(|| lower_expr(body))
        }
        _ => Expr::Identifier(frag.trim().to_owned()),
    }
}

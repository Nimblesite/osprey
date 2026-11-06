//! Immutable reactive snapshots. Implements [STAGE-SIGNALS-CACHE], [STAGE-SIGNALS-REBUILD].
use super::StaticEffects;
use osprey_ast::{
    mutate, walk_program, AstVisitor, Expr, FieldAssignment, MatchArm, NamedArgument, Pattern,
    Program, Stmt,
};

pub(super) fn is_intrinsic(name: &str) -> bool {
    matches!(
        name,
        "signalDependencies" | "signalSnapshot" | "signalRefresh"
    )
}

/// Freeze checker-derived metadata before discharge removes signal operations.
/// Traverse source and clone in the same preorder; pointer identities never
/// cross the clone boundary. Implements [STAGE-SIGNALS-EXACT].
pub(super) fn materialize_signal_queries(
    program: &Program,
    types: &crate::ProgramTypes,
) -> Program {
    let mut effects = StaticEffects::default();
    walk_program(program, &mut effects);
    let mut queries = SignalQueries {
        types,
        effects,
        values: Vec::new(),
    };
    walk_program(program, &mut queries);
    let mut values = queries.values.into_iter();
    let mut resolved = program.clone();
    for statement in &mut resolved.statements {
        mutate::statement_children_mut(statement, &mut |expression| {
            materialize_query(expression, &mut values);
        });
    }
    resolved
}

struct SignalQueries<'a> {
    types: &'a crate::ProgramTypes,
    effects: StaticEffects,
    values: Vec<Option<Vec<String>>>,
}

impl AstVisitor for SignalQueries<'_> {
    fn expression(&mut self, expression: &Expr) {
        let query = match expression {
            Expr::Call { function, .. } => self
                .types
                .signal_queries
                .get(&std::ptr::from_ref(function.as_ref()).addr()),
            _ => None,
        };
        self.values
            .push(query.map(|row| self.effects.dependencies(row)));
    }
}

fn materialize_query(
    expression: &mut Expr,
    values: &mut impl Iterator<Item = Option<Vec<String>>>,
) {
    let query = values.next().flatten();
    mutate::children_mut(expression, &mut |child| materialize_query(child, values));
    if let Some(operations) = query {
        *expression = expand(expression, operations);
    }
}

fn expand(expression: &Expr, operations: Vec<String>) -> Expr {
    let Expr::Call {
        function,
        arguments,
        named_arguments,
    } = expression
    else {
        return expression.clone();
    };
    let Some(name) = callee_name(function) else {
        return expression.clone();
    };
    let mut statements = bound_arguments(name, arguments, named_arguments);
    if name == "signalRefresh" {
        return Expr::Block {
            statements,
            value: Some(Box::new(refresh(operations))),
            position: None,
        };
    }
    statements.push(dependency_binding(operations));
    let value = match name {
        "signalSnapshot" => snapshot(Expr::Integer(1)),
        _ => ident("$signalDependencies"),
    };
    Expr::Block {
        statements,
        value: Some(Box::new(value)),
        position: None,
    }
}

pub(super) fn callee_name(function: &Expr) -> Option<&str> {
    match function {
        Expr::Identifier(name) => Some(name),
        Expr::TypeApply { function, .. } => callee_name(function),
        _ => None,
    }
}

fn bound_arguments(name: &str, arguments: &[Expr], named: &[NamedArgument]) -> Vec<Stmt> {
    let slots = if name == "signalRefresh" {
        vec!["$signalCached", "$signalChanged", "$signalWork"]
    } else {
        vec!["$signalWork"]
    };
    let positional = slots
        .iter()
        .zip(arguments)
        .filter(|(_, value)| needs_binding(name, value))
        .map(|(name, value)| binding(name, value.clone()));
    let named = named
        .iter()
        .filter(|argument| needs_binding(name, &argument.value))
        .map(|argument| {
            binding(
                match argument.name.as_str() {
                    "cached" => "$signalCached",
                    "changed" => "$signalChanged",
                    _ => "$signalWork",
                },
                argument.value.clone(),
            )
        });
    positional.chain(named).collect()
}

/// A dependency query never calls its work, so work whose evaluation has no
/// effect -- a name or a lambda -- is not bound at all: a bound name would keep
/// a static performer referenced, and static lowering prunes every function
/// that references one ([STAGE-SIGNALS-CACHE], [STAGE-RESIDUE]).
fn needs_binding(query: &str, value: &Expr) -> bool {
    query != "signalDependencies" || !matches!(value, Expr::Identifier(_) | Expr::Lambda { .. })
}

fn dependency_binding(operations: Vec<String>) -> Stmt {
    let empty = operations.is_empty();
    let mut declaration = binding(
        "$signalDependencies",
        Expr::List(operations.into_iter().map(Expr::Str).collect(), None),
    );
    if empty {
        if let Stmt::Let { ty, .. } = &mut declaration {
            *ty = Some(string_list_type());
        }
    }
    declaration
}

/// Rebuild when stale, otherwise keep the cached snapshot. Only a rebuild
/// builds the dependency list it stores.
fn refresh(operations: Vec<String>) -> Expr {
    let stale = std::iter::once(binary("==", ident("$signalSame"), Expr::Bool(false)))
        .chain(changed(&operations))
        .reduce(|left, right| binary("||", left, right))
        .unwrap_or(Expr::Bool(true));
    let same = same_set(&operations);
    let rebuilt = Expr::Block {
        statements: vec![binding("$signalDependencies", reused(operations))],
        value: Some(Box::new(snapshot(call(
            "satAdd",
            vec![field("revision"), Expr::Integer(1)],
        )))),
        position: None,
    };
    Expr::Block {
        statements: vec![binding("$signalSame", same)],
        value: Some(Box::new(Expr::Match {
            value: Box::new(stale),
            arms: vec![arm(true, rebuilt), arm(false, ident("$signalCached"))],
        })),
        position: None,
    }
}

const UNKNOWN: &str = "<unknown>";

/// Whether the cached may-read set is this call's set. The set is a
/// compile-time constant, so the comparison is unrolled against literals and
/// allocates nothing; both sets are duplicate-free, so equal length plus
/// containment of every member is set equality. An unknown set is never the
/// same, so it always rebuilds [STAGE-SIGNALS-CACHE].
fn same_set(operations: &[String]) -> Expr {
    if operations.iter().any(|operation| operation == UNKNOWN) {
        return Expr::Bool(false);
    }
    let count = i64::try_from(operations.len()).unwrap_or(i64::MAX);
    let sized = binary(
        "==",
        call("length", vec![field("dependencies")]),
        Expr::Integer(count),
    );
    std::iter::once(sized)
        .chain(
            operations
                .iter()
                .map(|operation| contains(field("dependencies"), operation)),
        )
        .reduce(|left, right| binary("&&", left, right))
        .unwrap_or(Expr::Bool(false))
}

/// Whether a change is unknown or names one of this call's dependencies.
fn changed(operations: &[String]) -> impl Iterator<Item = Expr> + '_ {
    std::iter::once(UNKNOWN)
        .chain(operations.iter().map(String::as_str))
        .map(|operation| contains(ident("$signalChanged"), operation))
}

/// A rebuild keeps the cached list when the set is unchanged, so only a
/// replaced callback builds a new one.
fn reused(operations: Vec<String>) -> Expr {
    let fresh = Expr::Block {
        statements: vec![dependency_binding(operations)],
        value: Some(Box::new(ident("$signalDependencies"))),
        position: None,
    };
    Expr::Match {
        value: Box::new(ident("$signalSame")),
        arms: vec![arm(true, field("dependencies")), arm(false, fresh)],
    }
}

fn contains(list: Expr, operation: &str) -> Expr {
    call("listContains", vec![list, Expr::Str(operation.to_owned())])
}

fn snapshot(revision: Expr) -> Expr {
    Expr::Object(vec![
        assignment("dependencies", ident("$signalDependencies")),
        assignment("value", call("$signalWork", vec![])),
        assignment("revision", revision),
    ])
}

fn assignment(name: &str, value: Expr) -> FieldAssignment {
    FieldAssignment {
        name: name.to_owned(),
        value,
    }
}

fn binding(name: &str, value: Expr) -> Stmt {
    Stmt::Let {
        name: name.to_owned(),
        mutable: false,
        ty: None,
        value,
        doc: None,
        position: None,
    }
}

fn ident(name: &str) -> Expr {
    Expr::Identifier(name.to_owned())
}

fn field(name: &str) -> Expr {
    Expr::FieldAccess {
        target: Box::new(ident("$signalCached")),
        field: name.to_owned(),
    }
}

fn call(name: &str, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        function: Box::new(ident(name)),
        arguments,
        named_arguments: Vec::new(),
    }
}

fn binary(op: &str, left: Expr, right: Expr) -> Expr {
    Expr::Binary {
        op: op.to_owned(),
        left: Box::new(left),
        right: Box::new(right),
        position: None,
    }
}

fn arm(value: bool, body: Expr) -> MatchArm {
    MatchArm {
        pattern: Pattern::Literal(Box::new(Expr::Bool(value))),
        body,
    }
}

fn string_list_type() -> osprey_ast::TypeExpr {
    let mut ty = osprey_ast::TypeExpr::named("List");
    ty.generic_params
        .push(osprey_ast::TypeExpr::named("string"));
    ty
}

// Read argument labels from the same documentation/signature table as tooling.
pub(crate) fn parameter_names() -> std::collections::HashMap<String, Vec<String>> {
    crate::builtin_names()
        .into_iter()
        .filter(|name| is_intrinsic(name))
        .filter_map(|name| {
            crate::builtin_docs::parameter_names(&name).map(|parameters| (name, parameters))
        })
        .collect()
}

#[cfg(test)]
mod tests;

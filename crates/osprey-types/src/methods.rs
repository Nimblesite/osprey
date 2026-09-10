//! Type-resolved dotted calls become ordinary calls in the backend copy.
//! Implements [BUILTIN-STRING-UFCS]: record fields win before UFCS fallback.

use osprey_ast::{AstVisitor, Expr, NamedArgument, Position, Program, TypeExpr};
use std::collections::HashMap;

pub(crate) type Targets = HashMap<usize, Target>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Target {
    Field(String),
    Function,
    Deferred(String),
}

#[derive(Clone, Copy)]
pub(crate) struct Parts<'a> {
    pub target: &'a Expr,
    pub method: &'a str,
    pub arguments: &'a [Expr],
    pub named: &'a [NamedArgument],
    pub application: Option<(&'a [TypeExpr], Option<Position>)>,
}

/// `function` wrapped in the call site's explicit type application, or left
/// alone when the site wrote no turbofish — the one rule both the deferred
/// method-call rewrite and the ordinary call rewrite apply.
fn applied(function: Expr, application: Option<(&[TypeExpr], Option<Position>)>) -> Expr {
    match application {
        Some((type_args, position)) => Expr::TypeApply {
            function: Box::new(function),
            type_args: type_args.to_vec(),
            position,
        },
        None => function,
    }
}

pub(crate) fn parts(expression: &Expr) -> Option<Parts<'_>> {
    match expression {
        Expr::MethodCall {
            target,
            method,
            arguments,
            named_arguments,
        } => Some(Parts {
            target,
            method,
            arguments,
            named: named_arguments,
            application: None,
        }),
        Expr::Call {
            function,
            arguments,
            named_arguments,
        } => {
            let Expr::TypeApply {
                function,
                type_args,
                position,
            } = function.as_ref()
            else {
                return None;
            };
            let Expr::FieldAccess { target, field } = function.as_ref() else {
                return None;
            };
            Some(Parts {
                target,
                method: field,
                arguments,
                named: named_arguments,
                application: Some((type_args, *position)),
            })
        }
        _ => None,
    }
}

/// Preserve the source field spelling when assembly qualified a free function.
pub(crate) fn field_name(method: &str) -> String {
    let source = osprey_ast::symbol::demangle(method).unwrap_or_else(|| method.to_owned());
    source.rsplit("::").next().unwrap_or(&source).to_owned()
}

pub(crate) fn collect(program: &Program, sites: &Targets) -> Targets {
    struct Collector<'a> {
        sites: &'a Targets,
        index: usize,
        targets: Targets,
    }
    impl AstVisitor for Collector<'_> {
        fn expression(&mut self, expression: &Expr) {
            if let Some(field) = self.sites.get(&std::ptr::from_ref(expression).addr()) {
                let _ = self.targets.insert(self.index, field.clone());
            }
            self.index += 1;
        }
    }
    let mut collector = Collector {
        sites,
        index: 0,
        targets: HashMap::new(),
    };
    osprey_ast::walk_program(program, &mut collector);
    collector.targets
}

pub(crate) fn lower(expression: &mut Expr, target: &Target) {
    let Some(parts) = parts(expression) else {
        return;
    };
    if matches!(target, Target::Deferred(_)) {
        let call = Expr::MethodCall {
            target: Box::new(parts.target.clone()),
            method: parts.method.to_owned(),
            arguments: parts.arguments.to_vec(),
            named_arguments: parts.named.to_vec(),
        };
        *expression = applied(call, parts.application);
        return;
    }
    let mut arguments = parts.arguments.to_vec();
    let function = if let Target::Field(field) = target {
        Expr::FieldAccess {
            target: Box::new(parts.target.clone()),
            field: field.to_owned(),
        }
    } else {
        arguments.insert(0, parts.target.clone());
        Expr::Identifier(parts.method.to_owned())
    };
    let function = applied(function, parts.application);
    *expression = Expr::Call {
        function: Box::new(function),
        arguments,
        named_arguments: parts.named.to_vec(),
    };
}

use crate::check::Checker;
use crate::error::TypeError;
use crate::ty::Type;

pub(crate) const OBLIGATION: &str = "$method:";

impl Checker {
    /// A generic dotted call keeps both possible callees until its receiver
    /// resolves. The relation travels with the ordinary HM scheme, including
    /// variables used only by the selected callee's deferred constraints.
    pub(crate) fn resolve_method_use(&mut self, name: &str, relation: &Type) -> bool {
        let Some(descriptor) = name.strip_prefix(OBLIGATION) else {
            return false;
        };
        let mut descriptor = descriptor.splitn(5, ':');
        let explicit = descriptor.next().and_then(|n| n.parse::<usize>().ok());
        let line = descriptor
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
        let column = descriptor
            .next()
            .and_then(|n| n.parse::<u32>().ok())
            .unwrap_or(0);
        let Some(field) = descriptor.next() else {
            return false;
        };
        let Some(method) = descriptor.next() else {
            return false;
        };
        let Type::Con { args: relation, .. } = relation else {
            return false;
        };
        let [receiver, fallback, call, obligations, binders, written, named] = relation.as_slice()
        else {
            return false;
        };
        let receiver = self.ctx.apply(receiver);
        if matches!(receiver, Type::Var(_)) {
            return false;
        }
        let Type::Fun {
            params: arguments,
            ret,
        } = call
        else {
            return false;
        };
        let position = (line != 0).then_some(Position { line, column });
        let selected = self.resolved_record_field(&receiver, field).ok();
        let mut arguments = ordered_arguments(
            arguments,
            named,
            selected
                .is_none()
                .then(|| self.fn_params.get(method))
                .flatten(),
        );
        let result = if let Some(selected) = selected {
            if let Some(count) = explicit {
                self.errors.push(
                    TypeError::new(format!(
                        "function `{field}` takes 0 type argument(s), got {count}"
                    ))
                    .with_pos(position),
                );
            }
            self.apply_named_fn(None, &selected, arguments)
        } else {
            self.resolve_method_binders(method, explicit, position, binders, written);
            self.builtin_uses.extend(unpack_obligations(obligations));
            arguments.insert(0, receiver);
            self.apply_named_fn(Some(method), fallback, arguments)
        };
        self.push_unify(ret, &result);
        true
    }

    fn resolve_method_binders(
        &mut self,
        method: &str,
        explicit: Option<usize>,
        position: Option<Position>,
        binders: &Type,
        written: &Type,
    ) {
        let (Some(count), Type::Con { args: binders, .. }, Type::Con { args: written, .. }) =
            (explicit, binders, written)
        else {
            return;
        };
        if binders.len() == count {
            for (binder, written) in binders.iter().zip(written) {
                self.push_unify(binder, written);
            }
        } else {
            self.errors.push(
                TypeError::new(format!(
                    "function `{method}` takes {} type argument(s), got {count}",
                    binders.len()
                ))
                .with_pos(position),
            );
        }
    }
}

fn ordered_arguments(
    arguments: &[Type],
    named: &Type,
    parameters: Option<&Vec<String>>,
) -> Vec<Type> {
    let mut arguments = arguments.to_vec();
    let Type::Con { args: named, .. } = named else {
        return arguments;
    };
    let mut supplied: Vec<_> = named
        .iter()
        .filter_map(|argument| match argument {
            Type::Con { name, args } => args.first().map(|ty| (name, ty)),
            _ => None,
        })
        .collect();
    if let Some(parameters) = parameters {
        supplied.sort_by_key(|(name, _)| parameters.iter().position(|p| p == *name));
    }
    arguments.extend(supplied.into_iter().map(|(_, ty)| ty.clone()));
    arguments
}

fn unpack_obligations(obligations: &Type) -> Vec<(String, Type)> {
    let Type::Con {
        args: obligations, ..
    } = obligations
    else {
        return Vec::new();
    };
    obligations
        .iter()
        .filter_map(|obligation| match obligation {
            Type::Con { name, args } => args.first().map(|ty| (name.clone(), ty.clone())),
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::testutil::{accepts, rejects_with};
    use osprey_syntax::Flavor;

    #[test]
    fn a_generic_receiver_selects_its_field_before_a_same_named_function() {
        accepts(
            Flavor::Default,
            r#"type Action = Action { m: () -> int }
fn field() -> int = 11
fn m<T>(x: T) -> string = "free"
fn invoke<T>(x: T) = x.m()
fn recordResult() -> int = invoke(Action { m: field })
fn scalarResult() -> string = invoke(3)
print("${recordResult()} ${scalarResult()}")"#,
        );
    }

    #[test]
    fn deferred_named_arguments_follow_the_selected_free_functions_parameter_order() {
        for application in ["choose", "choose<T>"] {
            accepts(
                Flavor::Default,
                format!(
                    r#"fn choose<T>(receiver: T, text: string, count: int) = "${{text}} ${{count}}"
fn invoke<T>(receiver: T) = receiver.{application}(count: 7, text: "generic")
fn result() -> string = invoke(4)
print(result())"#
                ),
            );
        }
    }

    #[test]
    fn field_dispatch_keeps_only_the_selected_callees_constraints() {
        accepts(
            Flavor::Default,
            r#"type Action = Action { m: () -> bool }
fn field() -> bool = true
fn m<T>(x: T) = length(x)
fn invoke<T>(x: T) = x.m()
fn recordResult() -> bool = invoke(Action { m: field })
fn stringResult() -> int = invoke("abc")
fn listResult() -> int = invoke([1, 2])
print("${recordResult()} ${stringResult()} ${listResult()}")"#,
        );
    }

    #[test]
    fn a_generic_callback_field_keeps_its_argument_and_return_contract() {
        let declarations = r"type Sink<in T> = Sink { hof: ((int) -> T) -> int }
fn callbackResult(f) = 7
fn invoke(g) = g.hof(|n| => 3)
";
        accepts(
            Flavor::Default,
            format!(
                "{declarations}fn result() -> int = invoke(Sink<int> {{ hof: callbackResult }})\n"
            ),
        );
        rejects_with(
            Flavor::Default,
            format!("{declarations}fn result() -> int = invoke(Sink<string> {{ hof: callbackResult }})\n"),
            "cannot unify",
        );
    }

    #[test]
    fn a_field_cannot_borrow_a_same_named_functions_explicit_binders() {
        for invocation in [
            "fn result() = Action { m: field }.m<int>()",
            "fn invoke<T>(x: T) = x.m<int>()\nfn result() = invoke(Action { m: field })",
        ] {
            rejects_with(
                Flavor::Default,
                format!(
                    "type Action = Action {{ m: () -> int }}\nfn field() = 11\nfn m<T>(x: T) = 22\n{invocation}\n"
                ),
                "function `m` takes 0 type argument(s), got 1",
            );
        }
    }

    #[test]
    fn a_deferred_free_function_checks_written_binder_arity_and_value_types() {
        for (application, message) in [
            (
                "m<int, string>",
                "function `m` takes 1 type argument(s), got 2",
            ),
            ("m<string>", "cannot unify"),
        ] {
            rejects_with(
                Flavor::Default,
                format!("fn m<T>(x: T) = x\nfn invoke<T>(x: T) = x.{application}()\nfn result() = invoke(3)\n"),
                message,
            );
        }
    }

    #[test]
    fn explicit_arguments_validate_nested_types_and_enclosing_binders() {
        for (argument, message) in [
            ("T<int>", "type parameter `T` cannot take type arguments"),
            (
                "List<T<int>>",
                "type parameter `T` cannot take type arguments",
            ),
            ("int<string>", "type `int` takes 0 type argument(s), got 1"),
            (
                "List<int, string>",
                "type `List` takes 1 type argument(s), got 2",
            ),
            ("(Unknown) -> int", "unknown type `Unknown`"),
            ("(int) -> Unknown", "unknown type `Unknown`"),
        ] {
            rejects_with(
                Flavor::Default,
                format!(
                    "fn empty<A>() -> List<A> = []\nfn invoke<T>(x: T) = empty<{argument}>()\n"
                ),
                message,
            );
        }
    }
}

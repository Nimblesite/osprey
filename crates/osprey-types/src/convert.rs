//! Turn syntactic type annotations (`ast::TypeExpr`, and the bare field-type
//! strings the grammar stores on records/effects) into inference [`Type`]s.
//!
//! A `params` map carries the in-scope generic parameters (`T`, `K`, ...) so a
//! `type Box<T>` field of type `T` becomes the *same* fresh variable everywhere
//! it appears in that declaration.

use crate::ty::{names, Type};
use osprey_ast::TypeExpr;
use std::collections::HashMap;

/// Convert a parsed `TypeExpr` into an inference type. `params` maps generic
/// parameter names already bound to fresh variables for the enclosing decl.
pub(crate) fn type_expr_to_type(te: &TypeExpr, params: &HashMap<String, Type>) -> Type {
    if te.is_function {
        let ps = te
            .parameter_types
            .iter()
            .map(|p| type_expr_to_type(p, params))
            .collect();
        let ret = te
            .return_type
            .as_ref()
            .map_or_else(Type::unit, |r| type_expr_to_type(r, params));
        return Type::fun(ps, ret);
    }
    if te.is_array {
        let elem = te
            .array_element
            .as_ref()
            .map_or_else(Type::any, |e| type_expr_to_type(e, params));
        return Type::list(elem);
    }
    if let Some(var) = params.get(&te.name) {
        return var.clone();
    }
    if te.generic_params.is_empty() {
        normalize_named(&te.name)
    } else {
        let args = te
            .generic_params
            .iter()
            .map(|g| type_expr_to_type(g, params))
            .collect();
        Type::con(te.name.clone(), args)
    }
}

/// Convert a bare field/effect type string (e.g. `"int"`, `"T"`,
/// `"Result<string, Error>"`, `"(int) -> bool"`) into a type. The shallow
/// `Name<...>`, `[elem]` and function-arrow forms are recognised; that covers
/// every field type used by the examples.
pub(crate) fn type_name_to_type(s: &str, params: &HashMap<String, Type>) -> Type {
    let s = s.trim();
    if let Some(var) = params.get(s) {
        return var.clone();
    }
    // A function type — `fn(int) -> bool` or `(int) -> bool` — parses through
    // the same tolerant parser effect-operation signatures use.
    if s.starts_with("fn(") || (s.starts_with('(') && s.contains("->")) {
        let (ps, ret) = parse_fn_sig(s, params);
        return Type::fun(ps, ret);
    }
    if let Some(open) = s.find('<') {
        if s.ends_with('>') {
            let head = s[..open].trim();
            let inner = &s[open + 1..s.len() - 1];
            let args = split_generic_args(inner)
                .iter()
                .map(|a| type_name_to_type(a, params))
                .collect();
            return Type::con(head.to_string(), args);
        }
    }
    if let Some(elem) = s.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return Type::list(type_name_to_type(elem, params));
    }
    normalize_named(s)
}

/// Map a bare type name to a canonical primitive where one exists, otherwise a
/// nullary constructor (a user type or generic placeholder name).
fn normalize_named(name: &str) -> Type {
    match name {
        "Int" | names::INT => Type::int(),
        "Float" | names::FLOAT => Type::float(),
        "String" | names::STRING => Type::string(),
        "Bool" | names::BOOL => Type::bool(),
        names::ANY => Type::any(),
        names::UNIT | "" => Type::unit(),
        other => Type::prim(other.to_string()),
    }
}

/// Parse an effect-operation / lambda type string `fn(p0, p1) -> ret` into
/// inference types. Tolerant: a malformed string yields `() -> Unit`.
pub(crate) fn parse_fn_sig(s: &str, params: &HashMap<String, Type>) -> (Vec<Type>, Type) {
    let s = s.trim();
    let s = s.strip_prefix("fn").map_or(s, str::trim_start).trim();
    let Some(open) = s.find('(') else {
        return (Vec::new(), Type::unit());
    };
    let Some(close) = matching_paren(s, open) else {
        return (Vec::new(), Type::unit());
    };
    let inner = s[open + 1..close].trim();
    let ps = if inner.is_empty() {
        Vec::new()
    } else {
        split_generic_args(inner)
            .iter()
            .map(|a| type_name_to_type(a, params))
            .collect()
    };
    let ret = s[close + 1..]
        .trim()
        .strip_prefix("->")
        .map_or_else(Type::unit, |r| type_name_to_type(r.trim(), params));
    (ps, ret)
}

fn matching_paren(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices().skip(open) {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Split `a, b<c, d>, (e) -> f` on top-level commas only, respecting `<>`,
/// `()` and `[]` nesting.
fn split_generic_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '(' | '[' => depth += 1,
            // The `>` in a function arrow does not close a generic argument.
            '>' if s[..i].ends_with('-') => {}
            '>' | ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                out.push(s[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        out.push(last.to_string());
    }
    out
}

#[cfg(test)]
#[expect(
    unused_results,
    reason = "tests drive inference for its side effects and discard the returned types"
)]
mod tests {
    use super::*;

    #[test]
    fn converts_primitive_and_array() {
        let m = HashMap::new();
        assert_eq!(type_name_to_type("int", &m), Type::int());
        assert_eq!(
            type_name_to_type("[string]", &m),
            Type::list(Type::string())
        );
    }

    #[test]
    fn converts_generic_and_param() {
        let mut m = HashMap::new();
        m.insert("T".to_string(), Type::Var(7));
        assert_eq!(type_name_to_type("T", &m), Type::Var(7));
        assert_eq!(
            type_name_to_type("Result<int, Error>", &m),
            Type::result(Type::int(), Type::prim("Error"))
        );
    }

    fn arr(elem: TypeExpr) -> TypeExpr {
        let mut te = TypeExpr::named("");
        te.is_array = true;
        te.array_element = Some(Box::new(elem));
        te
    }

    #[test]
    fn type_expr_array_function_and_param_paths() {
        let mut m = HashMap::new();
        m.insert("T".to_string(), Type::Var(5));
        // `[string]` → List<string>; an array with no element type → List<any>.
        assert_eq!(
            type_expr_to_type(&arr(TypeExpr::named("string")), &m),
            Type::list(Type::string())
        );
        assert_eq!(
            type_expr_to_type(
                &{
                    let mut te = TypeExpr::named("");
                    te.is_array = true;
                    te
                },
                &m
            ),
            Type::list(Type::any())
        );
        // A bare generic-parameter name resolves to its bound variable.
        assert_eq!(type_expr_to_type(&TypeExpr::named("T"), &m), Type::Var(5));
        // `fn(T) -> int`, plus a function with no declared return → Unit.
        let mut fn_te = TypeExpr::named("");
        fn_te.is_function = true;
        fn_te.parameter_types = vec![TypeExpr::named("T")];
        fn_te.return_type = Some(Box::new(TypeExpr::named("int")));
        assert_eq!(
            type_expr_to_type(&fn_te, &m),
            Type::fun(vec![Type::Var(5)], Type::int())
        );
        let mut noret = TypeExpr::named("");
        noret.is_function = true;
        assert_eq!(
            type_expr_to_type(&noret, &m),
            Type::fun(Vec::new(), Type::unit())
        );
        // An applied generic head builds a `Con`.
        let mut boxed = TypeExpr::named("Box");
        boxed.generic_params = vec![TypeExpr::named("int")];
        assert_eq!(
            type_expr_to_type(&boxed, &m),
            Type::con("Box", vec![Type::int()])
        );
    }

    #[test]
    fn type_name_to_type_nested_generics_and_fn_sigs() {
        let m = HashMap::new();
        // Generic head with a nested generic argument exercises `split_generic_args`
        // depth tracking over `<>`.
        assert_eq!(
            type_name_to_type("Map<string, List<int>>", &m),
            Type::map(Type::string(), Type::list(Type::int()))
        );
        // `fn(int) -> bool` and the paren form share `parse_fn_sig`.
        assert_eq!(
            type_name_to_type("fn(int) -> bool", &m),
            Type::fun(vec![Type::int()], Type::bool())
        );
        assert_eq!(
            type_name_to_type("(int, string) -> bool", &m),
            Type::fun(vec![Type::int(), Type::string()], Type::bool())
        );
        // A `<` that is not closed by a trailing `>` is not a generic: it falls
        // through to a nullary named type.
        assert_eq!(
            type_name_to_type("Weird<unclosed", &m),
            Type::prim("Weird<unclosed")
        );
    }

    #[test]
    fn parse_fn_sig_tolerates_malformed_input() {
        let m = HashMap::new();
        // No open paren at all → `() -> Unit`.
        assert_eq!(parse_fn_sig("fn", &m), (Vec::new(), Type::unit()));
        // Open paren without a matching close → `() -> Unit`.
        assert_eq!(parse_fn_sig("fn(int", &m), (Vec::new(), Type::unit()));
        // Empty params and no arrow → `() -> Unit`.
        assert_eq!(parse_fn_sig("fn()", &m), (Vec::new(), Type::unit()));
        // A nested-paren parameter exercises the `matching_paren` depth counter.
        assert_eq!(
            parse_fn_sig("fn((int) -> int) -> bool", &m),
            (
                vec![Type::fun(vec![Type::int()], Type::int())],
                Type::bool()
            )
        );
    }
}

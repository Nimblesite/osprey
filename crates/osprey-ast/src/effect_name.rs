//! What an effect mention names: one declaration, many instantiations.
//!
//! `Signal<Count>` and `Signal<Cursor>` are DIFFERENT effects to a row and to a
//! handler — that is exactly what makes a dependency set exact, and it is the
//! surface contract the reactive story rests on ([STAGE-SIGNALS-EXACT]). They
//! are the SAME declaration, so a lookup of what an effect *declares* reads the
//! base name while a lookup of what a row *requires* reads the whole mention.
//! Both spellings live here so the two can never drift apart.

use crate::TypeExpr;

/// The declaration a mention names: `Signal<Count>` is declared by `Signal`.
/// A mention with no instantiation is its own base.
#[must_use]
pub fn base(effect: &str) -> &str {
    effect.split_once('<').map_or(effect, |(head, _)| head)
}

/// The type arguments a mention instantiates its declaration at, or `None` when
/// it names none. `Signal<Count>` yields `Count`; `Log` yields `None`.
#[must_use]
pub fn instantiation(effect: &str) -> Option<&str> {
    let (_, rest) = effect.split_once('<')?;
    rest.strip_suffix('>')
}

/// How many arguments a mention instantiates its declaration at. Counted at
/// bracket depth zero, so `Signal<Result<int, Error>>` is ONE argument rather
/// than two — a nested generic is not a second signal.
#[must_use]
pub fn arity(effect: &str) -> usize {
    let Some(arguments) = instantiation(effect) else {
        return 0;
    };
    if arguments.trim().is_empty() {
        return 0;
    }
    let mut depth = 0_i32;
    arguments
        .chars()
        .filter(|c| separates(*c, &mut depth))
        .count()
        + 1
}

/// Whether `c` separates two top-level arguments, advancing `depth` past any
/// bracket it opens or closes.
fn separates(c: char, depth: &mut i32) -> bool {
    match c {
        '<' | '[' | '(' => *depth += 1,
        '>' | ']' | ')' => *depth -= 1,
        _ => {}
    }
    c == ',' && *depth == 0
}

/// The one spelling of an instantiated mention, used by every row entry and
/// every diagnostic so a name printed by one phase is the name matched by the
/// next. An empty argument list renders the bare declaration name.
#[must_use]
pub fn instantiated(base: &str, arguments: &[TypeExpr]) -> String {
    if arguments.is_empty() {
        return base.to_string();
    }
    let rendered: Vec<String> = arguments.iter().map(render).collect();
    format!("{base}<{}>", rendered.join(", "))
}

/// One type argument as this module spells it. Whitespace and the source's
/// choice of line breaks are normalised away, so `Signal< Count >` and
/// `Signal<Count>` are one dependency rather than two.
fn render(argument: &TypeExpr) -> String {
    if argument.is_array {
        let element = argument
            .array_element
            .as_deref()
            .map_or_else(String::new, render);
        return format!("[{element}]");
    }
    if argument.is_function {
        let parameters: Vec<String> = argument.parameter_types.iter().map(render).collect();
        let returned = argument
            .return_type
            .as_deref()
            .map_or_else(String::new, render);
        return format!("fn({}) -> {returned}", parameters.join(", "));
    }
    instantiated(&argument.name, &argument.generic_params)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(name: &str) -> TypeExpr {
        TypeExpr::named(name)
    }

    #[test]
    fn a_mention_splits_into_its_declaration_and_its_instantiation() {
        assert_eq!(base("Signal<Count>"), "Signal");
        assert_eq!(base("Log"), "Log");
        assert_eq!(base("store::Clocks::Clock"), "store::Clocks::Clock");
        assert_eq!(instantiation("Signal<Count>"), Some("Count"));
        assert_eq!(
            instantiation("Signal<Count, Cursor>"),
            Some("Count, Cursor")
        );
        assert_eq!(instantiation("Log"), None);
    }

    #[test]
    fn rendering_is_the_inverse_of_splitting_and_normalises_spacing() {
        assert_eq!(instantiated("Log", &[]), "Log");
        let one = instantiated("Signal", &[named("Count")]);
        assert_eq!(one, "Signal<Count>");
        assert_eq!(base(&one), "Signal");
        assert_eq!(
            instantiated("Pair", &[named("int"), named("string")]),
            "Pair<int, string>"
        );
        let mut nested = named("Result");
        nested.generic_params = vec![named("int"), named("Error")];
        assert_eq!(
            instantiated("Signal", &[nested]),
            "Signal<Result<int, Error>>"
        );
    }

    #[test]
    fn arity_counts_top_level_arguments_only() {
        assert_eq!(arity("Log"), 0);
        assert_eq!(arity("Signal<Count>"), 1);
        assert_eq!(arity("Pair<int, string>"), 2);
        // A nested generic is one argument, not two.
        assert_eq!(arity("Signal<Result<int, Error>>"), 1);
        assert_eq!(arity("Signal<fn(int, int) -> int>"), 1);
        assert_eq!(arity("Signal<[int]>"), 1);
    }

    #[test]
    fn array_and_function_arguments_render_by_shape() {
        let mut array = named("[]");
        array.is_array = true;
        array.array_element = Some(Box::new(named("int")));
        assert_eq!(instantiated("Signal", &[array]), "Signal<[int]>");
        let mut function = named("fn");
        function.is_function = true;
        function.parameter_types = vec![named("int")];
        function.return_type = Some(Box::new(named("string")));
        assert_eq!(
            instantiated("Signal", &[function]),
            "Signal<fn(int) -> string>"
        );
    }
}

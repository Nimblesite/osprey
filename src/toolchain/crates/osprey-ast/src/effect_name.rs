//! What an effect mention names: one declaration, many instantiations.
//!
//! `Signal<Count>` and `Signal<Cursor>` are DIFFERENT effects to a row and to a
//! handler — this distinction is necessary for a sound may-read dependency
//! set, and it is the reactive contract ([STAGE-SIGNALS-EXACT]). They
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

/// Whether a handler spelled `answered` answers a request spelled `request`:
/// the same spelling, or a request naming no instantiation — which asks for
/// inference ([EFFECTS-GENERIC-INSTANTIATION]) — against any instantiation of
/// its declaration. An explicitly instantiated request matches only its own
/// spelling, so `Stash<int>` is never answered by `Stash<string>`.
#[must_use]
pub fn covers(request: &str, answered: &str) -> bool {
    request == answered || (instantiation(request).is_none() && base(answered) == request)
}

/// [`covers`] over `Effect.op` operation names: the operation must agree and
/// the effect mention must be covered.
#[must_use]
pub fn operation_covers(request: &str, answered: &str) -> bool {
    match (request.rsplit_once('.'), answered.rsplit_once('.')) {
        (Some((effect, operation)), Some((handled, supplied))) => {
            operation == supplied && covers(effect, handled)
        }
        _ => request == answered,
    }
}

/// How many arguments a mention instantiates its declaration at. Counted at
/// bracket depth zero, so `Signal<Result<int, Error>>` is ONE argument rather
/// than two — a nested generic is not a second signal.
#[must_use]
pub fn arity(effect: &str) -> usize {
    instantiation(effect).map_or(0, |arguments| split_arguments(arguments).len())
}

/// The comma-separated items of `list` at bracket depth zero, trimmed; an
/// empty list has none. `fn(int, int) -> int, bool` is TWO items.
#[must_use]
pub fn split_arguments(list: &str) -> Vec<&str> {
    if list.trim().is_empty() {
        return Vec::new();
    }
    let mut depth = 0_i32;
    let mut start = 0_usize;
    let mut items = Vec::new();
    for (index, c) in list.char_indices() {
        if separates(c, list[..index].ends_with('-'), &mut depth) {
            items.push(list[start..index].trim());
            start = index + 1;
        }
    }
    items.push(list[start..].trim());
    items
}

/// The parameter types an operation signature `fn(P1, P2, …) -> R` declares,
/// as written, one text per parameter. A signature with no parameter list
/// declares none.
#[must_use]
pub fn signature_parameters(signature: &str) -> Vec<&str> {
    let Some((_, rest)) = signature.split_once('(') else {
        return Vec::new();
    };
    let mut depth = 1_i32;
    let close = rest
        .char_indices()
        .find(|(index, c)| {
            let _ = separates(*c, rest[..*index].ends_with('-'), &mut depth);
            depth == 0
        })
        .map(|(index, _)| index);
    close.map_or_else(Vec::new, |close| split_arguments(&rest[..close]))
}

/// Whether `c` separates two top-level arguments, advancing `depth` past any
/// bracket it opens or closes.
fn separates(c: char, follows_dash: bool, depth: &mut i32) -> bool {
    match c {
        '<' | '[' | '(' | '{' => *depth += 1,
        '>' if follows_dash => {}
        '>' | ']' | ')' | '}' => *depth -= 1,
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
    fn a_bare_request_is_covered_by_any_instantiation_of_its_declaration() {
        assert!(covers("Tensor", "Tensor<List<float>>"));
        assert!(covers("Tensor<List<float>>", "Tensor<List<float>>"));
        assert!(!covers("Tensor<int>", "Tensor<List<float>>"));
        assert!(!covers("Tensor", "Tensors<int>"));
        assert!(operation_covers("Tensor.get", "Tensor<List<float>>.get"));
        assert!(!operation_covers("Tensor.get", "Tensor<List<float>>.put"));
        assert!(!operation_covers("Tensor<int>.get", "Tensor<string>.get"));
    }

    #[test]
    fn arity_counts_top_level_arguments_only() {
        assert_eq!(arity("Log"), 0);
        assert_eq!(arity("Signal<Count>"), 1);
        assert_eq!(arity("Pair<int, string>"), 2);
        // A nested generic is one argument, not two.
        assert_eq!(arity("Signal<Result<int, Error>>"), 1);
        assert_eq!(arity("Signal<fn(int, int) -> int>"), 1);
        assert_eq!(arity("Pair<fn(int) -> int, bool>"), 2);
        assert_eq!(arity("Signal<[int]>"), 1);
        assert_eq!(arity("Signal<{ x: int, y: int }>"), 1);
    }

    #[test]
    fn a_signature_lists_its_written_parameter_types() {
        assert_eq!(
            signature_parameters("fn(B, B, fn(float, float) -> float) -> B"),
            vec!["B", "B", "fn(float, float) -> float"]
        );
        assert_eq!(signature_parameters("fn() -> string"), Vec::<&str>::new());
        assert_eq!(signature_parameters("string"), Vec::<&str>::new());
        assert_eq!(
            split_arguments("Result<int, Error>, [int]"),
            vec!["Result<int, Error>", "[int]"]
        );
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

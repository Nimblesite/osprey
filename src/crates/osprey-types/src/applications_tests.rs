//! The published site types of a `let`-generalized initializer resolve
//! through the one instantiation every later use of the binding agrees on
//! ([TYPE-GENERICS-APPLY], docs/specs/language/0004-TypeSystem.md).
//!
//! A `let` quantifies its unconstrained variables, so a lambda inside the
//! initializer keeps the quantified variable in its recorded type even though
//! `--check` settled it: the pinning argument belongs to a LATER call that
//! consumes the binding. Without the alias resolution, the backend saw a
//! still-generic closure and rejected a program the checker had accepted.

use crate::infer_program;
use osprey_syntax::parse_program;

/// One sidebar shape: `entries` generalizes over the row's action type and
/// only `rows`'s action argument settles it, after the lambda was inferred.
const SIDEBAR: &str = "\
type Action = Navigate
type Node<A> = Text { content: string }
type Row<A> = { rowKey: string, cell: Node<A> }
fn step<T, U>(out, items, f, index) = match listGet(items, index) {
    Success { item } => listAppend(out, f(item))
    Error => out
}
fn mapOver<T, U>(items, f) = range(0, length(items)) |> fold([], fn(out, index) => step(out, items, f, index))
fn label<A>(text) = Text { content: text }
fn entry<A>(key, cell) = Row { rowKey: key, cell: cell }
fn rows<A>(entries: List<Row<A>>, picked: A, extra: int) -> Node<A> = Text { content: \"${length(entries)}:${extra}\" }
fn view() = {
    let entries = mapOver([\"startup\", \"files\"], fn(key) => entry(key, label(key)))
    rows(entries, Navigate, 7)
}
fn main() = match view() { Text { content } => print(content) }
";

/// The lambda at 16:50 (`fn(key) => entry(key, label(key))`) and its `let`.
const LAMBDA_SITE: (u32, u32) = (13, 48);
const LET_SITE: (u32, u32) = (13, 4);

fn published(src: &str) -> crate::info::ProgramTypes {
    let parsed = parse_program(src);
    assert!(parsed.errors.is_empty(), "parse: {:?}", parsed.errors);
    infer_program(&parsed.program)
}

fn site_types(table: &crate::info::SiteTable, site: (u32, u32)) -> Vec<String> {
    table
        .iter()
        .filter(|((line, column), _)| (*line, *column) == site)
        .map(|(_, ty)| ty.to_string())
        .collect()
}

#[test]
fn a_lambda_in_a_let_initializer_publishes_its_only_use_s_instantiation() {
    let types = published(SIDEBAR);
    assert_eq!(
        site_types(&types.lambdas, LAMBDA_SITE),
        ["(string) -> { cell: Node<Action>, rowKey: string }"],
        "the closure's ABI must name the action type its only use pinned"
    );
    assert_eq!(
        site_types(&types.lets, LET_SITE),
        ["List<{ cell: Node<Action>, rowKey: string }>"],
        "the binding's published type carries the same resolution"
    );
}

#[test]
fn disagreeing_uses_leave_the_lambda_s_variable_open() {
    // `view(pick)` is generic, so the SAME lambda position serves two row
    // types; no single instantiation may be published for it.
    let two = SIDEBAR
        .replace("fn view() = {", "fn view(pick) = {")
        .replace("rows(entries, Navigate, 7)", "rows(entries, pick, 7)")
        .replace(
            "fn main() = match view() { Text { content } => print(content) }",
            "fn main() = {\n    let a = match view(Navigate) { Text { content } => content }\n    let b = match view(Restart) { Text { content } => content }\n    print(\"${a} ${b}\")\n}",
        );
    let types = published(&format!("type Reset = Restart\n{two}"));
    // The prepended `type Reset` shifts the sidebar down one line.
    let recorded = site_types(&types.lambdas, (LAMBDA_SITE.0 + 1, LAMBDA_SITE.1));
    assert_eq!(recorded.len(), 1, "one lambda site: {recorded:?}");
    let open = types.lambdas.iter().any(|((line, column), ty)| {
        (*line, *column) == (LAMBDA_SITE.0 + 1, LAMBDA_SITE.1) && crate::has_type_var(ty)
    });
    assert!(
        open,
        "two instantiations must not collapse to one: {recorded:?}"
    );
}

/// One shared lambda used directly at `bool` and through the generic
/// `firstOf` at `string` and `int`: the generic use is a different type at
/// each of its own instantiations, so it can never agree with the concrete
/// one, and pinning the lambda to `bool` would miscompile the other two.
#[test]
fn a_use_inside_a_generic_function_never_pins_the_lambda() {
    let types = published(
        "let boxIt = fn(x) => [x, x]\n\
         fn firstOf(v) = listGet(boxIt(v), 0)\n\
         fn main() = {\n\
             let a = firstOf(\"text\") ?: \"none\"\n\
             let b = firstOf(7) ?: 0\n\
             print(\"${a} ${toString(b)} ${toString(listLength(boxIt(true)))}\")\n\
         }\n",
    );
    let lambda = types
        .lambdas
        .iter()
        .find(|((line, _), _)| *line == 1)
        .map(|(_, ty)| ty.clone());
    assert!(
        lambda.as_ref().is_some_and(crate::has_type_var),
        "the shared lambda must stay generic: {lambda:?}"
    );
}

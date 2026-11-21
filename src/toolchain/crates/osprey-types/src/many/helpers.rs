//! The declarations a lowered program runs on, written as Osprey source: the
//! step type (finished, or one request carrying its continuation), sequencing,
//! finalization, re-entry of direct handlers, and one answering function per
//! captured effect.
//!
//! A continuation is the `k` field of a request: an ordinary closure. `drop`
//! runs the finalizers of a continuation nobody resumes
//! ([EFFECTS-FINALIZATION], [MULTI-COST-ABORT]).

use super::build::{handle_name, request_name, HELPER_LINE_BASE};
use super::facts::Facts;
use super::table::{Effect, Op, Table};
use crate::TypeError;
use osprey_ast::Program;
use std::fmt::Write as _;

/// Declarations every lowered program shares. `__manyRun` reads the value of
/// a step the source proof shows is finished; were a request ever left in one,
/// `Many__Open.request` has no handler, so it stops at the backend's own
/// unhandled-request guard instead of inventing a value.
const FIXED: &str = "effect Many__Open<a> {
    request: fn() -> a
}

effect Many__Mark {
    replayable settle: fn() -> Unit
    replayable settled: fn() -> bool
}

fn __manyUnit() = {
    let nothing = 0
}

fn __manyBoth(first, second) = {
    let ignored = first()
    second()
}

fn __manySettling(again, x) = {
    perform Many__Mark.settle()
    again(x)
}

fn __manyDropUnsettled(drop) = match perform Many__Mark.settled() {
    true => 0
    false => {
        perform Many__Mark.settle()
        drop()
    }
}

fn __manyAnswer(arm, again, drop) = {
    mut settled = false
    let install = fn(thunk) => {
        handle Many__Mark {
            settle => { settled = true }
            settled => settled
        }
        thunk()
    }
    __manyFinally(
        __manyReinstall(install, fn() => arm(fn(x) => __manySettling(again, x))),
        fn() => install(fn() => __manyDropUnsettled(drop)))
}

fn __manyRun(step) = match step {
    Many__Done { value } => value
    _ => perform Many__Open.request()
}
";

/// A captured effect with type parameters has no single request layout yet.
pub(super) fn generic_errors(table: &Table<'_>, facts: &Facts) -> Vec<TypeError> {
    captured(table, facts)
        .filter(|(_, effect)| effect.generic)
        .map(|(name, effect)| {
            TypeError::new(format!(
                "effect `{name}` has type parameters and its continuation is captured by a `many` \
region; a captured effect needs one request layout, so declare `{name}` without type parameters"
            ))
            .with_pos(effect.position)
        })
        .collect()
}

fn captured<'t>(
    table: &'t Table<'_>,
    facts: &'t Facts,
) -> impl Iterator<Item = (&'t String, &'t Effect)> {
    table
        .effects
        .iter()
        .filter(|(name, _)| facts.captured.contains(*name))
}

/// Put the helper declarations in front of the lowered program.
pub(super) fn attach(program: &mut Program, table: &Table<'_>, facts: &Facts) {
    let mut helpers = osprey_syntax::parse_program(&source(table, facts)).program;
    osprey_ast::span::offset_program(&mut helpers, HELPER_LINE_BASE);
    helpers.statements.append(&mut program.statements);
    program.statements = helpers.statements;
}

/// One request variant: its constructor name and payload field names.
struct Request {
    ctor: String,
    fields: Vec<String>,
}

impl Request {
    fn of(effect: &str, op: &Op) -> Self {
        Self {
            ctor: format!("Many__Ask_{effect}_{}", op.name),
            fields: (0..op.params.len()).map(|i| format!("a{i}")).collect(),
        }
    }

    /// `Ctor { a0, a1, k, drop }`.
    fn pattern(&self) -> String {
        let mut names = self.fields.clone();
        names.extend(["k".to_owned(), "drop".to_owned()]);
        format!("{} {{ {} }}", self.ctor, names.join(", "))
    }

    /// `Ctor { a0: a0, k: <k>, drop: <drop> }`.
    fn rebuilt(&self, k: &str, drop: &str) -> String {
        let mut fields: Vec<String> = self.fields.iter().map(|f| format!("{f}: {f}")).collect();
        fields.extend([format!("k: {k}"), format!("drop: {drop}")]);
        format!("{} {{ {} }}", self.ctor, fields.join(", "))
    }
}

fn requests(table: &Table<'_>, facts: &Facts) -> Vec<(String, Request)> {
    captured(table, facts)
        .flat_map(|(name, effect)| {
            effect
                .ops
                .iter()
                .map(move |op| (name.clone(), Request::of(name, op)))
        })
        .collect()
}

/// The helper source for this program's captured effects.
pub(super) fn source(table: &Table<'_>, facts: &Facts) -> String {
    let all = requests(table, facts);
    let mut out = String::from(FIXED);
    step_type(&mut out, table, facts);
    forwarding(
        &mut out,
        "__manyBind(step, next)",
        "step",
        "next(value)",
        &all,
        |request| request.rebuilt("fn(x) => __manyBind(k(x), next)", "drop"),
    );
    forwarding(
        &mut out,
        "__manyFinally(step, fin)",
        "step",
        "{\n        let ignored = fin()\n        Many__Done { value: value }\n    }",
        &all,
        |request| {
            request.rebuilt(
                "fn(x) => __manyFinally(k(x), fin)",
                "fn() => __manyBoth(drop, fin)",
            )
        },
    );
    forwarding(
        &mut out,
        "__manyReinstall(install, thunk)",
        "install(thunk)",
        "Many__Done { value: value }",
        &all,
        |request| request.rebuilt("fn(x) => __manyReinstall(install, fn() => k(x))", "drop"),
    );
    for (name, effect) in captured(table, facts) {
        requesters(&mut out, name, effect);
        answerer(&mut out, name, effect, &all);
    }
    out
}

fn step_type(out: &mut String, table: &Table<'_>, facts: &Facts) {
    out.push_str("\ntype Many__Step<a> = Many__Done { value: a }\n");
    for (name, effect) in captured(table, facts) {
        for op in &effect.ops {
            let request = Request::of(name, op);
            let mut fields: Vec<String> = request
                .fields
                .iter()
                .zip(&op.params)
                .map(|(field, ty)| format!("{field}: {ty}"))
                .collect();
            fields.push(format!("k: fn({}) -> Many__Step<a>", op.result));
            fields.push("drop: fn() -> int".to_owned());
            let _ = writeln!(out, "    | {} {{ {} }}", request.ctor, fields.join(", "));
        }
    }
}

/// A function that passes a finished step to `finished` and re-wraps every
/// request so the same function meets the continuation's next step.
fn forwarding(
    out: &mut String,
    signature: &str,
    scrutinee: &str,
    finished: &str,
    all: &[(String, Request)],
    rewrap: impl Fn(&Request) -> String,
) {
    let _ = writeln!(out, "\nfn {signature} = match {scrutinee} {{");
    let _ = writeln!(out, "    Many__Done {{ value }} => {finished}");
    for (_, request) in all {
        let _ = writeln!(out, "    {} => {}", request.pattern(), rewrap(request));
    }
    out.push_str("}\n");
}

fn requesters(out: &mut String, name: &str, effect: &Effect) {
    for op in &effect.ops {
        let request = Request::of(name, op);
        let _ = writeln!(
            out,
            "\nfn {}({}) = {}",
            request_name(name, &op.name),
            request.fields.join(", "),
            request.rebuilt("fn(x) => Many__Done { value: x }", "fn() => 0")
        );
    }
}

fn answerer(out: &mut String, name: &str, effect: &Effect, all: &[(String, Request)]) {
    let arms: Vec<String> = effect
        .ops
        .iter()
        .map(|op| format!("arm_{}", op.name))
        .collect();
    let handle = handle_name(name);
    let again = format!("{handle}(k(x), ret, {})", arms.join(", "));
    let _ = writeln!(
        out,
        "\nfn {handle}(step, ret, {}) = match step {{",
        arms.join(", ")
    );
    out.push_str("    Many__Done { value } => ret(value)\n");
    for (owner, request) in all {
        let own = effect
            .ops
            .iter()
            .find(|op| owner == name && Request::of(name, op).ctor == request.ctor);
        let answer = match own {
            Some(op) => {
                let mut given = request.fields.clone();
                given.push("again".to_owned());
                format!(
                    "__manyAnswer(fn(again) => arm_{}({}), fn(x) => {again}, drop)",
                    op.name,
                    given.join(", ")
                )
            }
            None => request.rebuilt(&format!("fn(x) => {again}"), "drop"),
        };
        let _ = writeln!(out, "    {} => {answer}", request.pattern());
    }
    out.push_str("}\n");
}

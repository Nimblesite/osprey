//! File-scope initialization order. Implements [MODULES-FILE-SCOPE-BINDING].
//!
//! A file-scope `let`/`mut` is a declaration whose initializer runs where it is
//! written, and the backend gives it a module global so a function body can
//! read it. Name resolution already rejects a function that mentions a binding
//! declared below it, which leaves exactly one hole: a top-level statement that
//! runs *before* the initializer and calls a function declared *after* it. That
//! read would observe the global's zero instead of the bound value, so it is
//! rejected here rather than compiled into a silently wrong answer.
//!
//! Reachability is by name and deliberately conservative: naming a function is
//! treated as calling it, because a name passed to a higher-order function is
//! called at a moment this pass cannot see.

use crate::TypeError;
use osprey_ast::freevars::free_idents;
use osprey_ast::{Expr, Program, Stmt};
use std::collections::{BTreeMap, BTreeSet};

/// Every file-scope diagnostic: entry conflicts, then reads that precede their
/// own initializer.
pub(crate) fn check(program: &Program) -> Vec<TypeError> {
    let mut errors = entry_conflicts(program);
    let bindings = file_scope_bindings(program);
    if !bindings.is_empty() {
        let functions = top_level_functions(program);
        let reads = resolve_reads(&functions, &bindings);
        errors.extend(rebound_and_read(program, &reads));
        errors.extend(early_reads(program, &bindings, &reads));
    }
    errors
}

/// A file-scope name may be rebound — later statements simply see the newer
/// binding. A function body reading it cannot: both bindings would share one
/// module global, and the second type would be read back through the first's
/// storage. Rejected rather than silently truncated.
fn rebound_and_read(
    program: &Program,
    reads: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<TypeError> {
    let read_anywhere: BTreeSet<&String> = reads.values().flatten().collect();
    let mut seen = BTreeSet::new();
    program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Let { name, .. } if !seen.insert(name.clone()) => Some(name),
            _ => None,
        })
        .filter(|name| read_anywhere.contains(name))
        .map(|name| {
            TypeError::new(format!(
                "file-scope `{name}` is rebound and also read from a function body; both \
                 bindings would share one module slot, so rename one of them"
            ))
        })
        .collect()
}

/// Name → index of the statement that initializes it. A rebinding of the same
/// name keeps the FIRST index: from there on the slot holds a bound value.
fn file_scope_bindings(program: &Program) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (index, statement) in program.statements.iter().enumerate() {
        if let Stmt::Let { name, .. } = statement {
            let _ = out.entry(name.clone()).or_insert(index);
        }
    }
    out
}

/// Every top-level function's free identifiers, with its own parameters
/// subtracted — the raw edges the read fixed point closes over.
fn top_level_functions(program: &Program) -> BTreeMap<String, BTreeSet<String>> {
    program
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::Function {
                name,
                parameters,
                body,
                ..
            } => {
                let mut free = idents_of(body);
                for parameter in parameters {
                    let _ = free.remove(&parameter.name);
                }
                Some((name.clone(), free))
            }
            _ => None,
        })
        .collect()
}

fn idents_of(expr: &Expr) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    free_idents(expr, &mut out);
    out
}

/// Least fixed point of "which file-scope bindings can this function read",
/// closed over the call graph. Terminates because each round can only add
/// names and the binding set is finite.
fn resolve_reads(
    functions: &BTreeMap<String, BTreeSet<String>>,
    bindings: &BTreeMap<String, usize>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut reads: BTreeMap<String, BTreeSet<String>> = functions
        .iter()
        .map(|(name, free)| (name.clone(), named_bindings(free, bindings)))
        .collect();
    loop {
        let mut grew = false;
        for (name, free) in functions {
            let inherited: BTreeSet<String> = free
                .iter()
                .filter(|called| *called != name)
                .filter_map(|called| reads.get(called))
                .flatten()
                .cloned()
                .collect();
            if let Some(own) = reads.get_mut(name) {
                let before = own.len();
                own.extend(inherited);
                grew |= own.len() != before;
            }
        }
        if !grew {
            return reads;
        }
    }
}

/// Narrow a free-identifier set to the file-scope bindings it names.
fn named_bindings(free: &BTreeSet<String>, bindings: &BTreeMap<String, usize>) -> BTreeSet<String> {
    free.iter()
        .filter(|name| bindings.contains_key(*name))
        .cloned()
        .collect()
}

/// A top-level statement may only reach bindings initialized above it. The
/// direct case (`print(x)` above `let x`) is already an unknown identifier, so
/// only the indirect one — reached through a function — is reported here.
fn early_reads(
    program: &Program,
    bindings: &BTreeMap<String, usize>,
    reads: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<TypeError> {
    let mut reported = BTreeSet::new();
    for (index, statement) in program.statements.iter().enumerate() {
        let Some(value) = executable_value(statement) else {
            continue;
        };
        for callee in idents_of(value) {
            let reachable = reads.get(&callee).into_iter().flatten();
            reported.extend(
                reachable
                    .filter(|binding| bindings.get(*binding).is_some_and(|at| *at > index))
                    .map(|binding| early_read_message(binding, &callee)),
            );
        }
    }
    reported.into_iter().map(TypeError::new).collect()
}

fn early_read_message(binding: &str, callee: &str) -> String {
    format!(
        "`{binding}` is read through `{callee}` before its own initializer runs; \
         move this statement below the `{binding}` binding"
    )
}

/// The expression a top-level statement evaluates, or `None` for a declaration
/// that evaluates nothing at this position.
fn executable_value(statement: &Stmt) -> Option<&Expr> {
    match statement {
        Stmt::Let { value, .. } | Stmt::Assignment { value, .. } | Stmt::Expr { value, .. } => {
            Some(value)
        }
        _ => None,
    }
}

/// `main` is the program entry [MODULES-ENTRYPOINT]. Top-level executable
/// statements are the *alternative* entry, so a source carrying both declares
/// two — which the backend used to resolve by silently discarding one.
fn entry_conflicts(program: &Program) -> Vec<TypeError> {
    let has_main = program
        .statements
        .iter()
        .any(|statement| matches!(statement, Stmt::Function { name, .. } if name == "main"));
    if !has_main {
        return Vec::new();
    }
    let executable = program
        .statements
        .iter()
        .any(|statement| matches!(statement, Stmt::Assignment { .. } | Stmt::Expr { .. }));
    if executable {
        vec![TypeError::new(
            "a top-level executable statement cannot sit beside `main`: `main` is the \
             program entry, so move the statement into it (a file-scope `let` or `mut` \
             is a declaration and stays where it is)",
        )]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use crate::testutil::{bad, ok};

    fn message(src: &str) -> String {
        bad(src)
            .iter()
            .map(|error| error.message.clone())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_function_may_read_a_binding_declared_above_it() {
        ok("let plain = 7\nfn readIt() = plain\nprint(\"${readIt()}\")\n");
    }

    #[test]
    fn a_statement_may_not_reach_a_later_binding_through_a_function() {
        assert!(
            message("fn early() = late()\nprint(\"${early()}\")\nlet x = 7\nfn late() = x\n")
                .contains("`x` is read through `early` before its own initializer runs")
        );
    }

    #[test]
    fn the_call_graph_is_followed_to_its_end() {
        assert!(message(
            "fn a() = b()\nfn b() = c()\nprint(\"${a()}\")\nlet deep = 1\nfn c() = deep\n"
        )
        .contains("`deep` is read through `a`"));
    }

    #[test]
    fn naming_a_function_counts_as_calling_it() {
        assert!(message(
            "fn apply(f) = f()\nprint(\"${apply(late)}\")\nlet x = 7\nfn late() = x\n"
        )
        .contains("`x` is read through `late`"));
    }

    #[test]
    fn a_statement_below_the_binding_may_reach_it() {
        ok("let x = 7\nfn late() = x\nprint(\"${late()}\")\n");
    }

    #[test]
    fn a_parameter_shadows_a_file_scope_binding_of_the_same_name() {
        ok("fn shadow(x) = x\nprint(\"${shadow(1)}\")\nlet x = 7\nfn reader() = x\n");
    }

    #[test]
    fn a_call_cycle_does_not_stall_the_read_fixed_point() {
        ok("let x = 7\nfn a() = b()\nfn b() = (a() + x) ?: x\nprint(\"${b()}\")\n");
    }

    #[test]
    fn a_rebound_file_scope_name_may_not_also_be_read_by_a_function() {
        assert!(
            message("let x = 1\nfn readX() = x\nlet x = 2\nprint(\"${readX()}\")\n")
                .contains("file-scope `x` is rebound and also read from a function body")
        );
    }

    #[test]
    fn a_rebound_name_no_function_reads_stays_legal() {
        ok("let x = 1\nprint(\"${x}\")\nlet x = 2\nprint(\"${x}\")\n");
    }

    #[test]
    fn main_may_not_share_a_source_with_an_executable_statement() {
        assert!(message("print(\"top\")\nfn main() = print(\"main\")\n")
            .contains("cannot sit beside `main`"));
    }

    #[test]
    fn a_file_scope_binding_beside_main_is_a_declaration_not_a_statement() {
        ok("let plain = 7\nfn readIt() = plain\nfn main() = print(\"${readIt()}\")\n");
    }
}

//! Which functions lie on a reference cycle.
use super::recursive_functions;
use crate::effect_rows::Index;
use std::collections::BTreeSet;

fn cycles_by_name(source: &str) -> BTreeSet<String> {
    let parsed = osprey_syntax::parse_program(source);
    assert_eq!(parsed.errors, []);
    let index = Index::collect(&parsed.program);
    let recursive = recursive_functions(&index);
    let named = index.functions.iter().enumerate();
    named
        .filter(|(id, _)| recursive.contains(id))
        .map(|(_, function)| function.name.clone())
        .collect()
}

#[test]
fn acyclic_chains_have_no_cycle() {
    let cycles = cycles_by_name("fn a()=b()\nfn b()=c()\nfn c()=7\n");
    assert_eq!(cycles, BTreeSet::new());
}

#[test]
fn self_recursion_is_a_cycle() {
    let cycles =
        cycles_by_name("fn count(n)=if n==0 { 0 } else { count(n-1) }\nfn caller()=count(3)\n");
    assert_eq!(cycles, BTreeSet::from(["count".to_owned()]));
}

#[test]
fn mutual_recursion_marks_the_whole_cycle_but_not_its_callers() {
    let cycles = cycles_by_name(
        "fn even(n)=if n==0 { true } else { odd(n-1) }\n\
         fn odd(n)=if n==0 { false } else { middle(n) }\n\
         fn middle(n)=even(n-1)\nfn entry()=even(4)\nfn leaf()=1\n",
    );
    let expected = ["even", "middle", "odd"].map(str::to_owned);
    assert_eq!(cycles, BTreeSet::from(expected));
}

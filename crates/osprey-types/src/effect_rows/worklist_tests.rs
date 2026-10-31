//! A sweep re-analyses exactly the functions whose inputs moved.
use super::{affected, record_read, recording};
use crate::effect_rows::{Analyzer, CallableEnv, Index, Instances, Summary};
use std::collections::BTreeSet;

fn set(ids: &[usize]) -> BTreeSet<usize> {
    ids.iter().copied().collect()
}

#[test]
fn recording_collects_reads_and_restores_the_enclosing_recording() {
    let ((), outer) = recording(|| {
        record_read(1);
        let ((), inner) = recording(|| record_read(2));
        assert_eq!(inner, set(&[2]));
        record_read(3);
    });
    assert_eq!(outer, set(&[1, 3]));
    record_read(4);
}

#[test]
fn only_moved_functions_and_their_readers_are_reanalysed() {
    let reads = [set(&[1]), set(&[]), set(&[2]), set(&[0, 1])];
    assert_eq!(affected(&set(&[1]), &set(&[]), &reads), set(&[0, 1, 3]));
    assert_eq!(affected(&set(&[2]), &set(&[]), &reads), set(&[2]));
    assert_eq!(affected(&set(&[]), &set(&[]), &reads), set(&[]));
}

#[test]
fn a_moved_function_the_file_scope_read_reanalyses_everything() {
    let reads = [set(&[]), set(&[]), set(&[])];
    assert_eq!(affected(&set(&[1]), &set(&[1]), &reads), set(&[0, 1, 2]));
    assert_eq!(affected(&set(&[1]), &set(&[2]), &reads), set(&[1]));
}

#[test]
fn naming_a_function_records_its_read() {
    let parsed = osprey_syntax::parse_program("fn leaf()=7\nfn caller()=leaf()\n");
    let index = Index::collect(&parsed.program);
    let position = |name: &str| {
        let found = index
            .functions
            .iter()
            .position(|function| function.name == name);
        found.unwrap_or_else(|| panic!("fixture declares {name}"))
    };
    let (leaf, caller) = (position("leaf"), position("caller"));
    let count = index.functions.len();
    let (rows, returns) = (vec![Summary::default(); count], vec![None; count]);
    let instances = Instances::default();
    let analyzer = Analyzer {
        index: &index,
        rows: &rows,
        returns: &returns,
        instances: &instances,
        file_scope: CallableEnv::default(),
    };
    let caller_body = index.functions.get(caller).map(|function| function.body);
    let body = caller_body.unwrap_or_else(|| panic!("caller has a body"));
    let read = recording(|| analyzer.expression(body, &[], &CallableEnv::default())).1;
    assert!(read.contains(&leaf), "caller's analysis read {read:?}");
}

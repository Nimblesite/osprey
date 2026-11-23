//! A sweep re-analyses exactly the functions whose inputs moved.
use super::{affected, invalidate_scope, record_read, recording};
use crate::effect_rows::expr_args::{assert_metadata_written, assert_model_metadata_matches};
use crate::effect_rows::field_rows::tests::analyzer as empty_analyzer;
use crate::effect_rows::recursion::Callees;
use crate::effect_rows::solver::{
    advance_function, clear_verdicts, converge, file_scope_env, ReturnHistory,
};
use crate::effect_rows::{
    Analyzer, Callable, CallableEnv, Index, Instances, Requirement, Summary, Value,
};
use osprey_ast::Stmt;
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
    assert_eq!(affected(&set(&[1]), &reads), set(&[0, 1, 3]));
    assert_eq!(affected(&set(&[2]), &reads), set(&[2]));
    assert_eq!(affected(&set(&[]), &reads), set(&[]));
}

#[test]
fn naming_a_function_records_its_read() {
    let parsed = osprey_syntax::parse_program("fn leaf()=7\nfn caller()=leaf()\n");
    let index = Index::collect(&parsed.program);
    let (leaf, caller) = (function_id(&index, "leaf"), function_id(&index, "caller"));
    let count = index.functions.len();
    let state = (vec![Summary::default(); count], vec![None; count]);
    let instances = Instances::default();
    let analyzer = context(&index, &instances, &state, CallableEnv::default());
    let caller_body = index.functions.get(caller).map(|function| function.body);
    let body = caller_body.unwrap_or_else(|| panic!("caller has a body"));
    let read = recording(|| analyzer.expression(body, &[], &CallableEnv::default())).1;
    assert!(read.contains(&leaf), "caller's analysis read {read:?}");
}

#[test]
fn file_scope_dependencies_ignore_calls_that_do_not_bind_values() {
    with_program("fn leaf()=7\nleaf()\n", |index, statements| {
        let count = index.functions.len();
        let state = (vec![Summary::default(); count], vec![None; count]);
        let instances = Instances::default();
        let (env, reads) =
            recording(|| file_scope_env(index, &instances, statements, &state.0, &state.1));
        assert!(env.values.is_empty());
        assert!(
            reads.is_empty(),
            "effect-only calls invalidate every function: {reads:?}"
        );
    });
}

#[test]
fn acyclic_rows_replace_superseded_callback_arguments() {
    with_program("fn apply(callback)=callback(7)\n", |index, _| {
        let id = function_id(index, "apply");
        let count = index.functions.len();
        let mut state = (vec![Summary::default(); count], vec![None; count]);
        let instances = Instances::default();
        let analyzer = context(index, &instances, &state, CallableEnv::default());
        let function = index
            .functions
            .get(id)
            .unwrap_or_else(|| panic!("apply exists"));
        let (mut actual, _) = analyzer.function_body_model(function);
        actual.widen();
        let mut historical = actual.clone();
        historical.parameter_uses = historical
            .parameter_uses
            .into_iter()
            .map(|mut use_| {
                use_.arguments.positional = vec![Some(Value::unknown_callable())];
                use_
            })
            .collect();
        state.0 = vec![historical];
        let snapshot = state.clone();
        let analyzer = context(index, &instances, &snapshot, CallableEnv::default());
        let mut history = ReturnHistory::default();
        let _ = advance_function(
            &analyzer,
            id,
            function,
            state.0.get_mut(id),
            state.1.get_mut(id),
            &mut history,
        );
        assert_eq!(
            state.0.get(id).map(|row| &row.parameter_uses),
            Some(&actual.parameter_uses)
        );
    });
}

type SolverState = (Vec<Summary>, Vec<Option<Value>>);
type SolverPass = fn(&Index<'_>, &Instances, &[Stmt], &mut SolverState);

fn context<'a>(
    index: &'a Index<'a>,
    instances: &'a Instances,
    state: &'a SolverState,
    file_scope: CallableEnv,
) -> Analyzer<'a> {
    Analyzer {
        rows: &state.0,
        returns: &state.1,
        file_scope,
        ..empty_analyzer(index, instances)
    }
}

const ALIAS_PROGRAM: &str = "effect Alarm {ring:fn()->int}\n\
type Control={press:fn()->int![]}\n\
fn leaf() ![] = {\nperform Alarm.ring()\n7\n}\n\
let alias=leaf\n\
fn main()={\n\
let deps=signalDependencies(fn()=>alias())\n\
let bad=Control{press:fn()=>perform Alarm.ring()}\n\
alias()\nhandle Alarm {ring=>7}\nperform Alarm.ring()\n}\n";

fn function_id(index: &Index<'_>, name: &str) -> usize {
    index
        .functions
        .iter()
        .position(|function| function.name == name)
        .unwrap_or_else(|| panic!("the fixture declares {name}"))
}

fn seeded_instances() -> Instances {
    let mut instances = Instances::default();
    let _ = instances.argument_types.insert(Vec::new(), Vec::new());
    instances
}

fn model(
    index: &Index<'_>,
    instances: &Instances,
    statements: &[Stmt],
    pass: SolverPass,
) -> SolverState {
    let count = index.functions.len();
    let mut state = (vec![Summary::default(); count], vec![None; count]);
    pass(index, instances, statements, &mut state);
    clear_verdicts(&mut state.0, &mut state.1);
    pass(index, instances, statements, &mut state);
    state
}

fn worklist_pass(
    index: &Index<'_>,
    instances: &Instances,
    statements: &[Stmt],
    state: &mut SolverState,
) {
    converge(index, instances, statements, &mut state.0, &mut state.1);
}

// Keep the old all-function Jacobi schedule as the independent reference;
// both schedules use the same transfer functions and return-history policy.
fn whole_pass(
    index: &Index<'_>,
    instances: &Instances,
    statements: &[Stmt],
    state: &mut SolverState,
) {
    let mut history = ReturnHistory::new(Callees::of(index).recursive());
    while reference_step(index, instances, statements, state, &mut history) {}
}

fn reference_step(
    index: &Index<'_>,
    instances: &Instances,
    statements: &[Stmt],
    state: &mut SolverState,
    history: &mut ReturnHistory,
) -> bool {
    let file_scope = file_scope_env(index, instances, statements, &state.0, &state.1);
    let analyzer = context(index, instances, state, file_scope);
    let mut next = (state.0.clone(), state.1.clone());
    let moved = advance_all(&analyzer, &mut next, history);
    *state = next;
    moved
}

fn advance_all(
    analyzer: &Analyzer<'_>,
    next: &mut SolverState,
    history: &mut ReturnHistory,
) -> bool {
    let mut moved = false;
    for (id, function) in analyzer.index.functions.iter().enumerate() {
        moved |= advance_function(
            analyzer,
            id,
            function,
            next.0.get_mut(id),
            next.1.get_mut(id),
            history,
        );
    }
    moved
}

fn requires_ring(summary: &Summary) -> bool {
    summary
        .required
        .contains(&Requirement::new("Alarm", "ring", Vec::new()))
}

fn assert_signal_written(instances: &Instances) {
    assert!(
        instances.signal_queries.borrow().values().any(|query| {
            query
                .operations
                .iter()
                .any(|operation| operation == "Alarm.ring")
        }),
        "the real alias query must retain its effect"
    );
}

fn compare_metadata(worklist: &Instances, whole: &Instances) {
    assert_signal_written(worklist);
    assert_signal_written(whole);
    assert_metadata_written(worklist);
    assert_metadata_written(whole);
    assert_model_metadata_matches(worklist, whole);
}

fn with_program<T>(source: &str, run: impl FnOnce(&Index<'_>, &[Stmt]) -> T) -> T {
    let parsed = osprey_syntax::parse_program(source);
    assert_eq!(parsed.errors, []);
    let index = Index::collect(&parsed.program);
    run(&index, &parsed.program.statements)
}

#[test]
fn ordinary_function_steps_model_each_argument_once() {
    let observed = [1, 2, 4, 8].map(ordinary_step_visits);
    assert_eq!(
        observed, [1; 4],
        "one value model per argument, not one per walk"
    );
}

#[test]
fn combined_function_models_preserve_real_metadata_and_dependency_reads() {
    let nested = format!(
        "fn sink(value)=value\n{}",
        ALIAS_PROGRAM.replace("alias()", "sink(sink(alias()))")
    );
    for source in [ALIAS_PROGRAM, nested.as_str()] {
        with_program(source, |index, statements| {
            let state = model(index, &seeded_instances(), statements, worklist_pass);
            let before = seeded_instances();
            let after = before.clone();
            compare_function_transfers(index, statements, &state, &before, &after);
            compare_metadata(&before, &after);
        });
    }
}

fn compare_function_transfers(
    index: &Index<'_>,
    statements: &[Stmt],
    state: &SolverState,
    before: &Instances,
    after: &Instances,
) {
    let before_env = file_scope_env(index, before, statements, &state.0, &state.1);
    let after_env = file_scope_env(index, after, statements, &state.0, &state.1);
    let before = context(index, before, state, before_env);
    let after = context(index, after, state, after_env);
    for function in &index.functions {
        let expected = recording(|| original_function_model(&before, function));
        let actual = recording(|| normalized_combined_model(&after, function));
        assert_eq!(actual, expected, "complete transfer for {}", function.name);
    }
}

fn normalized_combined_model(
    analyzer: &Analyzer<'_>,
    function: &crate::effect_rows::Function<'_>,
) -> (Summary, Option<Value>) {
    let (mut summary, returned) = analyzer.function_body_model(function);
    summary.widen();
    (summary, returned.map(|value| value.widened().widened()))
}

fn ordinary_step_visits(depth: usize) -> usize {
    let body = (0..depth).fold("value".to_owned(), |value, _| format!("sink({value})"));
    let source = format!("fn sink(value)=value\nfn caller(value)={body}\n");
    with_program(&source, |index, statements| {
        assert!(Callees::of(index).recursive().is_empty());
        let original = seeded_instances();
        let state = model(index, &original, statements, worklist_pass);
        let optimized = original.clone();
        let before = context(index, &original, &state, CallableEnv::default());
        let after = context(index, &optimized, &state, CallableEnv::default());
        assert_ordinary_step(&before, &after)
    })
}

fn assert_ordinary_step(before: &Analyzer<'_>, after: &Analyzer<'_>) -> usize {
    use crate::effect_rows::field_rows::tests::{measured_value_visits, nested_callback_leaf};
    let id = function_id(before.index, "caller");
    let function = before.index.functions.get(id).expect("fixture caller");
    let expected = recording(|| original_function_model(before, function));
    let leaf = nested_callback_leaf(function.body);
    let (actual, visits) = measured_value_visits(leaf, || {
        recording(|| advanced_function_model(after, id, function))
    });
    assert_eq!(actual, expected, "full row, return and dependency set");
    assert_eq!(actual.1, set(&[function_id(before.index, "sink")]));
    assert_model_metadata_matches(before.instances, after.instances);
    visits
}

fn original_function_model(
    analyzer: &Analyzer<'_>,
    function: &crate::effect_rows::Function<'_>,
) -> (Summary, Option<Value>) {
    let mut summary = analyzer.function_body(function);
    summary.widen();
    let returned = analyzer
        .function_return(function)
        .map(|value| value.widened().widened());
    (summary, returned)
}

fn advanced_function_model(
    analyzer: &Analyzer<'_>,
    id: usize,
    function: &crate::effect_rows::Function<'_>,
) -> (Summary, Option<Value>) {
    let mut next = (analyzer.rows.to_vec(), analyzer.returns.to_vec());
    let mut history = ReturnHistory::new(BTreeSet::new());
    let _ = advance_function(
        analyzer,
        id,
        function,
        next.0.get_mut(id),
        next.1.get_mut(id),
        &mut history,
    );
    (
        next.0.get(id).cloned().expect("fixture row"),
        next.1.get(id).cloned().expect("fixture return"),
    )
}

fn compare_models(source: &str, check: impl FnOnce(&Index<'_>, &SolverState)) {
    with_program(source, |index, statements| {
        let worklist = seeded_instances();
        let whole = worklist.clone();
        let actual = model(index, &worklist, statements, worklist_pass);
        let expected = model(index, &whole, statements, whole_pass);
        assert_eq!(
            actual, expected,
            "complete rows and returned values must match"
        );
        compare_metadata(&worklist, &whole);
        check(index, &actual);
    });
}

fn assert_alias_effect(index: &Index<'_>, state: &SolverState) {
    let leaf = function_id(index, "leaf");
    let main = function_id(index, "main");
    assert!(state.0.get(leaf).is_some_and(requires_ring));
    assert!(state.0.get(main).is_some_and(requires_ring));
    assert!(state.1.get(leaf).is_some_and(Option::is_some));
}

#[test]
fn worklist_matches_full_sweeps_with_actual_metadata_and_a_file_alias() {
    compare_models(ALIAS_PROGRAM, assert_alias_effect);
}

fn assert_callback_payload(index: &Index<'_>, state: &SolverState) {
    let parcel = function_id(index, "parcel");
    let callback = state
        .1
        .get(parcel)
        .and_then(Option::as_ref)
        .and_then(|value| value.fields.get("callback"))
        .and_then(|value| value.callable.as_ref());
    assert!(
        matches!(callback, Some(Callable::Known(known)) if requires_ring(&known.summary)),
        "the complete returned record must retain its effectful callback"
    );
}

#[test]
fn worklist_matches_full_sweeps_for_returned_callback_records() {
    let source = format!("{ALIAS_PROGRAM}fn parcel()={{callback:fn()=>perform Alarm.ring()}}\n");
    compare_models(&source, assert_callback_payload);
}

#[test]
fn worklist_matches_full_sweeps_for_mutually_recursive_callback_records() {
    let payload = "type Packet={next:fn()->Packet,callback:fn()->int}\n\
        fn parcel()=Packet{next:fn()=>relay(),callback:fn()=>perform Alarm.ring()}\n\
        fn relay()=parcel()\n";
    compare_models(
        &format!("{ALIAS_PROGRAM}{payload}"),
        assert_callback_payload,
    );
}

fn function_reads(analyzer: &Analyzer<'_>) -> Vec<BTreeSet<usize>> {
    analyzer
        .index
        .functions
        .iter()
        .map(|function| {
            recording(|| {
                let _ = analyzer.function_body(function);
                let _ = analyzer.function_return(function);
            })
            .1
        })
        .collect()
}

fn assert_alias_route(
    index: &Index<'_>,
    instances: &Instances,
    statements: &[Stmt],
    state: &SolverState,
) {
    let (file_scope, file_reads) =
        recording(|| file_scope_env(index, instances, statements, &state.0, &state.1));
    let leaf = function_id(index, "leaf");
    let analyzer = context(index, instances, state, file_scope);
    let reads = function_reads(&analyzer);
    assert_eq!(file_reads, set(&[leaf]));
    assert_eq!(reads, vec![BTreeSet::new(); index.functions.len()]);
    let count = reads.len();
    let initial = file_scope_env(
        index,
        instances,
        statements,
        &vec![Summary::default(); count],
        &vec![None; count],
    );
    assert_scope_invalidation(leaf, count, &initial, &analyzer.file_scope);
}

fn assert_scope_invalidation(
    leaf: usize,
    count: usize,
    initial: &CallableEnv,
    converged: &CallableEnv,
) {
    for previous in [None, Some(converged)] {
        let mut dirty = set(&[leaf]);
        invalidate_scope(previous, converged, &mut dirty, count);
        assert_eq!(dirty, set(&[leaf]));
    }
    let mut dirty = set(&[leaf]);
    invalidate_scope(Some(initial), converged, &mut dirty, count);
    assert_eq!(dirty, (0..count).collect());
}

#[test]
fn the_actual_file_alias_invalidates_all_of_its_readers() {
    with_program(ALIAS_PROGRAM, |index, statements| {
        let instances = seeded_instances();
        let state = model(index, &instances, statements, worklist_pass);
        assert_alias_route(index, &instances, statements, &state);
    });
}

#[test]
fn the_alias_oracle_retains_the_actual_rejection_diagnostics() {
    crate::effect_rows_tests::assert_rejected_with(
        ALIAS_PROGRAM,
        &["outside its declared row", "Alarm.ring"],
    );
}

/// `depth + 1` functions written callers first, each calling the next, the
/// last performing `Alarm.ring`: the order in which a sweep that models
/// every dirty function would model each caller before its callee settles.
fn chain_program(depth: usize) -> String {
    let calls = (0..depth)
        .map(|i| format!("fn f{i}() = f{}()\n", i + 1))
        .collect::<Vec<_>>()
        .concat();
    format!("effect Alarm {{ring:fn()->int}}\n{calls}fn f{depth}() = perform Alarm.ring()\n")
}

/// A caller waits for its callees, so each body is modelled a bounded number
/// of times per pass however deep the chain, and the fixed point is exactly
/// the whole-program schedule's.
#[test]
fn a_caller_waits_for_its_callees_and_reaches_the_same_fixed_point() {
    let mut counts = Vec::new();
    for depth in [1, 2, 4, 8, 16, 32] {
        with_program(&chain_program(depth), |index, statements| {
            let instances = Instances::default();
            crate::effect_rows::solver::BODY_MODELS.with(|models| models.set(0));
            let worklist = model(index, &instances, statements, worklist_pass);
            counts.push(crate::effect_rows::solver::BODY_MODELS.with(core::cell::Cell::get));
            let whole = model(index, &instances, statements, whole_pass);
            assert_eq!(worklist, whole, "depth {depth}: the fixed point moved");
            assert!(
                worklist.0.iter().all(requires_ring),
                "depth {depth}: every caller requires Alarm.ring"
            );
        });
    }
    // Three models per function across the structural and provenance passes;
    // modelling every dirty caller at once took 5 per function plus 2.
    assert_eq!(counts, [6, 9, 15, 27, 51, 99]);
}

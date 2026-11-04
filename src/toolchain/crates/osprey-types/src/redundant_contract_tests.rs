//! Exported signatures, original reader types and binding annotation contracts.
//! Implements [TYPE-ANNOTATION-REDUNDANT].

use crate::info::ProgramTypes;
use crate::redundant::redundant_annotation_sites_for;
use crate::ty::{Type, VarId};
use osprey_ast::{Position, Program};
use osprey_syntax::{parse_program_with_flavor, Flavor};

const TWO: &str = "fn published(name: string) -> string = \"hi \" + name\n\
    fn internal(name: string) -> string = {\n  let shout: string = name + \"!\"\n  shout\n}\n\
    fn api(label: string) -> string = {\n  let inner: string = label + \"?\"\n  inner\n}\n";

fn reported(contract: &dyn Fn(&str) -> bool) -> Vec<String> {
    reported_source(TWO, contract)
}

fn reported_source(source: &str, contract: &dyn Fn(&str) -> bool) -> Vec<String> {
    let program = checked(source);
    redundant_annotation_sites_for(&program, |_| true, contract)
        .into_iter()
        .map(|site| site.warning.message)
        .collect()
}

fn checked(source: &str) -> Program {
    let parsed = parse_program_with_flavor(source, Flavor::Default);
    assert_eq!(parsed.errors, []);
    assert_eq!(crate::check_program(&parsed.program), []);
    parsed.program
}

#[test]
fn original_reader_types_are_preserved_during_backend_specialization() -> Result<(), String> {
    let types = crate::infer_program(&checked("fn keep(value)={\nlet saved=value\nsaved\n}\n"));
    let position = Some(Position { line: 2, column: 0 });
    let id = match types.source_let_type(Some("keep"), position) {
        Some(Type::Var(id)) => *id,
        other => return Err(format!("expected generic binding: {other:?}")),
    };
    assert_original_reader_type(&types, position, id);
    Ok(())
}

fn assert_original_reader_type(types: &ProgramTypes, position: Option<Position>, id: VarId) {
    let integer = Type::int();
    let bindings = [(id, integer.clone())].into_iter().collect();
    let concrete = types.specialized(&bindings, Some("keep"));
    assert_eq!(concrete.let_type(Some("keep"), position), Some(&integer));
    assert_eq!(
        concrete.source_let_type(Some("keep"), position),
        Some(&Type::Var(id))
    );
    assert!(
        std::sync::Arc::ptr_eq(&types.source_lets, &concrete.source_lets),
        "backend specialization copied the original reader table"
    );
}

const PHANTOM: &str = "effect Read<T>{get:fn()->T}\ntype Control<T>={press:fn()->int ![Read<T>]}\nfn make()=Control {press:fn()=>1}\nfn needString(control:Control<string>)=0\n";

#[test]
fn phantom_binding_annotation_is_preserved_when_its_argument_is_not_inferred() {
    for binding in [
        "let c:Control<int>=make()",
        "let c:List<Control<int>>=[make()]",
    ] {
        let required = format!("{PHANTOM}{binding}\n");
        let messages = reported_source(&required, &|_| false);
        assert!(
            messages.iter().all(|message| !message.contains("`c`")),
            "{messages:?}"
        );
    }
    let inferred = format!("{PHANTOM}let c:Control<int>=Control<int> {{press:fn()=>1}}\n");
    let messages = reported_source(&inferred, &|_| false);
    assert!(
        messages.iter().any(|message| message.contains("`c`")),
        "{messages:?}"
    );
}

#[test]
fn phantom_binding_annotation_changes_which_later_calls_are_accepted() {
    let annotated = format!("{PHANTOM}let c:Control<int>=make()\nlet result=needString(c)\n");
    crate::effect_rows_tests::assert_rejected_with(&annotated, &["type mismatch"]);
    let inferred = format!("{PHANTOM}let c=make()\nlet result=needString(c)\n");
    crate::effect_rows_tests::assert_accepted(&inferred);
}

#[test]
fn concrete_record_binding_annotations_still_report_when_inferred() {
    for source in [
        "type Point={x:int}\nlet c:Point=Point{x:1}\n",
        "type Box<T>={value:T}\nlet c:Box<int>=Box{value:1}\n",
        "type Box<T>={value:T}\nlet c:List<Box<int>>=[Box{value:1}]\n",
        "type Point={x:int}\nlet c:List<Point>=[Point{x:1}]\n",
    ] {
        let messages = reported_source(source, &|_| false);
        assert!(
            messages.iter().any(|message| message.contains("`c`")),
            "{source}: {messages:?}"
        );
    }
}

#[test]
fn an_exported_members_signature_is_never_reported_but_its_body_still_is() {
    let all = reported(&|_| false);
    let contracted = reported(&|name| name == "published" || name == "api");
    assert!(all.iter().any(|m| m.contains("`published`")), "{all:?}");
    assert!(
        contracted.iter().all(|m| !m.contains("of `published`")
            && !m.contains("on `published`")
            && !m.contains("of `api`")
            && !m.contains("on `api`")),
        "{contracted:?}"
    );
    assert!(
        contracted.iter().any(|m| m.contains("`internal`")),
        "{contracted:?}"
    );
    assert!(
        contracted.iter().any(|m| m.contains("`inner`")),
        "{contracted:?}"
    );
}

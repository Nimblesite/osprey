//! Native regression for #184: full and partial curried calls retain effects.

use std::path::Path;
use std::process::{Command, Output};

#[path = "common/effect_execution.rs"]
mod effect_execution;

type ShadowType = (&'static str, &'static str, &'static str, &'static str);

const SHADOW_TYPES: [ShadowType; 4] = [
    ("List", "", "", ""),
    ("Map", "", "", ""),
    ("List<T>", ", T", ", _", ", 1"),
    ("Map<K,V>", ", K, V", ", _, _", ", 1, 2"),
];

#[test]
fn shadowed_collection_unions_retain_pure_and_handled_callbacks() {
    for (index, shape) in SHADOW_TYPES.into_iter().enumerate() {
        for handled in [false, true] {
            let (callback, invocation) = callback_parts(handled);
            let source = shadowed_callback_source(shape, callback, invocation);
            effect_execution::assert_flavored_output(
                &format!("shadowed_{index}_{handled}"),
                "osp",
                &source,
                "7\n",
            );
        }
        assert_shadowed_alarm_refused(index, shape);
    }
}

fn shadowed_callback_source(
    (name, fields, pattern, values): ShadowType,
    callback: &str,
    invocation: &str,
) -> String {
    format!("effect Alarm {{ ring: fn() -> int }}\ntype {name} = Hold(fn() -> int{fields})\nfn trigger(item) = match item {{ Hold(run{pattern}) => run() }}\nlet item = Hold({callback}{values})\nlet answer = {invocation}\nprint(answer)\n")
}

fn assert_shadowed_alarm_refused(index: usize, shape: ShadowType) {
    let (callback, _) = callback_parts(true);
    let source = shadowed_callback_source(shape, callback, "trigger(item)");
    for memory in ["gc", "arc"] {
        assert_refusal(
            effect_execution::execute(
                &format!("shadowed_{index}_unhandled"),
                "osp",
                &source,
                memory,
            ),
            "unhandled effect operations at program entry: Alarm.ring; add a matching `handle`",
        );
    }
}

fn callback_parts(handled: bool) -> (&'static str, &'static str) {
    if handled {
        (
            "fn()=>perform Alarm.ring()",
            "{\nhandle Alarm {ring=>7}\ntrigger(item)\n}",
        )
    } else {
        ("fn()=>7", "trigger(item)")
    }
}

#[test]
fn mutually_recursive_records_retain_callbacks() {
    for handled in [false, true] {
        let (callback, invocation) = callback_parts(handled);
        let source = format!("effect Alarm {{ ring: fn() -> int }}\ntype Chain = {{ call: fn() -> int, links: List<Link> }}\ntype Link = {{ item: Chain }}\nfn trigger(item) = item.item.call()\nlet item = Link {{ item: Chain {{ call: {callback}, links: [] }} }}\nlet answer = {invocation}\nprint(answer)\n");
        effect_execution::assert_flavored_output(
            &format!("recursive_callback_{handled}"),
            "osp",
            &source,
            "7\n",
        );
    }
}

#[test]
fn recursive_dynamic_calls_refuse_unproven_callback_provenance() {
    let source = "effect Alarm { ring: fn() -> int }\ntype Chain = Leaf(fn() -> int) | Next(Link)\ntype Link = { item: Chain }\nfn trigger(item) = match item { Leaf(run) => run() Next(link) => trigger(link.item) }\nlet item = Next(Link { item: Next(Link { item: Leaf(fn()=>7) }) })\nlet answer = trigger(item)\nprint(answer)\n";
    for memory in ["gc", "arc"] {
        assert_refusal(
            effect_execution::execute("recursive_dynamic_callback", "osp", source, memory),
            "program entry invokes a dynamic callable whose effect provenance cannot be proven; preserve the callable through a statically tracked value path",
        );
    }
}

fn assert_refusal(result: std::io::Result<Output>, diagnostic: &str) {
    assert!(result.is_ok(), "compiler execution failed: {result:?}");
    let Ok(output) = result else { return };
    assert!(!output.status.success(), "refused program was accepted");
    assert!(output.stdout.is_empty(), "refused program executed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stderr.lines().count(), 1, "{stderr}");
    assert!(stderr.ends_with(&format!(": {diagnostic}\n")), "{stderr}");
}

#[test]
fn ml_lambda_lists_execute_without_map_lowering() {
    effect_execution::assert_flavored_output(
        "lambda_lists", "ospml",
        "main () =\n    callbacks = [\\x => wrapAdd (x, 2), \\x => wrapMul (x, 3)]\n    answers = callbacks |> mapList (\\f => f 4)\n    print \"${listGet answers 0 ?: -1}|${listGet answers 1 ?: -1}\"\n",
        "6|12\n",
    );
}

#[test]
fn ml_curried_effects_reach_value_and_resuming_handlers_in_every_memory_mode() {
    let directory = std::env::temp_dir().join(format!("osprey_curried_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create test directory");
    let path = directory.join("curried_effects.ospml");
    std::fs::write(&path, include_str!("fixtures/curried_effects.ospml"))
        .expect("write source fixture");

    for memory in ["gc", "arc"] {
        let output = Command::new(env!("CARGO_BIN_EXE_osprey"))
            .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
            .arg(&path)
            .args(["--run", &format!("--memory={memory}")])
            .output()
            .expect("run Osprey compiler");
        assert!(
            output.status.success(),
            "{memory}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            "direct=10,control=10,audit=4\n",
            "{memory}"
        );
    }
}

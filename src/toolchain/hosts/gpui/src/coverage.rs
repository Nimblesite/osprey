//! The GPUI host's column of the coverage matrix ([NATIVE-HOSTS-COVERAGE]): every node kind the
//! framework defines, every row kind the shared frame carries and every capability kind, as
//! told, drawn or answered here, or refused with a visible diagnostic until it is. The tests
//! read the framework's own sources, so a kind added to @osprey/native fails them until it is
//! told, drawn or answered by name.

/// Row kinds `view.rs` draws, as `rows::Kind` names them ([NATIVE-SHM-VIEW]).
const DRAWN: &[&str] = &[
    "Column", "Line", "Label", "Press", "Entry", "Gap", "Scroll", "Layer", "Toggle", "Picker",
    "Choice", "Rule", "Image", "Canvas", "Editor", "Menu", "Sheet",
];

/// Row kinds that tell the component tree instead of drawing a control ([NATIVE-SHM-NODES]).
const TOLD: &[&str] = &["Field", "Record", "List"];

/// Capability kinds `host.rs` answers.
const ANSWERED: &[&str] = &[
    "animate",
    "run",
    "list-directories",
    "read-texts",
    "cancel",
    "store-read",
    "store-write",
    "store-list",
    "store-delete",
    "choose-file",
    "choose-folder",
    "save-file",
    "reveal",
];

/// Capability kinds completed with an "unsupported capability" error until this host answers them.
const UNANSWERED: &[&str] = &[
    "http",
    "measure-folders",
    "read-plists",
    "fingerprint",
    "running-applications",
    "open-settings",
    "recognise",
    "process-start",
    "process-write",
    "process-read",
    "process-stop",
];

/// Capability kinds answered only through the mobile OS bridge.
const MOBILE_ONLY: &[&str] = &["host-call"];

const NODES: &str = include_str!("../../../../osprey/packages/native/src/node.ospml");
const CAPABILITIES: &str = include_str!("../../../../osprey/packages/native/src/capability.ospml");
const TELLING: &str = include_str!("../../../../osprey/packages/native/src/told.ospml");
const ROWS: &str = include_str!("rows.rs");
const TREE: &str = include_str!("tree.rs");
const VIEW: &str = include_str!("view.rs");
const HOST: &str = include_str!("host.rs");

/// The variants of a union declared in block form, as the framework names their constructors.
fn constructors(source: &str, header: &str) -> Vec<String> {
    let mut found: Vec<String> = source
        .lines()
        .skip_while(|line| !line.starts_with(header))
        .skip(1)
        .take_while(|line| line.is_empty() || line.starts_with(char::is_whitespace))
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|name| {
            name.starts_with(char::is_uppercase) && name.chars().all(char::is_alphanumeric)
        })
        .map(str::to_owned)
        .collect();
    found.sort();
    found
}

/// The constructors the framework's exhaustive telling writes as component rows ([NATIVE-SHM-NODES]).
fn told(source: &str) -> Vec<String> {
    let mut found: Vec<String> = source
        .lines()
        .skip_while(|line| {
            !line
                .trim_start()
                .starts_with("walk (bindings, node) = match node")
        })
        .skip(1)
        .take_while(|line| line.is_empty() || line.starts_with("        "))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|name| name.starts_with(char::is_uppercase))
        .map(str::to_owned)
        .collect();
    found.sort();
    found
}

/// The capability names the framework's encoder writes, in protocol order.
fn capabilities(source: &str) -> Vec<String> {
    let mut found: Vec<String> = source
        .lines()
        .find(|line| line.trim_start().starts_with("kinds = ["))
        .map(|line| {
            line.trim()
                .trim_start_matches("kinds = [")
                .trim_end_matches(']')
                .split(',')
                .filter_map(|name| name.trim().strip_prefix('"')?.strip_suffix('"'))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// The variants of the host's row-kind check, which every row the frame carries must name.
fn row_kinds(source: &str) -> Vec<String> {
    let mut found: Vec<String> = source
        .lines()
        .skip_while(|line| !line.starts_with("pub enum Kind {"))
        .skip(1)
        .take_while(|line| !line.starts_with('}'))
        .map(|line| line.trim().trim_end_matches(','))
        .filter(|name| !name.is_empty() && name.chars().all(char::is_alphanumeric))
        .map(str::to_owned)
        .collect();
    found.sort();
    found
}

/// The capability names `rows.rs` maps codes to, as the framework's `kind` spells them.
fn row_capabilities(source: &str) -> Vec<String> {
    let block: String = source
        .lines()
        .skip_while(|line| !line.starts_with("const CAPABILITIES:"))
        .skip(1)
        .take_while(|line| !line.starts_with("];"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut found: Vec<String> = block
        .split('"')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect();
    found.sort();
    found
}

/// Every kind in the declared lists, sorted; a kind declared twice appears twice.
fn declared(lists: &[&[&str]]) -> Vec<String> {
    let mut all: Vec<String> = lists
        .iter()
        .flat_map(|list| list.iter().map(|kind| (*kind).to_string()))
        .collect();
    all.sort();
    all
}

#[test]
fn every_node_kind_is_drawn_or_refused_exactly_once() {
    let framework = constructors(NODES, "type Node A =");
    assert!(framework.iter().any(|kind| kind == "ContextMenu"));
    assert_eq!(framework, told(TELLING));
}

#[test]
fn every_row_kind_is_drawn_or_told_exactly_once() {
    assert_eq!(row_kinds(ROWS), declared(&[DRAWN, TOLD]));
}

#[test]
fn every_capability_kind_is_answered_or_refused_exactly_once() {
    let framework = capabilities(CAPABILITIES);
    assert!(framework.iter().any(|kind| kind == "measure-folders"));
    assert_eq!(framework, declared(&[ANSWERED, UNANSWERED, MOBILE_ONLY]));
    assert_eq!(row_capabilities(ROWS), framework);
}

#[test]
fn the_constructor_aliases_are_the_names_the_framework_writes() {
    let capabilities = capabilities(CAPABILITIES);
    assert!(capabilities.contains(&"read-plists".to_owned()));
    assert!(!capabilities.contains(&"read-property-lists".to_owned()));
    assert!(CAPABILITIES.contains("\"read-plists\""));
    let nodes = constructors(NODES, "type Node A =");
    assert!(nodes.contains(&"Typeset".to_owned()));
    assert!(told(TELLING).contains(&"Typeset".to_owned()));
}

#[test]
fn the_declaration_matches_what_the_host_dispatches() {
    for kind in DRAWN {
        assert!(
            VIEW.contains(&format!("Kind::{kind}")),
            "view.rs draws no {kind}"
        );
    }
    for kind in TOLD {
        assert!(
            TREE.contains(&format!("Kind::{kind}")),
            "tree.rs tells no {kind}"
        );
    }
    for kind in ANSWERED {
        assert!(
            HOST.contains(&format!("\"{kind}\"")),
            "host.rs answers no {kind}"
        );
    }
    for kind in UNANSWERED {
        assert!(
            !HOST.contains(&format!("\"{kind}\"")),
            "host.rs answers {kind}: move it to ANSWERED"
        );
    }
    assert_mobile_dispatch();
}

fn assert_mobile_dispatch() {
    let mobile_patterns = HOST
        .lines()
        .zip(HOST.lines().skip(1))
        .filter(|(_, guard)| {
            guard.trim() == "if cfg!(any(target_os = \"ios\", target_os = \"android\")) =>"
        })
        .flat_map(|(arm, _)| arm.split('|').map(str::trim));
    for kind in MOBILE_ONLY {
        let literal = format!("\"{kind}\"");
        assert_eq!(
            HOST.matches(&literal).count(),
            1,
            "host.rs must route {kind} exactly once"
        );
        assert!(
            mobile_patterns.clone().any(|pattern| pattern == literal),
            "host.rs must restrict {kind} to mobile"
        );
    }
}

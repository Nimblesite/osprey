//! The GPUI host's column of the coverage matrix ([NATIVE-HOSTS-COVERAGE]): every node and
//! capability kind the framework defines, as drawn or answered here, or refused with a visible
//! diagnostic until it is. The tests read the framework's own sources, so a kind added to
//! @osprey/native fails them until this host draws, answers or refuses it by name.

/// Node kinds `render.rs` draws.
const DRAWN: &[&str] = &[
    "stack", "text", "label", "pair", "icon", "badge", "button", "field", "toggle", "slot",
    "scene", "section", "screen", "divider", "spacer", "empty", "progress", "rows", "picker",
    "themed", "meter",
];

/// Node kinds drawn as an "unknown node kind" diagnostic until this host draws them
/// ([plan 0036](../../../docs/plans/0036-gpui-desktop-host.md), [plan 0039](../../../docs/plans/0039-examples-on-osprey-native.md)).
const REFUSED: &[&str] = &[
    "split",
    "layer",
    "context-menu",
    "aside",
    "tabs",
    "link",
    "segments",
    "swatch",
    "disclosure",
    "alert",
    "rich-text",
    "framed",
    "dialog",
    "evidence",
    "brand",
    "math",
    "image",
    "video",
    "editor",
    "board",
];

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
    "host-call",
    "measure-folders",
    "read-plists",
    "fingerprint",
    "running-applications",
    "open-settings",
    "recognise",
];

const NODES: &str = include_str!("../../../packages/native/src/node.ospml");
const CAPABILITIES: &str = include_str!("../../../packages/native/src/capability.ospml");
const ENCODER: &str = include_str!("../../../packages/native/src/encode.ospml");
const RENDER: &str = include_str!("render.rs");
const HOST: &str = include_str!("host.rs");

/// The variants of a union declared in block form, as the encoder names them on the wire.
fn kinds(source: &str, header: &str) -> Vec<String> {
    let mut found: Vec<String> = source
        .lines()
        .skip_while(|line| !line.starts_with(header))
        .skip(1)
        .take_while(|line| line.is_empty() || line.starts_with(char::is_whitespace))
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|name| {
            name.starts_with(char::is_uppercase) && name.chars().all(char::is_alphanumeric)
        })
        .map(wire)
        .collect();
    found.sort();
    found
}

/// The encoder's wire name for a constructor, including the names that differ from the kebab spelling.
fn wire(name: &str) -> String {
    match name {
        "Typeset" => "math".to_owned(),
        "ReadPropertyLists" => "read-plists".to_owned(),
        _ => kebab(name),
    }
}

/// A constructor's wire name: `ContextMenu` travels as `context-menu`.
fn kebab(name: &str) -> String {
    let mut wire = String::new();
    for (index, letter) in name.chars().enumerate() {
        if letter.is_uppercase() && index > 0 {
            wire.push('-');
        }
        wire.push(letter.to_ascii_lowercase());
    }
    wire
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
    let framework = kinds(NODES, "type Node A =");
    assert!(framework.iter().any(|kind| kind == "context-menu"));
    assert_eq!(framework, declared(&[DRAWN, REFUSED]));
}

#[test]
fn every_capability_kind_is_answered_or_refused_exactly_once() {
    let framework = kinds(CAPABILITIES, "type Capability =");
    assert!(framework.iter().any(|kind| kind == "measure-folders"));
    assert_eq!(framework, declared(&[ANSWERED, UNANSWERED]));
}

#[test]
fn the_constructor_aliases_are_the_wire_names_the_encoder_writes() {
    let nodes = kinds(NODES, "type Node A =");
    let capabilities = kinds(CAPABILITIES, "type Capability =");
    assert!(nodes.contains(&"math".to_owned()));
    assert!(!nodes.contains(&"typeset".to_owned()));
    assert!(capabilities.contains(&"read-plists".to_owned()));
    assert!(!capabilities.contains(&"read-property-lists".to_owned()));
    assert!(ENCODER.contains("\"math\""));
    assert!(CAPABILITIES.contains("\"read-plists\""));
}

#[test]
fn the_declaration_matches_what_the_host_dispatches() {
    for kind in DRAWN {
        assert!(
            RENDER.contains(&format!("\"{kind}\" =>")),
            "render.rs draws no {kind}"
        );
    }
    for kind in REFUSED {
        assert!(
            !RENDER.contains(&format!("\"{kind}\" =>")),
            "render.rs draws {kind}: move it to DRAWN"
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
}

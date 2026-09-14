//! Library exports are independent effect roots. [IOS-HOST-ABI]

use crate::{check_program, check_program_exports};

fn program(source: &str) -> osprey_ast::Program {
    let parsed = osprey_syntax::parse_program(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert!(check_program(&parsed.program).is_empty());
    parsed.program
}

#[test]
fn every_export_requires_its_own_handlers() {
    let p = program("effect Alarm { ring: fn() -> int }\nfn ring() = perform Alarm.ring()\nfn relay() = ring()\nfn safe() = handle Alarm\n ring => 7\nin relay()\n");
    assert!(check_program_exports(&p, &["safe"]).is_empty());
    let errors = check_program_exports(&p, &["ring", "relay", "safe"]);
    assert_eq!(errors.len(), 2, "{errors:?}");
    for name in ["ring", "relay"] {
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains(&format!("library export `{name}`"))
                    && e.message.contains("Alarm.ring")),
            "{errors:?}"
        );
    }
}

#[test]
fn missing_exports_are_reported_even_without_any_effects() {
    let p = program("fn answer() = 42\n");
    assert!(check_program_exports(&p, &["answer"]).is_empty());
    let errors = check_program_exports(&p, &["missing"]);
    assert!(
        errors.iter().any(|e| e
            .message
            .contains("library export `missing` is not a defined function")),
        "{errors:?}"
    );
}

#[test]
fn exported_callbacks_with_unresolved_effects_are_rejected() {
    let p = program("effect Alarm { ring: fn() -> int }\nfn invoke(f) = f()\n");
    let errors = check_program_exports(&p, &["invoke"]);
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("library export `invoke`")
                && e.message
                    .contains("callback whose effects cannot be discharged")),
        "{errors:?}"
    );
}

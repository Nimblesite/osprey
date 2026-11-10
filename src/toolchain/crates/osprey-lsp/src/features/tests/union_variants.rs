//! Union variants hover and define from every site, in both flavors and
//! across a project.

use super::{col_of, definitions_at, first_definition, hover, word_span, U16};

const VARIANTS_ML: &str = "(** A semantic icon. *)\n\
type Symbol = NoSymbol | Clock | Person\n\n\
(** A host's system. *)\n\
type Platform =\n    Macos\n    OtherPlatform\n        platformName : string\n\n\
module Symbols\n    name symbol = match symbol\n        NoSymbol => \"\"\n        Clock => \"clock\"\n        _ => \"person\"\n\n\
    system platform = match platform\n        Macos => \"macos\"\n        OtherPlatform named => named\n\n\
    chosen = Clock\n";

const VARIANTS_DEFAULT: &str = "/// A semantic icon.\n\
type Symbol = NoSymbol | Clock | Person\n\n\
/// A host's system.\n\
type Platform = Macos | OtherPlatform { platformName: string }\n\n\
fn name(symbol) = match symbol {\n    NoSymbol => \"\"\n    Clock => \"clock\"\n    _ => \"person\"\n}\n\n\
fn system(platform) = match platform {\n    Macos => \"macos\"\n    OtherPlatform { platformName } => platformName\n}\n\n\
let chosen = Clock\n";

fn line_of(src: &str, needle: &str) -> usize {
    src.lines()
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no line holds {needle}"))
}

/// A variant's hover and definition from its `match` arm, a value and its own name.
fn assert_variant_navigates(src: &str, uri: &str, word: &str, declared: &str, shown: &[&str]) {
    let declaration = (line_of(src, declared), word);
    let sites = src
        .lines()
        .enumerate()
        .filter(|(_, line)| {
            line.split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|w| w == word)
        })
        .map(|(row, _)| (row, word));
    for cursor in sites {
        assert_eq!(
            definitions_at(src, uri, cursor),
            vec![word_span(src, declaration)],
            "definition of {word} at {cursor:?} in {uri}"
        );
        let row = u32::try_from(cursor.0).expect("line fits");
        let found = hover(src, uri, row, col_of(src, cursor.0, word), U16);
        let found = found.unwrap_or_else(|| panic!("no hover on {word} at {cursor:?} in {uri}"));
        crate::testkit::shows(&found, shown);
    }
}

// [LSP-DEFINITION-VARIANTS] [LSP-HOVER-VARIANTS]: `Clock => "clock"` hovered nothing and defined
// nothing, because a union's variants were never symbols.
#[test]
fn union_variants_hover_and_define_from_every_site_in_both_flavors() {
    // The `Platform` declaration needle follows each flavor's layout.
    for (src, uri, platform, other_platform) in [
        (
            VARIANTS_ML,
            "file:///variants.ospml",
            "    Macos",
            "    OtherPlatform",
        ),
        (
            VARIANTS_DEFAULT,
            "file:///variants.osp",
            "type Platform",
            "type Platform",
        ),
    ] {
        let variants: [(&str, &str, &[&str]); 4] = [
            (
                "NoSymbol",
                "type Symbol",
                &["NoSymbol", "Symbol", "A semantic icon"],
            ),
            (
                "Clock",
                "type Symbol",
                &["Clock", "Symbol", "A semantic icon"],
            ),
            ("Macos", platform, &["Macos", "Platform", "A host's system"]),
            (
                "OtherPlatform",
                other_platform,
                &[
                    "OtherPlatform",
                    "platformName",
                    "Platform",
                    "A host's system",
                ],
            ),
        ];
        for (word, declared, shown) in variants {
            assert_variant_navigates(src, uri, word, declared, shown);
        }
    }
}

// [LSP-WORKSPACE]: the application's arm names a variant another file of the project declares.
#[test]
fn union_variants_hover_and_define_across_the_project() -> std::io::Result<()> {
    let fixture = crate::test_support::ProjectFixture::new("ospml")?;
    let declaring = "(** A semantic icon. *)\ntype Symbol = NoSymbol | Clock | Person\n";
    let using = "label symbol = match symbol\n    Clock => \"clock\"\n    _ => \"other\"\n";
    let declared_in = fixture.write("symbol", declaring)?;
    let uri = fixture.write("main", using)?;
    let first = first_definition(using, &uri, 1, 5);
    assert_eq!(first.uri, declared_in, "{first:?}");
    assert_eq!(first.span, word_span(declaring, (1, "Clock")), "{first:?}");
    let shown = hover(using, &uri, 1, 5, U16).expect("hover across the project");
    crate::testkit::shows(&shown, &["Clock", "Symbol", "A semantic icon"]);
    Ok(())
}

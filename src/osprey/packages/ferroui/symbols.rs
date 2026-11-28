//! The framework's symbols (`ui::Symbols::name`) as text glyphs. FerroUI bundles no symbol font
//! (a `SymbolIcon` needs one the application supplies), so a symbol is drawn as a character of
//! the system font in text presentation; each symbol has exactly one, and the tests fail when the
//! framework names a symbol without one.

/// Every symbol name Osprey Native 0.29 defines, in the order of `ui::Symbols::name`.
pub const SYMBOLS: [&str; 33] = [
    "add", "delete", "dismiss", "check", "warning", "info", "question", "search", "settings",
    "star", "play", "pause", "stop", "refresh", "folder", "document", "package", "inbox", "globe",
    "external", "lock", "shield", "branch", "tag", "clock", "person", "cursor", "chevron", "send",
    "terminal", "edit", "panel", "sparkle",
];

/// The glyph of a symbol, or `None` for a name the framework does not define. U+FE0E asks for
/// the text presentation where a character also has a coloured one.
pub fn glyph(name: &str) -> Option<&'static str> {
    Some(match name {
        "add" => "+",
        "delete" => "\u{232B}",
        "dismiss" => "\u{2715}",
        "check" => "\u{2713}",
        "warning" => "\u{26A0}\u{FE0E}",
        "info" => "\u{24D8}",
        "question" => "?",
        "search" => "\u{2315}",
        "settings" => "\u{2699}\u{FE0E}",
        "star" => "\u{2605}",
        "play" => "\u{25B6}\u{FE0E}",
        "pause" => "\u{2016}",
        "stop" => "\u{25A0}",
        "refresh" => "\u{21BB}",
        "folder" => "\u{25A4}",
        "document" => "\u{25A1}",
        "package" => "\u{25A3}",
        "inbox" => "\u{2193}",
        "globe" => "\u{25CE}",
        "external" => "\u{2197}",
        "lock" => "\u{26BF}",
        "shield" => "\u{26E8}",
        "branch" => "\u{2387}",
        "tag" => "#",
        "clock" => "\u{25F7}",
        "person" => "\u{263A}\u{FE0E}",
        "cursor" => "\u{2316}",
        "chevron" => "\u{203A}",
        "send" => "\u{27A4}",
        "terminal" => "\u{276F}",
        "edit" => "\u{270E}",
        "panel" => "\u{25EB}",
        "sparkle" => "\u{2726}",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{glyph, SYMBOLS};

    #[test]
    fn every_framework_symbol_has_one_glyph() {
        let missing: Vec<_> = SYMBOLS
            .iter()
            .filter(|name| glyph(name).is_none())
            .collect();
        assert!(missing.is_empty(), "no glyph for {missing:?}");
    }

    #[test]
    fn platform_spellings_are_not_symbols() {
        for name in ["xmark", "trash", "gear", "chevron.right", ""] {
            assert!(glyph(name).is_none(), "{name} is not a framework symbol");
        }
    }
}

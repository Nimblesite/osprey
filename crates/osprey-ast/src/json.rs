//! Exact JSON escaping shared by compiler artifacts and editor output.

use std::fmt::Write as _;

/// The escape spelling required by each existing artifact format.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Escaping {
    /// Standard JSON, including the short backspace and form-feed escapes.
    Standard,
    /// Keep backspace and form-feed in their four-digit Unicode spelling.
    UnicodeControls,
    /// Unicode controls plus HTML-safe angle brackets and ampersands.
    Script,
}

/// Encode string content without surrounding quotes.
#[must_use]
pub fn content(raw: &str, style: Escaping) -> String {
    raw.chars()
        .fold(String::with_capacity(raw.len()), |mut out, ch| {
            if let Some(escaped) = escape(ch, style) {
                out.push_str(escaped);
            } else if ch < ' ' {
                let _ = write!(out, "\\u{:04x}", u32::from(ch));
            } else {
                out.push(ch);
            }
            out
        })
}

/// Encode a complete JSON string, preserving the selected escape spelling.
#[must_use]
pub fn quoted(raw: &str, style: Escaping) -> String {
    format!("\"{}\"", content(raw, style))
}

fn escape(ch: char, style: Escaping) -> Option<&'static str> {
    match ch {
        '"' => Some("\\\""),
        '\\' => Some("\\\\"),
        '\n' => Some("\\n"),
        '\r' => Some("\\r"),
        '\t' => Some("\\t"),
        '\u{8}' if style == Escaping::Standard => Some("\\b"),
        '\u{c}' if style == Escaping::Standard => Some("\\f"),
        '<' if style == Escaping::Script => Some("\\u003c"),
        '>' if style == Escaping::Script => Some("\\u003e"),
        '&' if style == Escaping::Script => Some("\\u0026"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{content, quoted, Escaping};
    const TEXT: &str = "\0\u{8}\u{c}\n\r\t\"\\<>&é🙂";

    #[test]
    fn artifact_styles_preserve_control_and_unicode_spellings() {
        assert_eq!(
            content(TEXT, Escaping::Standard),
            r#"\u0000\b\f\n\r\t\"\\<>&é🙂"#
        );
        assert_eq!(
            content(TEXT, Escaping::UnicodeControls),
            r#"\u0000\u0008\u000c\n\r\t\"\\<>&é🙂"#
        );
        assert_eq!(
            content(TEXT, Escaping::Script),
            r#"\u0000\u0008\u000c\n\r\t\"\\\u003c\u003e\u0026é🙂"#
        );
    }

    #[test]
    fn surrounding_quotes_are_added_only_for_complete_json_strings() {
        assert_eq!(quoted("", Escaping::Standard), "\"\"");
        assert_eq!(quoted("hello", Escaping::UnicodeControls), "\"hello\"");
        assert_eq!(
            content("</script>", Escaping::Script),
            "\\u003c/script\\u003e"
        );
    }
}

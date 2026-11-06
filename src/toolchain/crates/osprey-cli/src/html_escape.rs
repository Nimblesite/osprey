//! Shared escaping for generated documentation and Hunt reports.

/// Escape text for HTML element content.
pub(crate) fn text(raw: &str) -> String {
    escape(raw, false)
}

/// Escape text for a double-quoted HTML attribute.
pub(crate) fn attr(raw: &str) -> String {
    escape(raw, true)
}

fn escape(raw: &str, attribute: bool) -> String {
    raw.chars()
        .fold(String::with_capacity(raw.len()), |mut out, c| {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' if attribute => out.push_str("&quot;"),
                '\'' if attribute => out.push_str("&#39;"),
                _ => out.push(c),
            }
            out
        })
}

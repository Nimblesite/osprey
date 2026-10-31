//! Lexing one `osprey.toml` value: comments, list separators and basic strings.

// Implements [MODULES-PROJECT].
//!
//! One quote-aware scan answers every question a value asks — where a comment
//! starts, where a comma separates items, and whether a string was left open —
//! so the three callers cannot drift apart.

/// Byte offsets of `separator` outside a basic string, and whether the scan
/// ended inside one (an unterminated string or a trailing escape).
fn separators(text: &str, separator: char) -> (Vec<usize>, bool) {
    let mut found = Vec::new();
    let mut quoted = false;
    let mut escaped = false;
    for (index, character) in text.char_indices() {
        if escaped {
            escaped = false;
        } else if character == '\\' && quoted {
            escaped = true;
        } else if character == '"' {
            quoted = !quoted;
        } else if character == separator && !quoted {
            found.push(index);
        }
    }
    (found, quoted || escaped)
}

/// `raw` up to the first `#` outside a basic string.
pub(super) fn strip_comment(raw: &str) -> &str {
    let (comments, _) = separators(raw, '#');
    comments.first().map_or(raw, |start| &raw[..*start])
}

/// The comma-separated items of a list or inline table, ignoring separators
/// inside strings.
pub(super) fn split_items(inner: &str) -> Result<Vec<&str>, String> {
    let (commas, open) = separators(inner, ',');
    if open {
        return Err("a string in the value is unterminated".to_string());
    }
    let mut items = Vec::with_capacity(commas.len() + 1);
    let mut start = 0;
    for comma in commas {
        items.push(inner[start..comma].trim());
        start = comma + 1;
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        items.push(last);
    }
    Ok(items)
}

/// `field` split at its first `=` outside a basic string, so a quoted key may
/// hold `=` itself (`"metadata=tag" = "ignored"`).
pub(super) fn split_assignment(field: &str) -> Option<(&str, &str)> {
    let (equals, _) = separators(field, '=');
    let index = *equals.first()?;
    Some((&field[..index], &field[index + 1..]))
}

/// A table key as written: bare, or a quoted basic string with its escapes
/// decoded, so `path`, `"path"` and `"\u0070ath"` name one supported field.
pub(super) fn key_name(key: &str) -> Result<String, String> {
    let key = key.trim();
    if key.starts_with('"') {
        return parse_string(key);
    }
    if key.is_empty() {
        return Err("an inline table field has no key name".to_string());
    }
    Ok(key.to_string())
}

/// One quoted basic string, with its escapes decoded.
pub(super) fn parse_string(value: &str) -> Result<String, String> {
    let inner = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or_else(|| "expected a quoted string".to_string())?;
    decode_basic(inner)
}

/// A quoted string list.
pub(super) fn parse_list(value: &str) -> Result<Vec<String>, String> {
    let inner = value
        .strip_prefix('[')
        .and_then(|items| items.strip_suffix(']'))
        .ok_or_else(|| "expected a string list".to_string())?;
    if inner.trim().is_empty() {
        return Ok(Vec::new());
    }
    split_items(inner)?.into_iter().map(parse_string).collect()
}

fn decode_basic(inner: &str) -> Result<String, String> {
    let mut decoded = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        match character {
            '"' => return Err("an unescaped `\"` inside a string".to_string()),
            '\\' => push_escape(&mut decoded, &mut characters)?,
            plain => push_plain(&mut decoded, plain)?,
        }
    }
    Ok(decoded)
}

/// A raw character: TOML forbids C0 controls and DEL, but allows TAB and C1.
fn push_plain(decoded: &mut String, character: char) -> Result<(), String> {
    if matches!(character, '\u{0}'..='\u{8}' | '\u{a}'..='\u{1f}' | '\u{7f}') {
        let code = u32::from(character);
        return Err(format!("a raw control character U+{code:04X} in a string"));
    }
    decoded.push(character);
    Ok(())
}

fn push_escape(decoded: &mut String, characters: &mut std::str::Chars<'_>) -> Result<(), String> {
    let escape = characters
        .next()
        .ok_or_else(|| "a string ends in an unfinished escape".to_string())?;
    match escape {
        'u' => decoded.push(hex_escape(characters, 4)?),
        'U' => decoded.push(hex_escape(characters, 8)?),
        simple => decoded.push(simple_escape(simple)?),
    }
    Ok(())
}

fn simple_escape(escape: char) -> Result<char, String> {
    match escape {
        '"' => Ok('"'),
        '\\' => Ok('\\'),
        'b' => Ok('\u{8}'),
        'f' => Ok('\u{c}'),
        'n' => Ok('\n'),
        'r' => Ok('\r'),
        't' => Ok('\t'),
        other => Err(format!("unsupported escape `\\{other}` in a string")),
    }
}

fn hex_escape(characters: &mut std::str::Chars<'_>, digits: usize) -> Result<char, String> {
    let mut value = 0_u32;
    for _ in 0..digits {
        let digit = characters
            .next()
            .and_then(|character| character.to_digit(16))
            .ok_or_else(|| "an escape ends in unfinished hex digits".to_string())?;
        value = value * 16 + digit;
    }
    char::from_u32(value).ok_or_else(|| format!("`\\u{value:X}` is not a scalar value"))
}

#[cfg(test)]
mod tests {
    use super::{parse_list, parse_string, split_items, strip_comment};

    #[test]
    fn comments_ignore_hash_inside_strings() {
        assert_eq!(
            strip_comment(r#"name = "keep # this" # note"#),
            r#"name = "keep # this" "#
        );
        assert_eq!(strip_comment("[project] # the section"), "[project] ");
        assert_eq!(
            strip_comment(r#""escaped \" # still"#),
            r#""escaped \" # still"#
        );
    }

    #[test]
    fn separators_ignore_commas_inside_strings() {
        assert_eq!(split_items(r#""a,b", c"#), Ok(vec![r#""a,b""#, "c"]));
        assert!(split_items(r#""unterminated"#).is_err());
    }

    #[test]
    fn refuses_raw_c0_controls_and_del_but_keeps_tab_and_c1() {
        for raw in ["\u{1}", "\u{7f}"] {
            let quoted = format!("\"{raw}\"");
            assert!(
                parse_string(&quoted).is_err(),
                "raw {raw:?} must be refused"
            );
        }
        for allowed in ["\t", "\u{85}"] {
            let quoted = format!("\"{allowed}\"");
            assert!(
                parse_string(&quoted).is_ok(),
                "raw {allowed:?} must stay legal"
            );
        }
    }

    #[test]
    fn decodes_every_supported_escape_and_rejects_the_rest() {
        let decoded = parse_string(r#""a\\b\"c\td\u0041\U0001F600""#);
        let expected = Ok("a\\b\"c\tdA\u{1F600}".to_string());
        assert_eq!(decoded, expected);
        for bad in [r#""\q""#, r#""\u00""#, r#""\uD800""#, r#""a\""#] {
            assert!(parse_string(bad).is_err(), "{bad} must be refused");
        }
    }

    #[test]
    fn an_empty_list_is_an_empty_vec() {
        assert_eq!(parse_list("[]"), Ok(Vec::new()));
        assert_eq!(parse_list("[   ]"), Ok(Vec::new()));
    }

    #[test]
    fn an_unescaped_interior_quote_is_refused() {
        let refused = parse_string(r#""a"b""#);
        assert_eq!(
            refused,
            Err("an unescaped `\"` inside a string".to_string())
        );
    }

    #[test]
    fn decodes_the_four_letter_escapes() {
        let decoded = parse_string(r#""a\bb\fc\nd\re\tf""#);
        assert_eq!(decoded, Ok("a\u{8}b\u{c}c\nd\re\tf".to_string()));
    }

    #[test]
    fn lists_keep_commas_and_hashes_inside_items() {
        let list = parse_list(r#"["comma,inside", "hash#inside"]"#);
        assert_eq!(
            list,
            Ok(vec!["comma,inside".to_string(), "hash#inside".to_string()])
        );
    }
}

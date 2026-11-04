//! Strings retain Python delimiters in the CST and original expression spans.
use super::context::{children, Lowerer, Result};
use osprey_ast::{Expr, InterpolatedPart};
use tree_sitter::Node;

impl Lowerer<'_> {
    pub(super) fn string(&mut self, node: Node<'_>) -> Result<Expr> {
        let opening = Self::first(node)?;
        let prefix = self
            .text(opening)
            .trim_end_matches(['\'', '"'])
            .to_ascii_lowercase();
        if prefix.contains('b') {
            return self.unsupported(opening);
        }
        let parts = self.string_parts(node, &prefix)?;
        Ok(if prefix.contains('f') {
            Expr::InterpolatedStr(parts)
        } else {
            Expr::Str(text_parts(parts))
        })
    }

    fn string_parts(&mut self, node: Node<'_>, prefix: &str) -> Result<Vec<InterpolatedPart>> {
        let mut parts = Vec::new();
        for child in children(node) {
            match child.kind() {
                "string_start" | "string_end" => (),
                "string_content" => parts.push(InterpolatedPart::Text(self.content(
                    child,
                    prefix.contains('r'),
                    prefix.contains('f'),
                )?)),
                "interpolation" => parts.push(InterpolatedPart::Expr(self.interpolation(child)?)),
                _ => return self.unsupported(child),
            }
        }
        Ok(parts)
    }

    /// Brace syntax belongs to the source text, so it is split before escapes
    /// decode: `\x7b\x7b` is two literal braces, not one escaped brace.
    fn content(&self, node: Node<'_>, raw: bool, formatted: bool) -> Result<String> {
        let text = self.text(node);
        let braces = if formatted {
            text.replace("{{", "{").replace("}}", "}")
        } else {
            text.into()
        };
        if raw {
            return Ok(braces);
        }
        decode(&braces).map_err(|message| Self::error(node, message))
    }

    fn interpolation(&mut self, node: Node<'_>) -> Result<Expr> {
        if children(node)
            .iter()
            .any(|n| matches!(n.kind(), "format_specifier" | "type_conversion"))
            || self
                .text(node)
                .trim_end_matches('}')
                .trim_end()
                .ends_with('=')
        {
            return self.unsupported(node);
        }
        self.expr(Self::field(node, "expression")?)
    }
}

fn text_parts(parts: Vec<InterpolatedPart>) -> String {
    parts
        .into_iter()
        .filter_map(|part| match part {
            InterpolatedPart::Text(text) => Some(text),
            InterpolatedPart::Expr(_) => None,
        })
        .collect()
}

fn decode(text: &str) -> std::result::Result<String, String> {
    let mut chars = text.chars();
    let mut output = String::new();
    while let Some(character) = chars.next() {
        if character == '\\' {
            output.push_str(&escape(&mut chars)?);
        } else {
            output.push(character);
        }
    }
    Ok(output)
}

fn escape(chars: &mut std::str::Chars<'_>) -> std::result::Result<String, String> {
    let Some(character) = chars.next() else {
        return Err("incomplete Python string escape".into());
    };
    if let Some(decoded) = ascii_escape(character) {
        return Ok(decoded.into());
    }
    match character {
        '\n' => Ok(String::new()),
        'x' => unicode_escape(chars, 2),
        'u' => unicode_escape(chars, 4),
        'U' => unicode_escape(chars, 8),
        '0'..='7' => octal_escape(chars, character),
        'N' => Err("named Unicode escapes are not supported in Osprey Python strings".into()),
        other => Ok(format!("\\{other}")),
    }
}

fn ascii_escape(character: char) -> Option<char> {
    let decoded = match character {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'a' => '\u{7}',
        'b' => '\u{8}',
        'f' => '\u{c}',
        'v' => '\u{b}',
        '\\' => '\\',
        '\'' => '\'',
        '"' => '"',
        _ => return None,
    };
    Some(decoded)
}

fn unicode_escape(
    chars: &mut std::str::Chars<'_>,
    count: usize,
) -> std::result::Result<String, String> {
    let digits: String = chars.take(count).collect();
    if digits.len() != count {
        return Err("incomplete Unicode escape".into());
    }
    codepoint(&digits, 16)
}

fn octal_escape(
    chars: &mut std::str::Chars<'_>,
    first: char,
) -> std::result::Result<String, String> {
    let mut digits = first.to_string();
    for _ in 0..2 {
        match chars.clone().next() {
            Some(next @ '0'..='7') => {
                let _ = chars.next();
                digits.push(next);
            }
            _ => break,
        }
    }
    codepoint(&digits, 8)
}

fn codepoint(digits: &str, radix: u32) -> std::result::Result<String, String> {
    let code = u32::from_str_radix(digits, radix)
        .map_err(|error| format!("invalid base-{radix} string escape: {error}"))?;
    char::from_u32(code)
        .map(String::from)
        .ok_or_else(|| "escape is not a Unicode scalar value".into())
}

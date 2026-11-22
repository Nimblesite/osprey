//! The shared syntax nesting limit, enforced before the TypeScript parser
//! recurses. `oxc_parser` descends once per nested bracket, operator and
//! prefix, so a source nested past the limit is rejected by this lexical scan
//! and never reaches it. Implements [FAILURE-EXPLICIT] and [FLAVOR-TS-CST].

/// One open bracket or template hole, and the operators seen since its last
/// `,` or `;`: each is a level of the expression tree the parser builds.
#[derive(Default)]
struct Level {
    operators: usize,
    hole: bool,
}

/// The byte offset at which `source` first nests deeper than `limit`.
pub(super) fn too_deep(source: &str, limit: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut levels = vec![Level::default()];
    let mut at = 0;
    while let Some(&byte) = bytes.get(at) {
        let next = bytes.get(at + 1).copied();
        at = match byte {
            b'/' if next == Some(b'/') => line_end(bytes, at),
            b'/' if next == Some(b'*') => comment_end(bytes, at),
            b'\'' | b'"' => string_end(bytes, at, byte),
            b'`' => template(bytes, at, &mut levels),
            b'(' | b'[' | b'{' => {
                levels.push(Level::default());
                at + 1
            }
            b'}' if levels.last().is_some_and(|level| level.hole) => {
                let _ = levels.pop();
                template(bytes, at, &mut levels)
            }
            b')' | b']' | b'}' => {
                if levels.len() > 1 {
                    let _ = levels.pop();
                }
                at + 1
            }
            b',' | b';' => {
                if let Some(level) = levels.last_mut() {
                    level.operators = 0;
                }
                at + 1
            }
            _ if OPERATORS.contains(&byte) => {
                count(&mut levels);
                operator_end(bytes, at)
            }
            _ if byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$' => {
                let end = word_end(bytes, at);
                if source
                    .get(at..end)
                    .is_some_and(|word| PREFIX_WORDS.contains(&word))
                {
                    count(&mut levels);
                }
                end
            }
            _ => at + 1,
        };
        let depth = levels.len() + levels.iter().map(|level| level.operators).sum::<usize>();
        if depth > limit {
            return Some(at.min(source.len()).saturating_sub(1));
        }
    }
    None
}

const OPERATORS: &[u8] = b"?!~+-*/%<>=&|^.:";
const PREFIX_WORDS: [&str; 6] = ["typeof", "void", "delete", "await", "new", "yield"];

/// Scans template text after `at`; a `${` hole opens a level that its `}` closes.
fn template(bytes: &[u8], at: usize, levels: &mut Vec<Level>) -> usize {
    let (end, hole) = template_text(bytes, at + 1);
    if hole {
        levels.push(Level {
            operators: 0,
            hole: true,
        });
    }
    end
}

fn count(levels: &mut [Level]) {
    if let Some(level) = levels.last_mut() {
        level.operators += 1;
    }
}

fn operator_end(bytes: &[u8], at: usize) -> usize {
    // A run of operator characters is one token (`===`, `=>`, `&&`), except
    // that a prefix run such as `!!!` or `- - -` nests once per character.
    let first = bytes.get(at).copied();
    if matches!(first, Some(b'!' | b'~')) {
        return at + 1;
    }
    let mut end = at + 1;
    while bytes
        .get(end)
        .is_some_and(|byte| OPERATORS.contains(byte) && !matches!(byte, b'!' | b'~' | b'-' | b'+'))
    {
        end += 1;
    }
    end
}

fn word_end(bytes: &[u8], at: usize) -> usize {
    let mut end = at;
    while bytes
        .get(end)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    {
        end += 1;
    }
    end
}

fn line_end(bytes: &[u8], at: usize) -> usize {
    bytes
        .get(at..)
        .and_then(|rest| rest.iter().position(|&byte| byte == b'\n'))
        .map_or(bytes.len(), |end| at + end)
}

fn comment_end(bytes: &[u8], at: usize) -> usize {
    bytes
        .get(at + 2..)
        .and_then(|rest| rest.windows(2).position(|pair| pair == b"*/"))
        .map_or(bytes.len(), |end| at + 2 + end + 2)
}

fn string_end(bytes: &[u8], at: usize, quote: u8) -> usize {
    let mut end = at + 1;
    while let Some(&byte) = bytes.get(end) {
        end += if byte == b'\\' { 2 } else { 1 };
        if byte == quote || byte == b'\n' {
            break;
        }
    }
    end
}

/// The end of template text from `at`, and whether it stopped at a `${` hole
/// rather than the closing backtick.
fn template_text(bytes: &[u8], at: usize) -> (usize, bool) {
    let mut end = at;
    while let Some(&byte) = bytes.get(end) {
        match byte {
            b'\\' => end += 2,
            b'`' => return (end + 1, false),
            b'$' if bytes.get(end + 1) == Some(&b'{') => return (end + 2, true),
            _ => end += 1,
        }
    }
    (end, false)
}

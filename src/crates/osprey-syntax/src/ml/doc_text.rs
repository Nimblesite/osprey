//! Normalises the raw inner text of an ML `(** … *)` doc comment into the
//! Markdown the shared body parser reads. Implements [DOC-SIGIL-ML].

/// The Markdown fence that opens and closes a code block.
const DOC_FENCE: &str = "```";

/// Trim the outer blank margins and, per prose line, drop leading whitespace
/// and one optional `*` continuation marker (the odoc convention). A line
/// inside a fenced code block keeps its indentation beyond the opening fence's
/// margin, because ML layout is syntax.
pub(super) fn strip_doc_lines(raw: &str) -> String {
    raw.trim()
        .lines()
        .scan(None::<&str>, |fence, line| {
            let prose = prose_text(line);
            let delimiter = prose.starts_with(DOC_FENCE);
            let text = match *fence {
                Some(open) if !delimiter => code_text(line, open),
                _ => prose,
            };
            // A delimiter opens a block when none is open and closes it otherwise.
            *fence = fence.xor(delimiter.then_some(line));
            Some(text.trim_end())
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// A prose line without its leading whitespace, `*` marker and the one space
/// after that marker.
fn prose_text(line: &str) -> &str {
    let t = line.trim_start();
    t.strip_prefix('*')
        .map_or(t, |r| r.strip_prefix(' ').unwrap_or(r))
}

/// A code line without the margin its opening fence line carried: the same
/// `*` marker when the fence had one, else at most the fence's indentation.
fn code_text<'a>(line: &'a str, fence: &str) -> &'a str {
    if fence.trim_start().starts_with('*') {
        return prose_text(line);
    }
    let width = indentation(fence);
    line.get(indentation(line).min(width)..).unwrap_or(line)
}

/// The count of leading ASCII spaces and tabs, always a char boundary.
fn indentation(line: &str) -> usize {
    line.bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count()
}

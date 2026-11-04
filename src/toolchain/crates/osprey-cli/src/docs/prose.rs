//! Extract metadata from Markdown structure, never from example code.
//! Implements [DOC-EXPORT-PAGES] and [DOC-EXPORT-HTML].

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};

/// Front matter belongs to the host site rather than the rendered article.
pub(super) fn body(markdown: &str) -> &str {
    let Some(rest) = markdown
        .strip_prefix("---\n")
        .or_else(|| markdown.strip_prefix("---\r\n"))
    else {
        return markdown;
    };
    let mut offset: usize = 0;
    for line in rest.split_inclusive('\n') {
        offset = offset.saturating_add(line.len());
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return rest.get(offset..).unwrap_or_default();
        }
    }
    markdown
}

pub(super) fn title(markdown: &str) -> Option<String> {
    first_block(markdown, |tag| {
        matches!(
            tag,
            Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }
        )
    })
}

pub(super) fn summary(markdown: &str) -> String {
    first_block(markdown, |tag| matches!(tag, Tag::Paragraph)).unwrap_or_default()
}

/// The parser distinguishes real headings and prose from text inside fences.
fn first_block(markdown: &str, wanted: impl Fn(&Tag<'_>) -> bool) -> Option<String> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_STRIKETHROUGH;
    let mut events = Parser::new_ext(body(markdown), options);
    let end = events.find_map(|event| match event {
        Event::Start(tag) if wanted(&tag) => Some(tag.to_end()),
        _ => None,
    })?;
    let text: String = events
        .take_while(|event| !matches!(event, Event::End(tag) if *tag == end))
        .filter_map(|event| match event {
            Event::Text(text) | Event::Code(text) | Event::InlineHtml(text) => {
                Some(text.into_string())
            }
            Event::SoftBreak | Event::HardBreak => Some(" ".into()),
            _ => None,
        })
        .collect();
    Some(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

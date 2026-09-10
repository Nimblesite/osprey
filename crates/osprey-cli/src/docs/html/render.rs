//! Markdown rendering and the four escaping contexts an exported page uses.
//! Implements [DOC-EXPORT-HTML].
//!
//! **Raw HTML policy.** Documentation comes from source comments and from
//! user-supplied Markdown files, and both are treated as UNTRUSTED text: raw
//! HTML blocks and inline HTML in Markdown are escaped and shown as literal
//! text rather than passed through. A doc comment must never be able to inject
//! markup or script into the generated site, and a reader is better served by
//! seeing `<script>` than by silently losing it.

use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};

/// Escape text for HTML element content.
pub(super) fn text(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape text for a double-quoted HTML attribute. Element escaping is not
/// enough here: an unescaped quote closes the attribute early.
pub(super) fn attr(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escape a string as a JSON string body (no surrounding quotes). The search
/// index is embedded in a `<script type="application/json">` element, so `<`
/// and `&` are escaped too: a `</script>` inside a doc comment would otherwise
/// close the element and spill the rest of the index into the document.
pub(super) fn json(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Render CommonMark to HTML with tables, footnotes, strikethrough and task
/// lists enabled, escaping any raw HTML the source contains.
pub(super) fn markdown(source: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    let escaped = Parser::new_ext(source, options)
        .map(neutralize_html)
        .map(retarget_link);
    let mut out = String::new();
    html::push_html(&mut out, escaped);
    out
}

/// Turn a raw-HTML event into the text it was written as. `Html` covers block
/// level, `InlineHtml` inline; neither reaches the writer as markup.
fn neutralize_html(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Event::Text(raw),
        other => other,
    }
}

/// Point an internal link at the file this exporter actually writes.
///
/// The Markdown sources are shared with the 11ty website, which serves pretty
/// URLs (`channel/`) and `.md` sources. A static export has neither: those
/// links resolve to nothing, and a link that looks valid but 404s is worse than
/// no link at all. External links, fragments and mail links are left alone.
fn retarget_link(event: Event<'_>) -> Event<'_> {
    match event {
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from(as_page_link(&dest_url)),
            title,
            id,
        }),
        other => other,
    }
}

/// The exported filename for a Markdown link destination.
fn as_page_link(destination: &str) -> String {
    let external = destination.starts_with("http://")
        || destination.starts_with("https://")
        || destination.starts_with("mailto:")
        || destination.starts_with('#')
        || destination.is_empty();
    if external {
        return destination.to_owned();
    }
    let (path, anchor) = destination
        .split_once('#')
        .map_or((destination, String::new()), |(p, a)| (p, format!("#{a}")));
    let stem = path.strip_suffix('/').unwrap_or(path);
    let stem = stem.strip_suffix(".md").unwrap_or(stem);
    if stem.is_empty() {
        return destination.to_owned();
    }
    if stem.ends_with(".html") {
        format!("{stem}{anchor}")
    } else {
        format!("{stem}.html{anchor}")
    }
}

/// The body of a Markdown page with any leading YAML front matter removed. The
/// Markdown writer adds front matter for the 11ty site; the HTML export renders
/// standalone pages, where that block is not metadata but visible garbage.
pub(super) fn without_front_matter(markdown: &str) -> &str {
    let Some(rest) = markdown.strip_prefix("---\n") else {
        return markdown;
    };
    rest.find("\n---\n")
        .map_or(markdown, |end| &rest[end.saturating_add(5)..])
}

/// Reduce a Markdown fragment to the words it renders as.
///
/// Summaries are reused in three places that cannot render Markdown — the
/// search result list, the landing page, and `<meta name="description">` — and
/// a reader there should see `Signature: map(...)`, not the asterisks and
/// backticks that would have styled it.
pub(super) fn plain(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' | '*' | '_' => {}
            '\\' => out.extend(chars.next()),
            '#' if out.chars().last().is_none_or(char::is_whitespace) => {}
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The first heading of a Markdown body, used when a page has no summary.
pub(super) fn first_paragraph(markdown: &str) -> String {
    plain(&raw_first_paragraph(markdown))
}

fn raw_first_paragraph(markdown: &str) -> String {
    without_front_matter(markdown)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("---"))
        .map(str::to_owned)
        .next()
        .unwrap_or_default()
}

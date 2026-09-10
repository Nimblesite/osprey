//! Markdown rendering and the four escaping contexts an exported page uses.
//! Implements [DOC-EXPORT-HTML].
//!
//! **Raw HTML policy.** Documentation comes from source comments and from
//! user-supplied Markdown files, and both are treated as UNTRUSTED text: raw
//! HTML blocks and inline HTML in Markdown are escaped and shown as literal
//! text rather than passed through. A doc comment must never be able to inject
//! markup or script into the generated site, and a reader is better served by
//! seeing `<script>` than by silently losing it.

use pulldown_cmark::{html, BrokenLink, CowStr, Event, Options, Parser, Tag};
use std::fmt::Write as _;
use std::path::Path;

/// Schemes that run code in the reader's browser. A documentation comment is
/// untrusted text, so a destination in one of these is replaced rather than
/// written into the page: escaping the surrounding markup does not help when
/// the link itself is the payload.
const EXECUTABLE_SCHEMES: [&str; 3] = ["javascript:", "data:", "vbscript:"];

/// What a refused destination becomes: a link the reader can still see and
/// still read the text of, that does nothing when clicked.
const INERT: &str = "#";

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

/// Escape a string as a JSON string body (no surrounding quotes).
///
/// `<` and `&` are escaped as well as the characters JSON requires. The index
/// is written as its own script file today, where they would be harmless, but
/// a `</script>` sequence in a doc comment closes any element the same bytes
/// are ever inlined into — and that is a change nobody would think to re-audit.
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
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out
}

/// Render `CommonMark` to HTML with the extensions a reference page needs,
/// escaping any raw HTML the source contains.
/// `resolve` turns a documentation symbol name into the href of the page
/// documenting it, or `None` where this export documents no such declaration
/// ([DOC-LINK]). Which declaration a name refers to depends on the scope the
/// comment was written in, so the caller owns that decision, not the renderer.
pub(super) fn markdown(source: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let mut callback = |link: BrokenLink<'_>| {
        resolve(link.reference.as_ref()).map(|href| (CowStr::from(href), CowStr::from("")))
    };
    let parser = Parser::new_with_broken_link_callback(source, options(), Some(&mut callback));
    let escaped: Vec<Event<'_>> = parser.map(neutralize_html).map(retarget_link).collect();
    let mut out = String::new();
    html::push_html(&mut out, super::anchors::headings(&escaped).into_iter());
    out
}

fn options() -> Options {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);
    // Lets an author pin a heading's anchor with `{#name}`. Generated anchors
    // move when a heading is reworded; a pinned one is a promise to whoever
    // already linked to it.
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options
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
        // An image destination is not retargeted — it names an asset, not a
        // page — but it is the same injection surface, so it is screened.
        Event::Start(Tag::Image {
            link_type,
            dest_url,
            title,
            id,
        }) => Event::Start(Tag::Image {
            link_type,
            dest_url: inert_scheme(&dest_url).map_or(dest_url, CowStr::from),
            title,
            id,
        }),
        other => other,
    }
}

/// [`INERT`] when `destination` would execute, otherwise `None`.
///
/// Whitespace and control characters are dropped before the comparison: a
/// browser reads `java&#9;script:` in an `href` as `javascript:`, so a check
/// that only matched the literal spelling would let it straight through.
fn inert_scheme(destination: &str) -> Option<String> {
    let normalized: String = destination
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .flat_map(char::to_lowercase)
        .collect();
    EXECUTABLE_SCHEMES
        .iter()
        .any(|scheme| normalized.starts_with(scheme))
        .then(|| INERT.to_owned())
}

/// The exported filename for a Markdown link destination.
fn as_page_link(destination: &str) -> String {
    if let Some(inert) = inert_scheme(destination) {
        return inert;
    }
    if is_external(destination) {
        return destination.to_owned();
    }
    let (path, anchor) = destination
        .split_once('#')
        .map_or((destination, String::new()), |(p, a)| (p, format!("#{a}")));
    let decoded = percent_decode(path);
    let trimmed = decoded.strip_suffix('/').unwrap_or(&decoded);
    page_stem(trimmed).map_or_else(
        || destination.to_owned(),
        |stem| format!("{stem}.html{anchor}"),
    )
}

/// A destination this exporter does not own: another site, another protocol,
/// or a fragment of the page the reader is already on.
fn is_external(destination: &str) -> bool {
    destination.is_empty()
        || destination.starts_with('#')
        || destination.starts_with("//")
        || destination.starts_with("mailto:")
        || destination
            .split_once("://")
            .is_some_and(|(scheme, _)| !scheme.contains('/'))
}

/// The slug this exporter generated for a link destination, or `None` when the
/// destination is not a page at all.
///
/// Additional pages are slugged component by component ([DOC-EXPORT-PAGES]),
/// so `Deep Dive/Getting Started.md` was written as `deep-dive/getting-started`
/// and a link naming the author's own filenames has to be put through the SAME
/// transformation. A destination carrying any other extension is an asset — a
/// diagram, a download — and is left exactly as written.
fn page_stem(path: &str) -> Option<String> {
    let extension = Path::new(path)
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase());
    let body = match extension.as_deref() {
        Some("md" | "html") => path.rsplit_once('.').map(|(head, _)| head)?,
        Some(_) => return None,
        None => path,
    };
    if body.is_empty() {
        return None;
    }
    Some(
        body.split('/')
            .map(slug_component)
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// `.` and `..` are traversal rather than names; anything else becomes the
/// same filesystem-safe stem the exporter used when it wrote the page.
fn slug_component(component: &str) -> String {
    match component {
        "" | "." | ".." => component.to_owned(),
        name => crate::docs::safe_slug(name),
    }
}

/// Decode the percent-escapes a destination may carry, so `Getting Started.md`
/// and `Getting%20Started.md` reach the same page.
fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let escape = bytes
            .get(index.saturating_add(1))
            .zip(bytes.get(index.saturating_add(2)))
            .filter(|_| byte == b'%')
            .and_then(|(high, low)| hex_byte(*high, *low));
        let (decoded, width) = escape.map_or((byte, 1), |decoded| (decoded, 3));
        out.push(decoded);
        index = index.saturating_add(width);
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_owned())
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
    let digit = |byte: u8| char::from(byte).to_digit(16);
    let value = digit(high)?.saturating_mul(16).saturating_add(digit(low)?);
    u8::try_from(value).ok()
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

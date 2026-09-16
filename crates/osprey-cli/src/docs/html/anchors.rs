//! Unique, inert Markdown anchors. Implements [DOC-EXPORT-HTML].

use pulldown_cmark::{CowStr, Event, Tag, TagEnd};
use std::collections::{BTreeMap, BTreeSet};

const PAGE_IDS: &[&str] = &["content", "q", "results", "menu", "tree"];

pub(super) fn headings<'a>(events: &[Event<'a>]) -> Vec<Event<'a>> {
    let reserved = explicit_ids(events);
    let mut used = PAGE_IDS.iter().map(|id| (*id).to_owned()).collect();
    let footnotes = footnote_ids(events, &mut used, &reserved);
    events
        .iter()
        .enumerate()
        .map(|(position, event)| anchor(event, position, events, &mut used, &reserved, &footnotes))
        .collect()
}

fn explicit_ids(events: &[Event<'_>]) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Start(Tag::Heading { id: Some(id), .. }) => Some(id.to_string()),
            _ => None,
        })
        .collect()
}

fn footnote_ids(
    events: &[Event<'_>],
    used: &mut BTreeSet<String>,
    reserved: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for event in events {
        if let Event::Start(Tag::FootnoteDefinition(name)) = event {
            let base = format!("footnote-{}", crate::docs::safe_slug(name));
            let _ = names
                .entry(name.to_string())
                .or_insert_with(|| unique(&base, false, used, reserved));
        }
    }
    names
}

fn anchor<'a>(
    event: &Event<'a>,
    position: usize,
    events: &[Event<'a>],
    used: &mut BTreeSet<String>,
    reserved: &BTreeSet<String>,
    footnotes: &BTreeMap<String, String>,
) -> Event<'a> {
    match event {
        Event::Start(Tag::Heading {
            level, id, classes, ..
        }) => {
            let base = id.as_ref().map_or_else(
                || crate::docs::safe_slug(&heading_text(events, position)),
                ToString::to_string,
            );
            Event::Start(Tag::Heading {
                level: *level,
                id: Some(CowStr::from(unique(&base, id.is_some(), used, reserved))),
                classes: classes.clone(),
                attrs: Vec::new(),
            })
        }
        Event::Start(Tag::FootnoteDefinition(name)) => {
            Event::Start(Tag::FootnoteDefinition(note_name(name, footnotes)))
        }
        Event::FootnoteReference(name) => Event::FootnoteReference(note_name(name, footnotes)),
        other => other.clone(),
    }
}

fn note_name<'a>(name: &CowStr<'a>, names: &BTreeMap<String, String>) -> CowStr<'a> {
    names
        .get(name.as_ref())
        .map_or_else(|| name.clone(), |name| CowStr::from(name.clone()))
}

fn heading_text(events: &[Event<'_>], position: usize) -> String {
    events
        .iter()
        .skip(position.saturating_add(1))
        .take_while(|event| !matches!(event, Event::End(TagEnd::Heading(_))))
        .filter_map(|event| match event {
            Event::Text(text) | Event::Code(text) => Some(text.as_ref()),
            Event::SoftBreak | Event::HardBreak => Some(" "),
            _ => None,
        })
        .collect()
}

fn unique(
    base: &str,
    explicit: bool,
    used: &mut BTreeSet<String>,
    reserved: &BTreeSet<String>,
) -> String {
    let mut candidate = base.to_owned();
    let mut ordinal = 1usize;
    while used.contains(&candidate) || ((!explicit || ordinal > 1) && reserved.contains(&candidate))
    {
        ordinal = ordinal.saturating_add(1);
        candidate = format!("{base}-{ordinal}");
    }
    let _ = used.insert(candidate.clone());
    candidate
}

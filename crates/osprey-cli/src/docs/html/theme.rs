//! The three built-in documentation themes. Implements [DOC-EXPORT-CSS].
//!
//! Each theme is a different design, not a recolouring: `osprey` is a clean
//! blue three-column reading layout, `midnight` a dark console with monospace
//! chrome, and `paper` an editorial serif page ruled instead of boxed. What they
//! share — structure, accessibility and the mobile disclosure — lives in
//! `theme/base.css`, which names no colour. Each theme file supplies the custom
//! properties base reads plus its own identity rules, and loads after base so
//! those rules win. User stylesheets load after both.
//!
//! Every text colour meets a 4.5:1 contrast ratio against the surface it sits
//! on. No web font and no CDN: faces are named local families with system
//! fallbacks, so an export opened offline renders the same.

/// One built-in theme: its name and its identity stylesheet.
struct Theme {
    name: &'static str,
    css: &'static str,
}

/// Structure, accessibility and responsive rules every theme shares.
const BASE: &str = include_str!("theme/base.css");

/// The default theme. Named separately from [`THEMES`] so the fallback in
/// [`css`] is a reference to a known value rather than an index that could
/// panic if the table were ever emptied.
const OSPREY: Theme = Theme {
    name: "osprey",
    css: include_str!("theme/osprey.css"),
};

const THEMES: &[Theme] = &[
    OSPREY,
    Theme {
        name: "midnight",
        css: include_str!("theme/midnight.css"),
    },
    Theme {
        name: "paper",
        css: include_str!("theme/paper.css"),
    },
];

/// The stylesheet for `name`: the shared base, then the theme's identity.
/// Falls back to `osprey` for an unknown name — `Options::parse` already
/// rejects those; this stays total rather than panicking on a caller that has
/// not.
pub(super) fn css(name: &str) -> String {
    let theme = THEMES
        .iter()
        .find(|theme| theme.name == name)
        .unwrap_or(&OSPREY);
    format!("{BASE}\n{}", theme.css)
}

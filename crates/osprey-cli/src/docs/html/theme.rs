//! The three built-in documentation themes. Implements [DOC-EXPORT-CSS].
//!
//! Every theme sets the SAME custom properties and differs only in their
//! values, so the one stylesheet in [`LAYOUT`] lays out all three. Colors are
//! chosen for a contrast ratio of at least 4.5:1 against their background —
//! documentation nobody can read is documentation that was not written.
//!
//! No CDN, no web font, no build step: the export must work from a static
//! server, so every face falls back to the reader's own system stack.

/// The palette for one theme: the custom properties [`LAYOUT`] consumes.
struct Palette {
    name: &'static str,
    scheme: &'static str,
    vars: &'static str,
}

const PALETTES: &[Palette] = &[
    Palette {
        name: "osprey",
        scheme: "light",
        vars: "--bg:#fbfaf7;--surface:#fff;--text:#1f2421;--muted:#5a635c;\
               --accent:#2f6b45;--accent-soft:#e7f0e9;--border:#dcd9d0;\
               --code-bg:#f4f2ec;--mark:#fdf0c8;",
    },
    Palette {
        name: "midnight",
        scheme: "dark",
        vars: "--bg:#12151a;--surface:#181c23;--text:#e6e9ee;--muted:#a3acba;\
               --accent:#7fb5ff;--accent-soft:#1e2836;--border:#2a313c;\
               --code-bg:#1e232b;--mark:#4a3f1d;",
    },
    Palette {
        name: "paper",
        scheme: "light",
        vars: "--bg:#fff;--surface:#fff;--text:#111;--muted:#585858;\
               --accent:#1a4fd6;--accent-soft:#eef2fd;--border:#e0e0e0;\
               --code-bg:#f6f6f6;--mark:#fff2a8;",
    },
];

/// The theme CSS for `name`, falling back to `osprey` for an unknown name.
/// `Options::parse` already rejects unknown themes; this stays total rather
/// than panicking on a caller that has not.
pub(super) fn css(name: &str) -> String {
    let palette = PALETTES
        .iter()
        .find(|palette| palette.name == name)
        .unwrap_or_else(|| &PALETTES[0]);
    format!(
        ":root{{color-scheme:{};{}}}\n{LAYOUT}",
        palette.scheme, palette.vars
    )
}

/// Layout, typography and component rules shared by every theme.
const LAYOUT: &str = "\
*{box-sizing:border-box}\
body{margin:0;background:var(--bg);color:var(--text);\
font:16px/1.65 system-ui,-apple-system,'Segoe UI',Roboto,Helvetica,Arial,sans-serif}\
a{color:var(--accent)}a:hover{text-decoration:none}\
:focus-visible{outline:3px solid var(--accent);outline-offset:2px}\
.skip{position:absolute;left:-9999px}\
.skip:focus{left:8px;top:8px;z-index:10;padding:8px 12px;background:var(--surface);border:1px solid var(--border);border-radius:6px}\
.shell{display:grid;grid-template-columns:270px minmax(0,1fr);gap:32px;\
max-width:1180px;margin:0 auto;padding:24px 20px}\
.side{position:sticky;top:24px;align-self:start;max-height:calc(100vh - 48px);overflow:auto}\
.brand{font-weight:700;font-size:18px;text-decoration:none;color:var(--text);display:block;margin-bottom:14px}\
.search{width:100%;padding:9px 11px;border:1px solid var(--border);border-radius:8px;\
background:var(--surface);color:var(--text);font:inherit;font-size:14px}\
.group{margin:18px 0 6px;font-size:11px;letter-spacing:.09em;text-transform:uppercase;color:var(--muted)}\
.nav{list-style:none;margin:0;padding:0}\
.nav a{display:block;padding:5px 9px;border-radius:6px;text-decoration:none;font-size:14px}\
.nav a:hover{background:var(--accent-soft)}\
.nav a[aria-current=page]{background:var(--accent-soft);font-weight:600}\
.note{color:var(--muted);font-size:14px;padding:6px 9px}\
main{min-width:0;background:var(--surface);border:1px solid var(--border);\
border-radius:12px;padding:28px 32px}\
main :first-child{margin-top:0}\
h1{font-size:30px;line-height:1.25}h2{font-size:22px;margin-top:1.8em}h3{font-size:17px}\
h1,h2,h3,h4{scroll-margin-top:20px}\
h2,h3{border-bottom:1px solid var(--border);padding-bottom:.25em}\
code{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.9em;\
background:var(--code-bg);padding:.15em .35em;border-radius:4px}\
pre{background:var(--code-bg);border:1px solid var(--border);border-radius:8px;\
padding:14px 16px;overflow-x:auto}\
pre code{background:none;padding:0;font-size:13.5px;line-height:1.55}\
blockquote{margin:1em 0;padding:.4em 1em;border-left:3px solid var(--accent);color:var(--muted)}\
table{border-collapse:collapse;width:100%;display:block;overflow-x:auto}\
th,td{border:1px solid var(--border);padding:7px 10px;text-align:left}\
th{background:var(--accent-soft)}\
mark{background:var(--mark);color:inherit}\
.summary{color:var(--muted);margin-top:-.5em}\
.crumb{font-size:13px;color:var(--muted);margin-bottom:10px}\
.hits{list-style:none;margin:10px 0 0;padding:0}\
.hits li{margin:0 0 8px}.hits a{font-weight:600}\
.hits p{margin:2px 0 0;color:var(--muted);font-size:14px}\
@media(max-width:860px){\
.shell{grid-template-columns:1fr;gap:18px;padding:16px 14px}\
.side{position:static;max-height:none}\
/* The sidebar precedes the content in source order, which is right for a \
   screen reader but would otherwise make a phone reader scroll past every \
   page in the reference before reaching the one they opened. Bounding the \
   tree keeps search at the top and the article within the first screen. */\
#tree{max-height:34vh;overflow:auto;border:1px solid var(--border);\
border-radius:8px;padding:4px 6px}\
main{padding:20px 18px;border-radius:10px}\
h1{font-size:25px}}\
@media(prefers-reduced-motion:no-preference){html{scroll-behavior:smooth}}\
";

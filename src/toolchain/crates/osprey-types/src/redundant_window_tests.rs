//! The parallel search reports exactly the set the one-at-a-time greedy search
//! does, whatever the window ([TYPE-ANNOTATION-REDUNDANT]).

use crate::redundant::Snapshot;
use crate::redundant::{accumulate, accumulate_candidates, individually_removable, published};
use crate::redundant_sites::{sites, Site};
use osprey_syntax::{parse_program_with_flavor, Flavor};

/// Removable, load-bearing and mutually pinning annotations, interleaved.
pub(crate) const MIXED: &str = "type Point = { x: int, y: int }\n\
    fn px(p: Point) -> int = p.x\n\
    fn identity<T>(x: T) -> T = x\n\
    fn pick(a: int, b: int) -> int = match a <= b {\n  true => a\n  false => b\n}\n\
    fn greet(name: string) -> string = \"hi \" + name\n\
    fn tag(label: string, count: int) -> string = label + toString(count)\n\
    let v = px(Point { x: 1, y: 2 })\n\
    let i = identity(42)\n\
    let chosen = pick(1, 2)\n";

/// A fixture program's published baseline, its written sites and their greedy set.
fn fixture(program: &osprey_ast::Program) -> (Snapshot, Vec<Site>, Vec<usize>) {
    let baseline = published(program).expect("fixture checks");
    let written = sites(program);
    let greedy = accumulate(program, &baseline, &written, 1);
    (baseline, written, greedy)
}

/// A reported set the tests pin: non-empty and strictly narrower than the sites written.
fn assert_strict_subset(subject: &[usize], written: usize, what: &str) {
    assert!(
        !subject.is_empty() && subject.len() < written,
        "{what}: {subject:?} of {written}"
    );
}

#[test]
fn every_window_keeps_the_greedy_set() {
    let program = parse_program_with_flavor(MIXED, Flavor::Default).program;
    let (baseline, written, greedy) = fixture(&program);
    assert_strict_subset(&greedy, written.len(), "fixture must mix kept and dropped sites");
    for window in [2, 3, 64] {
        let found = accumulate(&program, &baseline, &written, window);
        assert_eq!(found, greedy, "window {window}");
    }
}

#[test]
fn singleton_prefilter_keeps_the_greedy_set() {
    let program = parse_program_with_flavor(MIXED, Flavor::Default).program;
    let (baseline, written, greedy) = fixture(&program);
    let candidates = individually_removable(&program, &baseline, &written);
    assert_strict_subset(&candidates, written.len(), "prefilter must drop a load-bearing site");
    let found = accumulate_candidates(&program, &baseline, &written, &candidates, 1);
    assert_eq!(found, greedy, "singleton prefilter changed the greedy set");
}

/// [TYPE-ANNOTATION-REDUNDANT] Blame only steers the search, so the default path
/// reports what one-at-a-time source order reports, including across declarations
/// that pin each other and signatures written on a line of their own.
#[test]
fn the_directed_search_keeps_the_source_order_set() {
    let cases = [
        (Flavor::Default, MIXED),
        (
            Flavor::Default,
            "fn even(n: int) -> bool = match n == 0 {\n  true => true\n  false => odd(n - 1)\n}\n\
             fn odd(n: int) -> bool = match n == 0 {\n  true => false\n  false => even(n - 1)\n}\n\
             fn unused(x: int) = 0\nfn kept() -> string = \"hi\"\n",
        ),
        (
            Flavor::Default,
            "fn pick(a: int) -> int = hold(a)\nfn hold(a: int) -> int = a\nlet v: int = pick(1)\n",
        ),
        (
            Flavor::Default,
            "effect Log { say: fn(string) -> int }\n\
             fn talk(m: string) -> int !Log = perform Log.say(m)\n\
             fn plain(m: string) -> string = m\n\
             fn main() -> int = {\n  handle Log {\n    say m => 1\n  }\n  talk(plain(\"x\"))\n}\n",
        ),
        (
            Flavor::Default,
            "effect Read<T> { get: fn() -> T }\n\
             fn need() -> int !Read<int> = perform Read.get()\n\
             fn pad(x: int) = 0\n\
             fn main() -> int = {\n  handle Read {\n    get => 7\n  }\n  need()\n}\n",
        ),
        (
            Flavor::Ml,
            "tag : string -> string\ntag s = s\n\
             pad : int -> int\npad n = 0\n\
             first : int -> int\nfirst n = second n\n\
             second : int -> int\nsecond n = first n\n",
        ),
    ];
    for (flavor, source) in cases {
        let parsed = parse_program_with_flavor(source, flavor);
        assert_eq!(parsed.errors, Vec::new(), "{source}");
        let (_, written, greedy) = fixture(&parsed.program);
        assert!(!written.is_empty(), "{source}");
        let greedy: Vec<_> = greedy
            .into_iter()
            .filter_map(|index| written.get(index))
            .map(|site| site.source)
            .collect();
        let found: Vec<_> = crate::redundant::redundant_annotation_sites(&parsed.program)
            .into_iter()
            .map(|site| site.annotation_position)
            .collect();
        assert_eq!(found, greedy, "{source}");
    }
}

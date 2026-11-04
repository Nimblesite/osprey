//! The parallel search reports exactly the set the one-at-a-time greedy search
//! does, whatever the window ([TYPE-ANNOTATION-REDUNDANT]).

use crate::redundant::{accumulate, accumulate_candidates, individually_removable, published};
use crate::redundant_sites::sites;
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

#[test]
fn every_window_keeps_the_greedy_set() {
    let program = parse_program_with_flavor(MIXED, Flavor::Default).program;
    let baseline = published(&program).expect("fixture checks");
    let written = sites(&program);
    let greedy = accumulate(&program, &baseline, &written, 1);
    assert!(
        !greedy.is_empty() && greedy.len() < written.len(),
        "fixture must mix kept and dropped sites: {greedy:?} of {}",
        written.len()
    );
    for window in [2, 3, 64] {
        assert_eq!(
            accumulate(&program, &baseline, &written, window),
            greedy,
            "window {window}"
        );
    }
}

#[test]
fn singleton_prefilter_keeps_the_greedy_set() {
    let program = parse_program_with_flavor(MIXED, Flavor::Default).program;
    let baseline = published(&program).expect("fixture checks");
    let written = sites(&program);
    let greedy = accumulate(&program, &baseline, &written, 1);
    let candidates = individually_removable(&program, &baseline, &written);
    assert!(
        !candidates.is_empty() && candidates.len() < written.len(),
        "prefilter must drop a load-bearing site: {candidates:?} of {}",
        written.len()
    );
    assert_eq!(
        accumulate_candidates(&program, &baseline, &written, &candidates, 1),
        greedy,
        "singleton prefilter changed the greedy set"
    );
}

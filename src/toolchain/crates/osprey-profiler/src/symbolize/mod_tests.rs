//! Unit tests of [PROF-SYMBOLIZE-OFFLINE] address mapping and caching.
use super::*;
/// An image loaded at `base` (after `slide`) whose executable range spans
/// `text .. text + text_size`. Every case here varies only these five
/// fields, so the zeroed remainder is written once instead of per literal.
fn image(path: &str, base: u64, slide: u64, text: u64, text_size: u64) -> Image {
    Image {
        path: path.to_owned(),
        base,
        slide,
        text,
        text_size,
        arch: String::new(),
    }
}

use crate::raw::{Sample, Thread};
use std::cell::RefCell;

#[test]
fn frame_kind_comes_from_the_file_extension() {
    assert_eq!(
        SymFrame::new("fib", "/src/fib.osp", 3).kind,
        FrameKind::User
    );
    assert_eq!(
        SymFrame::new("fib", "/src/FIB.OSP", 3).kind,
        FrameKind::User
    );
    assert_eq!(
        SymFrame::new("fib", "/src/fib.ospml", 3).kind,
        FrameKind::User
    );
    assert_eq!(
        SymFrame::new("fiber_thread_func", "/rt/fiber.c", 9).kind,
        FrameKind::Runtime
    );
    assert_eq!(SymFrame::new("anon", "", 0).kind, FrameKind::Runtime);
    assert_eq!(FrameKind::User.as_str(), "user");
    assert_eq!(FrameKind::Runtime.as_str(), "runtime");
}

#[test]
fn hex_frames_format_the_address() {
    let frame = SymFrame::hex(0x1f);
    assert_eq!(frame.name, "0x1f");
    assert_eq!(
        (frame.file.as_str(), frame.line, frame.kind),
        ("", 0, FrameKind::Runtime)
    );
}

#[test]
fn leaf_frames_are_unslid_without_the_call_adjustment() {
    assert_eq!(adjusted_unslid(4_301_231, 0, 12_345), 4_288_886);
}

#[test]
fn return_addresses_subtract_one_before_unsliding() {
    assert_eq!(adjusted_unslid(4_301_100, 1, 12_345), 4_288_754);
    assert_eq!(adjusted_unslid(4_301_100, 7, 12_345), 4_288_754);
}

#[test]
fn address_adjustment_saturates_at_zero() {
    assert_eq!(adjusted_unslid(0, 1, 0), 0);
    assert_eq!(adjusted_unslid(5, 0, 9), 0);
}

#[test]
fn pcs_fall_back_to_greatest_base_when_no_image_reports_a_text_range() {
    let images = vec![image("/a", 100, 0, 0, 0), image("/b", 500, 0, 0, 0)];
    assert_eq!(image_index_for(&images, 499), Some(0));
    assert_eq!(image_index_for(&images, 500), Some(1));
    assert_eq!(image_index_for(&images, 99), None);
}

/// [PROF-SYMBOLIZE-OFFLINE] Ownership is decided by the executable range,
/// NOT by mach-header order.
///
/// This is the dyld shared cache's real layout: every system dylib's header
/// lives in one region and its `__TEXT` in another, so header order and text
/// order disagree. Here `/usr/lib/system/libsystem_kernel` has the greater
/// header base, while the pc is inside `libsystem_malloc`'s text. Greatest
/// `base <= pc` answers "kernel" — which is exactly how a `_xzm_free` leaf
/// was reported as `task_get_special_port`.
#[test]
fn a_shared_cache_pc_is_owned_by_the_image_whose_text_contains_it() {
    let images = vec![
        image(
            "/usr/lib/system/libsystem_malloc.dylib",
            0x1_8000_0000,
            0,
            0x1_8030_0000,
            0x2_0000,
        ),
        image(
            "/usr/lib/system/libsystem_kernel.dylib",
            0x1_8010_0000,
            0,
            0x1_8040_0000,
            0x2_0000,
        ),
    ];
    // The observed leaf: inside libsystem_malloc's __TEXT, but BELOW the
    // kernel image's header, which is what the old rule keyed on.
    assert_eq!(image_index_for(&images, 0x1_8030_F483), Some(0));
    assert_eq!(image_index_for(&images, 0x1_8040_1000), Some(1));
    // Between the two text ranges: owned by neither, so the fallback picks
    // the greatest header at or below rather than inventing containment.
    assert_eq!(image_index_for(&images, 0x1_8035_0000), Some(1));
}

/// A reported range is half-open: its last byte is inside, the byte after
/// the end is not.
#[test]
fn a_reported_text_range_is_half_open() {
    let image = image("/bin/app", 0, 0, 0x4000, 0x1000);
    assert!(image.text_contains(0x4000));
    assert!(image.text_contains(0x4FFF));
    assert!(!image.text_contains(0x5000));
    assert!(!image.text_contains(0x3FFF));
}

/// Records every address batch it is asked to resolve.
#[derive(Debug, Default)]
struct RecordingSymbolizer {
    calls: RefCell<Vec<Vec<u64>>>,
}

impl Symbolize for RecordingSymbolizer {
    fn symbolize(&self, _: &Path, addrs: &[u64]) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
        self.calls.borrow_mut().push(addrs.to_vec());
        Ok(addrs
            .iter()
            .map(|&a| vec![SymFrame::new(&format!("fn_{a}"), "/src/app.osp", 1)])
            .collect())
    }
}

fn two_image_profile() -> Profile {
    Profile {
        rate_hz: 997,
        dropped: 0,
        images: vec![
            image("/bin/app", 1000, 100, 0, 0),
            image("/usr/lib/sys", 9000, 0, 0, 0),
        ],
        threads: vec![Thread {
            fiber: 0,
            label: "main".to_owned(),
        }],
        stacks: vec![vec![1500, 1501, 9200], vec![1500, 500]],
        samples: vec![Sample {
            t_ns: 0,
            thread: 0,
            stack: 0,
            on_cpu: true,
        }],
    }
}

#[test]
fn stacks_resolve_through_per_image_batches_with_dedup() {
    // Image mapping, unsliding, return-address adjustment, batching, and
    // hex fallback from [PROF-SYMBOLIZE-OFFLINE].
    let profile = two_image_profile();
    let sym = RecordingSymbolizer::default();
    let stacks = symbolize_stacks(&profile, &sym).unwrap();
    // Leaf 1500 -> 1400; return 1501 -> 1501-1-100 = 1400 (dedup with the
    // leaf); return 9200 (image 2, slide 0) -> 9199.
    let calls = sym.calls.borrow();
    assert_eq!(calls.as_slice(), &[vec![1400], vec![9199]]);
    let first = stacks.first().unwrap();
    assert_eq!(
        first.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["fn_1400", "fn_1400", "fn_9199"]
    );
    // pc 500 sits below every image base -> raw hex fallback.
    assert_eq!(stacks.get(1).unwrap().get(1).unwrap().name, "0x1f4");
}

#[test]
fn inline_chains_splice_innermost_first_into_leaf_first_stacks() {
    let profile = two_image_profile();
    let mut fixed = FixedSymbolizer::default();
    // Leaf 1500 and return 1501 both adjust to 1400 (image 1, slide
    // 100): a 2-deep inline chain, innermost first.
    let _ = fixed.map.insert(
        1400,
        vec![
            SymFrame::new("inlined", "/src/app.osp", 4),
            SymFrame::new("outer", "/src/app.osp", 9),
        ],
    );
    let stacks = symbolize_stacks(&profile, &fixed).unwrap();
    let names: Vec<&str> = stacks
        .first()
        .unwrap()
        .iter()
        .map(|f| f.name.as_str())
        .collect();
    // Each pc's chain replaces it in place; 9200 adjusts to the
    // unmapped 9199 and hexes.
    assert_eq!(names, ["inlined", "outer", "inlined", "outer", "0x23ef"]);
}

#[test]
fn short_symbolizer_replies_fall_back_to_hex() {
    /// Always returns an empty batch, whatever it is asked.
    #[derive(Debug)]
    struct Silent;
    impl Symbolize for Silent {
        fn symbolize(&self, _: &Path, _: &[u64]) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
            Ok(Vec::new())
        }
    }
    let stacks = symbolize_stacks(&two_image_profile(), &Silent).unwrap();
    assert!(stacks
        .first()
        .unwrap()
        .iter()
        .all(|f| f.name.starts_with("0x")));
}

#[test]
fn empty_chains_fall_back_to_hex() {
    /// Answers every address with an EMPTY chain.
    #[derive(Debug)]
    struct Hollow;
    impl Symbolize for Hollow {
        fn symbolize(&self, _: &Path, a: &[u64]) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
            Ok(vec![Vec::new(); a.len()])
        }
    }
    let stacks = symbolize_stacks(&two_image_profile(), &Hollow).unwrap();
    let first = stacks.first().unwrap();
    assert_eq!(first.len(), 3);
    assert!(first.iter().all(|f| f.name.starts_with("0x")));
}

#[test]
fn fixed_symbolizer_maps_known_addresses_and_hexes_the_rest() {
    let mut fixed = FixedSymbolizer::default();
    let _ = fixed
        .map
        .insert(7, vec![SymFrame::new("seven", "/s.osp", 2)]);
    let chains = fixed.symbolize(Path::new("/bin/app"), &[7, 8]).unwrap();
    assert_eq!(chains.first().unwrap().first().unwrap().name, "seven");
    assert_eq!(chains.get(1).unwrap().first().unwrap().name, "0x8");
}

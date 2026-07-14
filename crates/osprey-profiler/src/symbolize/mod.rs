//! Symbolization: raw pc → inline-expanded chain of `(name, file, line)`
//! frames. Implements [PROF-SYMBOLIZE-OFFLINE]: each pc is mapped to the
//! image with the greatest `base <= pc`; return addresses (every frame
//! except the leaf) are adjusted by −1 BEFORE unsliding so samples attribute
//! to the call line; each unique adjusted address is symbolized exactly
//! once. Each pc's inline chain (innermost-first) is spliced into the
//! leaf-first stack in place, so the innermost inline frame is the leaf.

pub(crate) mod tools;

use crate::raw::{Image, Profile};
use crate::ProfileError;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// Whether a frame belongs to user `.osp` code or the C runtime / system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FrameKind {
    /// The frame's file is an `.osp` source file.
    User,
    /// Runtime/system frames (C runtime, libsystem, unresolved hex names).
    #[default]
    Runtime,
}

impl FrameKind {
    /// The summary-export spelling: `"user"` or `"runtime"`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Runtime => "runtime",
        }
    }
}

/// One symbolized frame. `line == 0` means the line is unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SymFrame {
    /// Function name (or `0x…` hex when unresolved).
    pub name: String,
    /// Source file path; empty when unknown.
    pub file: String,
    /// 1-based source line; 0 when unknown.
    pub line: u32,
    /// User (`.osp`) vs runtime classification, derived from `file`.
    pub kind: FrameKind,
}

impl SymFrame {
    /// Build a frame, classifying it from the file extension.
    pub(crate) fn new(name: &str, file: &str, line: u32) -> Self {
        let user = Path::new(file)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("osp"));
        let kind = if user {
            FrameKind::User
        } else {
            FrameKind::Runtime
        };
        Self {
            name: name.to_owned(),
            file: file.to_owned(),
            line,
            kind,
        }
    }

    /// The no-symbolizer fallback: a bare hex name, runtime kind.
    pub(crate) fn hex(addr: u64) -> Self {
        Self {
            name: format!("{addr:#x}"),
            file: String::new(),
            line: 0,
            kind: FrameKind::Runtime,
        }
    }
}

/// Resolves batches of unslid addresses within one image to inline-expanded
/// frame chains. The trait boundary keeps every transform in this crate
/// testable without external tools.
pub(crate) trait Symbolize {
    /// Symbolize `unslid_addrs` (on-disk image addresses) against `image`.
    /// One output CHAIN per input address, in order; each chain lists the
    /// frames at that address innermost-first (inline expansion).
    fn symbolize(
        &self,
        image: &Path,
        unslid_addrs: &[u64],
    ) -> Result<Vec<Vec<SymFrame>>, ProfileError>;
}

/// Test double: a fixed unslid-address → chain map; unmapped addresses fall
/// back to single hex-frame chains exactly like the real symbolizers.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct FixedSymbolizer {
    /// unslid address → innermost-first frame chain.
    pub map: BTreeMap<u64, Vec<SymFrame>>,
}

#[cfg(test)]
impl Symbolize for FixedSymbolizer {
    fn symbolize(
        &self,
        _image: &Path,
        unslid_addrs: &[u64],
    ) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
        Ok(unslid_addrs
            .iter()
            .map(|addr| {
                self.map
                    .get(addr)
                    .cloned()
                    .unwrap_or_else(|| vec![SymFrame::hex(*addr)])
            })
            .collect())
    }
}

/// Return-address adjustment [PROF-SYMBOLIZE-OFFLINE]: frame 0 is the precise
/// interrupted pc (`pc - slide`); every deeper frame is a RETURN address and
/// must attribute to the call site (`pc - 1 - slide`).
pub(crate) fn adjusted_unslid(pc: u64, frame_index: usize, slide: u64) -> u64 {
    pc.saturating_sub(u64::from(frame_index > 0))
        .saturating_sub(slide)
}

/// The image owning `pc`: the one with the greatest `base <= pc`.
fn image_index_for(images: &[Image], pc: u64) -> Option<usize> {
    images
        .iter()
        .enumerate()
        .filter(|(_, image)| image.base <= pc)
        .max_by_key(|(_, image)| image.base)
        .map(|(index, _)| index)
}

/// Cache: (image index, adjusted unslid address) → innermost-first chain.
type ChainCache = BTreeMap<(usize, u64), Vec<SymFrame>>;

/// Symbolize every stack of the profile, batching each image's unique
/// adjusted addresses through the symbolizer exactly once.
pub(crate) fn symbolize_stacks(
    profile: &Profile,
    sym: &dyn Symbolize,
) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
    let cache = build_cache(profile, sym)?;
    Ok(profile
        .stacks
        .iter()
        .map(|stack| symbolize_stack(profile, &cache, stack))
        .collect())
}

/// Collect unique `(image, adjusted address)` pairs and symbolize each once.
fn build_cache(profile: &Profile, sym: &dyn Symbolize) -> Result<ChainCache, ProfileError> {
    let mut wanted: BTreeMap<usize, BTreeSet<u64>> = BTreeMap::new();
    for stack in &profile.stacks {
        for (index, &pc) in stack.iter().enumerate() {
            if let Some(image) = image_index_for(&profile.images, pc) {
                let slide = profile.images.get(image).map_or(0, |i| i.slide);
                let _ = wanted
                    .entry(image)
                    .or_default()
                    .insert(adjusted_unslid(pc, index, slide));
            }
        }
    }
    let mut cache = ChainCache::new();
    for (image_index, addrs) in wanted {
        let Some(image) = profile.images.get(image_index) else {
            continue;
        };
        let list: Vec<u64> = addrs.into_iter().collect();
        let chains = sym.symbolize(Path::new(&image.path), &list)?;
        for (addr, chain) in list.into_iter().zip(chains) {
            let _ = cache.insert((image_index, addr), chain);
        }
    }
    Ok(cache)
}

/// Resolve one leaf-first stack through the cache, splicing each pc's inline
/// chain (innermost-first) in place — the result stays leaf-first, with the
/// innermost inline frame as the leaf.
fn symbolize_stack(profile: &Profile, cache: &ChainCache, stack: &[u64]) -> Vec<SymFrame> {
    stack
        .iter()
        .enumerate()
        .flat_map(|(index, &pc)| lookup_chain(profile, cache, index, pc))
        .collect()
}

/// Cache lookup for one pc's chain; anything unmapped (or an empty chain)
/// becomes a single hex frame.
fn lookup_chain(profile: &Profile, cache: &ChainCache, index: usize, pc: u64) -> Vec<SymFrame> {
    image_index_for(&profile.images, pc)
        .and_then(|image| {
            let slide = profile.images.get(image)?.slide;
            cache
                .get(&(image, adjusted_unslid(pc, index, slide)))
                .cloned()
        })
        .filter(|chain| !chain.is_empty())
        .unwrap_or_else(|| vec![SymFrame::hex(pc)])
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn pcs_map_to_the_image_with_greatest_base_at_or_below() {
        let images = vec![
            Image {
                path: "/a".to_owned(),
                base: 100,
                slide: 0,
            },
            Image {
                path: "/b".to_owned(),
                base: 500,
                slide: 0,
            },
        ];
        assert_eq!(image_index_for(&images, 499), Some(0));
        assert_eq!(image_index_for(&images, 500), Some(1));
        assert_eq!(image_index_for(&images, 99), None);
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
                Image {
                    path: "/bin/app".to_owned(),
                    base: 1000,
                    slide: 100,
                },
                Image {
                    path: "/usr/lib/sys".to_owned(),
                    base: 9000,
                    slide: 0,
                },
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
}

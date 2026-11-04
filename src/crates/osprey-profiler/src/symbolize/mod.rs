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

/// Whether a frame belongs to user Osprey code or the C runtime / system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum FrameKind {
    /// The frame's file is an `.osp` or `.ospml` source file.
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
    /// User (`.osp`/`.ospml`) vs runtime classification, derived from `file`.
    pub kind: FrameKind,
}

impl SymFrame {
    /// Build a frame, classifying it from the file extension.
    pub(crate) fn new(name: &str, file: &str, line: u32) -> Self {
        let user = Path::new(file).extension().is_some_and(|ext| {
            ext.eq_ignore_ascii_case("osp") || ext.eq_ignore_ascii_case("ospml")
        });
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
    fn hex(addr: u64) -> Self {
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
    pub(crate) map: BTreeMap<u64, Vec<SymFrame>>,
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
fn adjusted_unslid(pc: u64, frame_index: usize, slide: u64) -> u64 {
    pc.saturating_sub(u64::from(frame_index > 0))
        .saturating_sub(slide)
}

/// The image owning `pc`: the one whose executable range contains it, else the
/// greatest `base <= pc` [PROF-SYMBOLIZE-OFFLINE].
///
/// Containment first, because `base` ordering is WRONG for the dyld shared
/// cache. `base` is the mach-header address; the cache stores every system
/// dylib's header in one region and its `__TEXT` in another, so the image with
/// the nearest lower header is not the image the pc is executing in. A
/// `libsystem_malloc` leaf was being handed to a neighbouring image and
/// symbolizing as a function it never entered.
///
/// The `base` arm stays for producers that report no executable range (older
/// captures, and any platform whose dumper cannot supply one): a worse answer
/// beats no answer, and the fixtures that predate the range still resolve.
fn image_index_for(images: &[Image], pc: u64) -> Option<usize> {
    images
        .iter()
        .position(|image| image.text_contains(pc))
        .or_else(|| {
            images
                .iter()
                .enumerate()
                .filter(|(_, image)| image.base <= pc)
                .max_by_key(|(_, image)| image.base)
                .map(|(index, _)| index)
        })
}

/// Cache: (image index, adjusted unslid address) → innermost-first chain.
pub(crate) type ChainCache = BTreeMap<(usize, u64), Vec<SymFrame>>;

/// Symbolize every stack of the profile, batching each image's unique
/// adjusted addresses through the symbolizer exactly once.
pub(crate) fn symbolize_stacks(
    profile: &Profile,
    sym: &dyn Symbolize,
) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
    let mut cache = ChainCache::new();
    extend_cache(&mut cache, &profile.images, &profile.stacks, sym)?;
    Ok(profile
        .stacks
        .iter()
        .map(|stack| symbolize_stack(&profile.images, &cache, stack))
        .collect())
}

/// Symbolize every `(image, adjusted address)` of `stacks` the cache lacks,
/// batching each image's addresses through the symbolizer once. A live session
/// calls this per batch, so an address is only ever resolved the first time.
pub(crate) fn extend_cache(
    cache: &mut ChainCache,
    images: &[Image],
    stacks: &[Vec<u64>],
    sym: &dyn Symbolize,
) -> Result<(), ProfileError> {
    extend_cache_where(cache, images, stacks, sym, |_| true)
}

/// [`extend_cache`] for the addresses of only the images `wanted` accepts.
pub(crate) fn extend_cache_where(
    cache: &mut ChainCache,
    images: &[Image],
    stacks: &[Vec<u64>],
    sym: &dyn Symbolize,
    wanted: impl Fn(&Image) -> bool,
) -> Result<(), ProfileError> {
    for (image_index, addrs) in missing_addresses(cache, images, stacks) {
        let Some(image) = images.get(image_index).filter(|image| wanted(image)) else {
            continue;
        };
        let list: Vec<u64> = addrs.into_iter().collect();
        let chains = sym.symbolize(Path::new(&image.path), &list)?;
        for (addr, chain) in list.into_iter().zip(chains) {
            let _ = cache.insert((image_index, addr), chain);
        }
    }
    Ok(())
}

/// Per image, the adjusted addresses of `stacks` that `cache` does not hold.
fn missing_addresses(
    cache: &ChainCache,
    images: &[Image],
    stacks: &[Vec<u64>],
) -> BTreeMap<usize, BTreeSet<u64>> {
    let mut wanted: BTreeMap<usize, BTreeSet<u64>> = BTreeMap::new();
    let keys = stacks.iter().flat_map(|stack| {
        stack
            .iter()
            .enumerate()
            .filter_map(|(index, &pc)| cache_key(images, index, pc))
    });
    for (image, addr) in keys.filter(|key| !cache.contains_key(key)) {
        let _ = wanted.entry(image).or_default().insert(addr);
    }
    wanted
}

/// Frame `index` of a stack holding `pc`: the image it belongs to and its
/// adjusted unslid address, or `None` when no image owns it.
fn cache_key(images: &[Image], index: usize, pc: u64) -> Option<(usize, u64)> {
    let image = image_index_for(images, pc)?;
    let slide = images.get(image)?.slide;
    Some((image, adjusted_unslid(pc, index, slide)))
}

/// Resolve one leaf-first stack through the cache, splicing each pc's inline
/// chain (innermost-first) in place — the result stays leaf-first, with the
/// innermost inline frame as the leaf.
fn symbolize_stack(images: &[Image], cache: &ChainCache, stack: &[u64]) -> Vec<SymFrame> {
    stack
        .iter()
        .enumerate()
        .flat_map(|(index, &pc)| lookup_chain(images, cache, index, pc))
        .collect()
}

/// Cache lookup for frame `index` = `pc`; anything unmapped (or an empty
/// chain) becomes a single hex frame.
pub(crate) fn lookup_chain(
    images: &[Image],
    cache: &ChainCache,
    index: usize,
    pc: u64,
) -> Vec<SymFrame> {
    cache_key(images, index, pc)
        .and_then(|key| cache.get(&key).cloned())
        .filter(|chain| !chain.is_empty())
        .unwrap_or_else(|| vec![SymFrame::hex(pc)])
}

/// The image owning `pc`, for naming the module a frame came from.
pub(crate) fn image_of(images: &[Image], pc: u64) -> Option<&Image> {
    image_index_for(images, pc).and_then(|index| images.get(index))
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;

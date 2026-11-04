//! Incremental symbolization for a live-attach session [PROF-ATTACH-PROTOCOL].
//!
//! A live session's stacks arrive batch by batch while the program runs, so
//! each address is resolved once and remembered across batches, against the
//! image table the runtime last reported. A changed table — a library the
//! program loaded mid-session — starts the cache afresh, because the cache is
//! keyed by image index [PROF-SYMBOLIZE-OFFLINE].

use crate::raw::Image;
use crate::symbolize::tools::{is_main_image, LlvmSymbolizer};
use crate::symbolize::{self, ChainCache, FrameKind, SymFrame, Symbolize};
use std::fmt;
use std::path::{Path, PathBuf};

/// One resolved frame of a live stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveFrame {
    /// Function name, or `0x…` when no symbolizer could name the address.
    pub name: String,
    /// Source file, when the debug info names one.
    pub file: Option<String>,
    /// 1-based source line, when known.
    pub line: Option<u32>,
    /// Osprey source (`.osp`/`.ospml`) rather than the C runtime or the system.
    pub user: bool,
    /// File name of the image the address belongs to, when an image owns it.
    pub module: Option<String>,
}

/// Builds the symbolizer for an image table; tests substitute a fixed one.
type ToolFactory = fn(&Path, &[Image]) -> Box<dyn Symbolize + Send>;

/// Address-to-frame resolution that remembers every answer for one session.
pub struct LiveSymbols {
    exe: PathBuf,
    images: Vec<Image>,
    cache: ChainCache,
    tool: Box<dyn Symbolize + Send>,
    make_tool: ToolFactory,
}

impl fmt::Debug for LiveSymbols {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LiveSymbols")
            .field("exe", &self.exe)
            .field("images", &self.images.len())
            .field("cached", &self.cache.len())
            .finish_non_exhaustive()
    }
}

fn llvm_tool(exe: &Path, images: &[Image]) -> Box<dyn Symbolize + Send> {
    Box::new(LlvmSymbolizer::new(exe, images))
}

impl LiveSymbols {
    /// Resolve addresses of the program at `exe` against its `images` with
    /// `llvm-symbolizer`, falling back to `atos` and then to hex names.
    #[must_use]
    pub fn new(exe: &Path, images: Vec<Image>) -> Self {
        Self::with_tool(exe, images, llvm_tool)
    }

    fn with_tool(exe: &Path, images: Vec<Image>, make_tool: ToolFactory) -> Self {
        Self {
            exe: exe.to_path_buf(),
            tool: make_tool(exe, &images),
            images,
            cache: ChainCache::new(),
            make_tool,
        }
    }

    /// The runtime reported a new image table; every remembered answer is
    /// keyed by an index into the old one, so all of them are forgotten.
    pub fn replace_images(&mut self, images: Vec<Image>) {
        self.tool = (self.make_tool)(&self.exe, &images);
        self.images = images;
        self.cache.clear();
    }

    /// The leaf-first frames of each leaf-first stack of raw pcs. Frame 0 of a
    /// stack is the precise pc; every deeper one is a return address and is
    /// attributed to its call line. A live session never fails on a symbolizer:
    /// an address it could not resolve stays uncached, so it comes back as a
    /// hex name now and is asked for again with the next batch.
    pub fn resolve(&mut self, stacks: &[Vec<u64>]) -> Vec<Vec<LiveFrame>> {
        drop(symbolize::extend_cache(
            &mut self.cache,
            &self.images,
            stacks,
            self.tool.as_ref(),
        ));
        stacks.iter().map(|stack| self.frames(stack)).collect()
    }

    /// [`Self::resolve`] for a profile that keeps only Osprey source frames.
    /// Source lives only in the program's own image, so another image's
    /// addresses are named only for a stack that reaches no source: every
    /// other stack drops those frames, and asking `atos` for them is what
    /// stalled a live session on a busy machine [PROF-LSP-RUNTIME].
    pub fn resolve_source(&mut self, stacks: &[Vec<u64>]) -> Vec<Vec<LiveFrame>> {
        let exe = self.exe.as_path();
        let own = |image: &Image| is_main_image(Path::new(&image.path), exe);
        drop(symbolize::extend_cache_where(
            &mut self.cache,
            &self.images,
            stacks,
            self.tool.as_ref(),
            own,
        ));
        let sourceless: Vec<Vec<u64>> = stacks
            .iter()
            .filter(|stack| !self.frames(stack).iter().any(|frame| frame.user))
            .cloned()
            .collect();
        drop(symbolize::extend_cache(
            &mut self.cache,
            &self.images,
            &sourceless,
            self.tool.as_ref(),
        ));
        stacks.iter().map(|stack| self.frames(stack)).collect()
    }

    fn frames(&self, stack: &[u64]) -> Vec<LiveFrame> {
        stack
            .iter()
            .enumerate()
            .flat_map(|(index, &pc)| {
                let module = symbolize::image_of(&self.images, pc).and_then(|i| self.module(i));
                symbolize::lookup_chain(&self.images, &self.cache, index, pc)
                    .into_iter()
                    .map(move |frame| live_frame(frame, module.clone()))
            })
            .collect()
    }

    /// The image's file name. Linux reports the main executable with an empty
    /// path, which names the program itself.
    fn module(&self, image: &Image) -> Option<String> {
        let path = if image.path.is_empty() {
            self.exe.as_path()
        } else {
            Path::new(&image.path)
        };
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned())
    }
}

fn live_frame(frame: SymFrame, module: Option<String>) -> LiveFrame {
    LiveFrame {
        user: frame.kind == FrameKind::User,
        file: (!frame.file.is_empty()).then_some(frame.file),
        line: (frame.line != 0).then_some(frame.line),
        name: frame.name,
        module,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProfileError;
    use std::cell::Cell;

    thread_local! {
        /// Addresses the test symbolizer was asked for on this test's thread.
        static ASKED: Cell<usize> = const { Cell::new(0) };
    }

    /// Names every unslid address `f<hex>`; addresses below 0x2000 are Osprey
    /// source at line `addr`, the rest runtime code with no file.
    #[derive(Debug)]
    struct Naming;

    impl Symbolize for Naming {
        fn symbolize(
            &self,
            _image: &Path,
            addrs: &[u64],
        ) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
            ASKED.with(|asked| asked.set(asked.get() + addrs.len()));
            Ok(addrs
                .iter()
                .map(|&a| {
                    let file = if a < 0x2000 { "/src/app.osp" } else { "" };
                    vec![SymFrame::new(
                        &format!("f{a:x}"),
                        file,
                        u32::try_from(a).unwrap_or(0),
                    )]
                })
                .collect())
        }
    }

    fn naming(_exe: &Path, _images: &[Image]) -> Box<dyn Symbolize + Send> {
        Box::new(Naming)
    }

    fn image(path: &str, text: u64, text_size: u64) -> Image {
        Image {
            path: path.to_owned(),
            base: text,
            slide: 0,
            text,
            text_size,
            arch: String::new(),
        }
    }

    fn symbols() -> LiveSymbols {
        let images = vec![
            image("", 0x1000, 0x2000),
            image("/usr/lib/libc.so", 0x8000, 0x1000),
        ];
        LiveSymbols::with_tool(Path::new("/bin/app"), images, naming)
    }

    #[test]
    fn frames_carry_source_module_and_the_call_line_of_return_addresses() {
        let frames = symbols().resolve(&[vec![0x1100, 0x1201, 0x8001, 0x500]]);
        let names: Vec<_> = frames
            .iter()
            .flatten()
            .map(|f| (f.name.as_str(), f.user, f.module.as_deref()))
            .collect();
        // Frame 0 is the precise pc; deeper frames are return addresses, one
        // byte back; a pc below every image belongs to none and stays hex.
        assert_eq!(
            names,
            [
                ("f1100", true, Some("app")),
                ("f1200", true, Some("app")),
                ("f8000", false, Some("libc.so")),
                ("0x500", false, None)
            ]
        );
        let first = frames.first().and_then(|stack| stack.first()).unwrap();
        assert_eq!(
            (first.file.as_deref(), first.line),
            (Some("/src/app.osp"), Some(0x1100))
        );
        let libc = frames.first().and_then(|stack| stack.get(2)).unwrap();
        assert_eq!((libc.file.as_deref(), libc.line), (None, Some(0x8000)));
    }

    /// A symbolizer that crashes on every call.
    #[derive(Debug)]
    struct Broken;

    impl Symbolize for Broken {
        fn symbolize(
            &self,
            _image: &Path,
            addrs: &[u64],
        ) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
            ASKED.with(|asked| asked.set(asked.get() + addrs.len()));
            Err(ProfileError::Symbolize("tool crashed".to_owned()))
        }
    }

    fn broken(_exe: &Path, _images: &[Image]) -> Box<dyn Symbolize + Send> {
        Box::new(Broken)
    }

    #[test]
    fn a_crashed_symbolizer_leaves_hex_names_and_is_asked_again() {
        let images = vec![image("", 0x1000, 0x2000)];
        let mut live = LiveSymbols::with_tool(Path::new("/bin/app"), images, broken);
        let frames = live.resolve(&[vec![0x1100]]);
        let names: Vec<_> = frames
            .iter()
            .flatten()
            .map(|f| (f.name.as_str(), f.module.as_deref()))
            .collect();
        assert_eq!(names, [("0x1100", Some("app"))]);
        drop(live.resolve(&[vec![0x1100]]));
        assert_eq!(ASKED.with(Cell::get), 2); // nothing was remembered
    }

    #[test]
    fn a_source_profile_names_other_images_only_for_stacks_without_source() {
        let mut live = symbols();
        // The first stack reaches Osprey source in the main image, so its libc
        // leaf is never asked for; the second reaches none and is named whole.
        let frames = live.resolve_source(&[vec![0x8001, 0x1201], vec![0x8100, 0x2001]]);
        let names: Vec<Vec<_>> = frames
            .iter()
            .map(|stack| stack.iter().map(|f| (f.name.as_str(), f.user)).collect())
            .collect();
        assert_eq!(
            names,
            [
                vec![("0x8001", false), ("f1200", true)],
                vec![("f8100", false), ("f2000", false)]
            ]
        );
        assert_eq!(ASKED.with(Cell::get), 3); // 0x1200 and 0x2000, then 0x8100
    }

    #[test]
    fn an_address_is_symbolized_once_per_image_table() {
        let mut live = symbols();
        drop(live.resolve(&[vec![0x1100, 0x1201]]));
        drop(live.resolve(&[vec![0x1100, 0x1201], vec![0x1300]]));
        assert_eq!(ASKED.with(Cell::get), 3); // 0x1100, 0x1200, then only 0x1300
        live.replace_images(vec![image("", 0x1000, 0x2000)]);
        drop(live.resolve(&[vec![0x1100]]));
        assert_eq!(ASKED.with(Cell::get), 4); // a new table resolves again
        assert!(format!("{live:?}").contains("cached: 1"));
    }
}

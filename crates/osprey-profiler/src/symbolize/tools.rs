//! External symbolizer drivers [PROF-SYMBOLIZE-OFFLINE]: `llvm-symbolizer`
//! first (with inline expansion — one innermost-first chain per address),
//! `atos` as the macOS fallback (single-frame chains, `-i` not used), bare
//! hex names when neither tool is available — symbolization being
//! unavailable never fails the pipeline.

use super::{SymFrame, Symbolize};
use crate::raw::Image;
use crate::ProfileError;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Placeholder `llvm-symbolizer` prints for anything it cannot resolve.
const UNKNOWN: &str = "??";

/// Shells out to `llvm-symbolizer` (or `atos`) to resolve unslid addresses.
#[derive(Debug)]
pub(crate) struct LlvmSymbolizer {
    /// The executable handed to the CLI — the fallback object when an image
    /// path recorded in the profile no longer exists.
    binary: PathBuf,
    /// image path → `(base, slide, arch)`, needed to rebuild SLID addresses for
    /// the `atos -l <base>` fallback and to pin `atos -arch` to the slice the
    /// runtime actually mapped.
    images: BTreeMap<PathBuf, (u64, u64, String)>,
}

impl LlvmSymbolizer {
    /// Build a symbolizer for `binary` using the profile's image table.
    pub(crate) fn new(binary: &Path, images: &[Image]) -> Self {
        let images = images
            .iter()
            .map(|i| (PathBuf::from(&i.path), (i.base, i.slide, i.arch.clone())))
            .collect();
        Self {
            binary: binary.to_path_buf(),
            images,
        }
    }

    /// `atos -o <obj> -l 0x<base> 0x<slid>…`: atos undoes the slide itself,
    /// so the adjusted unslid addresses are re-slid before the call. Each
    /// output line becomes a single-frame chain (`atos -i` is not used).
    fn try_atos(&self, image: &Path, unslid_addrs: &[u64]) -> Option<Vec<Vec<SymFrame>>> {
        let tool = find_tool("atos")?;
        let (base, slide, arch) = self
            .images
            .get(image)
            .cloned()
            .unwrap_or((0, 0, String::new()));
        // `atos` resolves a dyld-shared-cache image from the LIVE cache, so the
        // recorded path is handed over even when no such file exists on disk —
        // /usr/lib/system/libsystem_malloc.dylib is cache-only on current macOS
        // and atos still names `_xzm_free` correctly from it. Only the main
        // image may fall back to the CLI binary, and only because that one is
        // genuinely the same object under a moved path.
        let object = if !image.exists() && is_main_image(image, &self.binary) {
            self.binary.as_path()
        } else {
            image
        };
        let mut command = Command::new(tool);
        let _ = command.args(atos_args(&arch, object, base, slide, unslid_addrs));
        let out = run_capture(&mut command)?;
        let lines = out.lines().chain(std::iter::repeat(""));
        Some(
            unslid_addrs
                .iter()
                .zip(lines)
                .map(|(&a, line)| vec![parse_atos_line(line, a)])
                .collect(),
        )
    }
}

impl Symbolize for LlvmSymbolizer {
    fn symbolize(
        &self,
        image: &Path,
        unslid_addrs: &[u64],
    ) -> Result<Vec<Vec<SymFrame>>, ProfileError> {
        Ok(resolve_object(image, &self.binary)
            .and_then(|object| try_llvm(&object, unslid_addrs))
            .or_else(|| self.try_atos(image, unslid_addrs))
            .unwrap_or_else(|| hex_chains(unslid_addrs)))
    }
}

/// Prefer the dSYM DWARF companion when dsymutil produced one, else the image
/// itself. `None` when this image has no object on disk that describes IT.
///
/// The CLI binary substitutes only for the main image under a moved path.
/// Substituting it for any other image is what invented symbol names: every
/// dyld-shared-cache dylib is absent from disk, so their addresses were
/// resolved against the profiled program's own symbol table and came back as
/// whatever symbol happened to precede that offset — `mach_absolute_time`
/// reported as `task_get_special_port`. A wrong name is worse than no name,
/// because only the wrong name looks like data [PROF-SYMBOLIZE-OFFLINE].
fn resolve_object(image: &Path, fallback: &Path) -> Option<PathBuf> {
    let primary = match (image.exists(), is_main_image(image, fallback)) {
        (true, _) => image,
        (false, true) => fallback,
        (false, false) => return None,
    };
    Some(dsym_object(primary).unwrap_or_else(|| primary.to_path_buf()))
}

/// `atos` argv for one image: the slice, the object, its load address, and the
/// addresses re-slid (atos undoes the slide itself).
///
/// `-arch` is not decoration. Every system dylib is universal, and `atos` left
/// on its host default reads the `arm64` slice of an image the kernel mapped
/// as `arm64e`: one address is `task_get_special_port + 40` under the first
/// slice and `mach_absolute_time + 108` under the second, and only the second
/// agrees with `dladdr` and `/usr/bin/sample` [PROF-SYMBOLIZE-OFFLINE].
fn atos_args(
    arch: &str,
    object: &Path,
    base: u64,
    slide: u64,
    unslid_addrs: &[u64],
) -> Vec<OsString> {
    // `unwrap_or_default()` here would emit TWO EMPTY arguments, which atos
    // takes as addresses and fails on; the Option must be flattened away.
    let arch_flag = (!arch.is_empty()).then(|| [OsString::from("-arch"), OsString::from(arch)]);
    arch_flag
        .into_iter()
        .flatten()
        .chain([
            OsString::from("-o"),
            object.into(),
            OsString::from("-l"),
            OsString::from(format!("{base:#x}")),
        ])
        .chain(
            unslid_addrs
                .iter()
                .map(|a| OsString::from(format!("{:#x}", a.saturating_add(slide)))),
        )
        .collect()
}

/// Whether `image` names the same binary as the CLI's `fallback`, so the
/// fallback may stand in for it.
///
/// Compared by file name because the recorded path is where the program ran,
/// which need not be where it now sits. An EMPTY path also qualifies: on Linux
/// `dl_iterate_phdr` reports the main executable with `dlpi_name == ""`, and
/// treating that as a foreign image would leave every Osprey frame unnamed.
fn is_main_image(image: &Path, fallback: &Path) -> bool {
    if image.as_os_str().is_empty() {
        return true;
    }
    match (image.file_name(), fallback.file_name()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// dsymutil layout: `<bin>.dSYM/Contents/Resources/DWARF/<basename>`.
fn dsym_object(binary: &Path) -> Option<PathBuf> {
    let name = binary.file_name()?;
    let mut dsym = binary.as_os_str().to_owned();
    dsym.push(".dSYM");
    let candidate = PathBuf::from(dsym)
        .join("Contents/Resources/DWARF")
        .join(name);
    candidate.is_file().then_some(candidate)
}

/// Feed `0x…` addresses to `llvm-symbolizer` on stdin, one per line.
/// Inlining stays ON so each address expands to its full inline chain
/// [PROF-SYMBOLIZE-OFFLINE].
fn try_llvm(object: &Path, unslid_addrs: &[u64]) -> Option<Vec<Vec<SymFrame>>> {
    let tool = find_symbolizer()?;
    let lines: Vec<String> = unslid_addrs.iter().map(|a| format!("{a:#x}")).collect();
    let input = lines.join("\n") + "\n";
    let mut command = Command::new(tool);
    let _ = command
        .arg(format!("--obj={}", object.display()))
        .arg("--functions=linkage");
    let out = run_with_stdin(&mut command, &input)?;
    Some(parse_llvm_output(&out, unslid_addrs))
}

/// The unconditional last resort: one single-frame hex chain per address.
fn hex_chains(unslid_addrs: &[u64]) -> Vec<Vec<SymFrame>> {
    unslid_addrs
        .iter()
        .map(|&addr| vec![SymFrame::hex(addr)])
        .collect()
}

/// First `name` on `PATH` that exists as a file.
fn find_tool(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Distributions may install LLVM tools with only a versioned executable name.
fn find_symbolizer() -> Option<PathBuf> {
    find_tool("llvm-symbolizer").or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path)
            .filter_map(|dir| std::fs::read_dir(dir).ok())
            .flatten()
            .filter_map(Result::ok)
            .filter_map(symbolizer_version)
            .filter(|(_, path)| path.is_file())
            .max_by_key(|(version, _)| *version)
            .map(|(_, path)| path)
    })
}

fn symbolizer_version(entry: std::fs::DirEntry) -> Option<(u32, PathBuf)> {
    let name = entry.file_name();
    let version = name
        .to_str()?
        .strip_prefix("llvm-symbolizer-")?
        .parse::<u32>()
        .ok()?;
    Some((version, entry.path()))
}

/// Run to completion, returning stdout only on a zero exit status.
fn run_capture(command: &mut Command) -> Option<String> {
    let output = command.stderr(Stdio::null()).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Like [`run_capture`] but writes `input` to the child's stdin first.
fn run_with_stdin(command: &mut Command, input: &str) -> Option<String> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(input.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `llvm-symbolizer` output: one blank-line-separated block per input
/// address, each holding one or more `name\nfile:line:col` PAIRS —
/// innermost inline frame first. Unresolved addresses (`??`) become
/// single hex-frame chains.
fn parse_llvm_output(out: &str, unslid_addrs: &[u64]) -> Vec<Vec<SymFrame>> {
    let blocks: Vec<&str> = out
        .split("\n\n")
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .collect();
    unslid_addrs
        .iter()
        .enumerate()
        .map(|(index, &addr)| {
            blocks
                .get(index)
                .map_or_else(|| vec![SymFrame::hex(addr)], |b| chain_from_block(b, addr))
        })
        .collect()
}

/// One `llvm-symbolizer` block → innermost-first inline chain.
fn chain_from_block(block: &str, addr: u64) -> Vec<SymFrame> {
    let lines: Vec<&str> = block.lines().map(str::trim).collect();
    let frames: Vec<SymFrame> = lines.chunks(2).filter_map(frame_from_pair).collect();
    if frames.is_empty() {
        vec![SymFrame::hex(addr)]
    } else {
        frames
    }
}

/// One `(name, location)` line pair → frame; `??`/empty names resolve to
/// nothing (the caller hex-falls-back when the whole chain is unknown).
fn frame_from_pair(pair: &[&str]) -> Option<SymFrame> {
    let name = pair.first()?.trim();
    if name == UNKNOWN || name.is_empty() {
        return None;
    }
    let (file, line) = parse_file_line(pair.get(1).copied().unwrap_or_default().trim());
    Some(SymFrame::new(name, &file, line))
}

/// Split `file:line:col` from the right, so drive letters and other colons
/// inside the file path survive.
fn parse_file_line(loc: &str) -> (String, u32) {
    let mut parts = loc.rsplitn(3, ':');
    let _col = parts.next();
    let line = parts.next().and_then(|l| l.parse().ok()).unwrap_or(0);
    let file = parts.next().unwrap_or_default();
    if file == UNKNOWN {
        (String::new(), 0)
    } else {
        (file.to_owned(), line)
    }
}

/// One `atos` line → frame. Formats: `name (in mod) (file.c:12)`,
/// `name (in mod) + 40`, or a bare `0x…` for unresolved addresses.
fn parse_atos_line(line: &str, addr: u64) -> SymFrame {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with("0x") {
        return SymFrame::hex(addr);
    }
    let name = trimmed.split(" (in ").next().unwrap_or(trimmed);
    let (file, line_no) = atos_location(trimmed).unwrap_or_default();
    SymFrame::new(name, &file, line_no)
}

/// The trailing `(file.c:12)` group of an `atos` line, when present.
fn atos_location(line: &str) -> Option<(String, u32)> {
    let start = line.rfind('(')?;
    let inner = line.get(start + 1..)?.strip_suffix(')')?;
    let (file, line_no) = inner.rsplit_once(':')?;
    Some((file.to_owned(), line_no.parse().ok()?))
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;

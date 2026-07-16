//! External symbolizer drivers [PROF-SYMBOLIZE-OFFLINE]: `llvm-symbolizer`
//! first (with inline expansion — one innermost-first chain per address),
//! `atos` as the macOS fallback (single-frame chains, `-i` not used), bare
//! hex names when neither tool is available — symbolization being
//! unavailable never fails the pipeline.

use super::{SymFrame, Symbolize};
use crate::raw::Image;
use crate::ProfileError;
use std::collections::BTreeMap;
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
    /// image path → `(base, slide)`, needed to rebuild SLID addresses for
    /// the `atos -l <base>` fallback.
    images: BTreeMap<PathBuf, (u64, u64)>,
}

impl LlvmSymbolizer {
    /// Build a symbolizer for `binary` using the profile's image table.
    pub(crate) fn new(binary: &Path, images: &[Image]) -> Self {
        let images = images
            .iter()
            .map(|i| (PathBuf::from(&i.path), (i.base, i.slide)))
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
        let (base, slide) = self.images.get(image).copied().unwrap_or((0, 0));
        let object = if image.exists() {
            image
        } else {
            self.binary.as_path()
        };
        let mut command = Command::new(tool);
        let _ = command
            .arg("-o")
            .arg(object)
            .arg("-l")
            .arg(format!("{base:#x}"));
        let _ = command.args(
            unslid_addrs
                .iter()
                .map(|a| format!("{:#x}", a.saturating_add(slide))),
        );
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
        let object = resolve_object(image, &self.binary);
        Ok(try_llvm(&object, unslid_addrs)
            .or_else(|| self.try_atos(image, unslid_addrs))
            .unwrap_or_else(|| hex_chains(unslid_addrs)))
    }
}

/// Prefer the dSYM DWARF companion when dsymutil produced one, else the
/// image itself (falling back to the CLI-provided binary if the recorded
/// image path is gone).
fn resolve_object(image: &Path, fallback: &Path) -> PathBuf {
    let primary = if image.exists() { image } else { fallback };
    dsym_object(primary).unwrap_or_else(|| primary.to_path_buf())
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
    let tool = find_tool("llvm-symbolizer")?;
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
pub(crate) fn find_tool(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// Run to completion, returning stdout only on a zero exit status.
pub(crate) fn run_capture(command: &mut Command) -> Option<String> {
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
pub(crate) fn parse_llvm_output(out: &str, unslid_addrs: &[u64]) -> Vec<Vec<SymFrame>> {
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
pub(crate) fn parse_atos_line(line: &str, addr: u64) -> SymFrame {
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
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn parses_llvm_symbolizer_blocks() {
        let out = "add\n/tmp/t.c:2:0\n\nrun\nC:\\proj\\app.osp:14:3\n\n??\n??:0:0\n\n";
        let chains = parse_llvm_output(out, &[0x10, 0x20, 0x30, 0x40]);
        let add = chains.first().unwrap().first().unwrap();
        assert_eq!(
            (add.name.as_str(), add.file.as_str(), add.line),
            ("add", "/tmp/t.c", 2)
        );
        let run = chains.get(1).unwrap().first().unwrap();
        assert_eq!((run.file.as_str(), run.line), ("C:\\proj\\app.osp", 14));
        assert_eq!(chains.get(2).unwrap().first().unwrap().name, "0x30");
        // Fewer blocks than addresses: the tail pads out as hex chains.
        assert_eq!(chains.get(3).unwrap().first().unwrap().name, "0x40");
    }

    #[test]
    fn parses_two_deep_inline_blocks_innermost_first() {
        // One address whose block carries TWO (name, location) pairs — the
        // inlined callee first, then the function it was inlined into —
        // followed by a plain single-pair block for the next address.
        let out = "inner\n/src/app.osp:4:9\nouter\n/src/app.osp:9:1\n\nmain\n/m.c:2:0\n\n";
        let chains = parse_llvm_output(out, &[0x10, 0x20]);
        let chain = chains.first().unwrap();
        let got: Vec<(&str, u32)> = chain.iter().map(|f| (f.name.as_str(), f.line)).collect();
        assert_eq!(got, [("inner", 4), ("outer", 9)]);
        assert_eq!(chains.get(1).unwrap().len(), 1);
    }

    #[test]
    fn llvm_blocks_without_location_lines_still_name_the_frame() {
        let chains = parse_llvm_output("main\n\n", &[0x10]);
        let main = chains.first().unwrap().first().unwrap();
        assert_eq!(
            (main.name.as_str(), main.file.as_str(), main.line),
            ("main", "", 0)
        );
    }

    #[test]
    fn parses_atos_line_with_location() {
        let frame = parse_atos_line("fib (in x.out) (fib.osp:12)", 5);
        assert_eq!(
            (frame.name.as_str(), frame.file.as_str(), frame.line),
            ("fib", "fib.osp", 12)
        );
    }

    #[test]
    fn parses_atos_line_with_offset_only() {
        let frame = parse_atos_line("start (in dyld) + 40", 5);
        assert_eq!(
            (frame.name.as_str(), frame.file.as_str(), frame.line),
            ("start", "", 0)
        );
    }

    #[test]
    fn atos_hex_and_blank_lines_become_hex_frames() {
        assert_eq!(parse_atos_line("0x100003f10", 0xbeef).name, "0xbeef");
        assert_eq!(parse_atos_line("   ", 0xbeef).name, "0xbeef");
        assert_eq!(parse_atos_line("weird (in mod) (nocolon)", 3).file, "");
    }

    #[test]
    fn find_tool_walks_path() {
        assert!(find_tool("sh").is_some());
        assert!(find_tool("definitely-not-a-real-tool-osprey").is_none());
    }

    #[test]
    fn resolve_object_prefers_the_dsym_dwarf_when_present() {
        let dir = testutil::temp_dir("dsym");
        let binary = dir.join("app");
        std::fs::write(&binary, b"bin").unwrap();
        let dwarf_dir = dir.join("app.dSYM/Contents/Resources/DWARF");
        std::fs::create_dir_all(&dwarf_dir).unwrap();
        let dwarf = dwarf_dir.join("app");
        std::fs::write(&dwarf, b"dwarf").unwrap();
        assert_eq!(resolve_object(&binary, Path::new("/fallback")), dwarf);
        // Missing image -> the CLI-provided binary stands in.
        assert_eq!(resolve_object(&dir.join("gone"), &binary), dwarf);
        std::fs::remove_dir_all(dir.join("app.dSYM")).unwrap();
        assert_eq!(resolve_object(&binary, Path::new("/fallback")), binary);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unresolvable_objects_fall_back_to_hex_frames() {
        let missing = Path::new("/definitely-not-a-real-binary-osprey");
        let sym = LlvmSymbolizer::new(missing, &[]);
        let chains = sym.symbolize(missing, &[0x1234]).unwrap();
        assert_eq!(chains.first().unwrap().first().unwrap().name, "0x1234");
    }

    #[test]
    fn run_capture_returns_none_on_nonzero_exit() {
        let mut fail = Command::new("sh");
        let _ = fail.arg("-c").arg("exit 3");
        assert!(run_capture(&mut fail).is_none());
        let mut spawn_fail = Command::new("/definitely-not-a-real-tool-osprey");
        assert!(run_capture(&mut spawn_fail).is_none());
        assert!(run_with_stdin(&mut Command::new("/definitely-not-a-real-tool"), "x").is_none());
    }

    /// Compile a tiny C fixture with `clang -g -O0` and return its `add`
    /// symbol address from `nm`. `None` when any tool is missing — callers
    /// skip silently so environments without toolchains stay green.
    fn compiled_fixture(tag: &str) -> Option<(PathBuf, PathBuf, u64)> {
        let clang = find_tool("clang")?;
        let nm = find_tool("nm")?;
        let dir = testutil::temp_dir(tag);
        let src = dir.join("t.c");
        std::fs::write(
            &src,
            "int add(int a,int b){return a+b;}\nint main(void){return add(1,2);}\n",
        )
        .ok()?;
        let bin = dir.join("t");
        let mut compile = Command::new(clang);
        let _ = compile.arg("-g").arg("-O0").arg("-o").arg(&bin).arg(&src);
        if !compile.status().ok()?.success() {
            return None;
        }
        let addr = symbol_addr(&nm, &bin, "add")?;
        Some((dir, bin, addr))
    }

    /// Text-section address of `name` (with or without the Mach-O `_`).
    fn symbol_addr(nm: &Path, bin: &Path, name: &str) -> Option<u64> {
        let mut command = Command::new(nm);
        let _ = command.arg(bin);
        let out = run_capture(&mut command)?;
        out.lines().find_map(|line| {
            let mut parts = line.split_whitespace();
            let (addr, kind, sym) = (parts.next()?, parts.next()?, parts.next()?);
            let hit = kind.eq_ignore_ascii_case("t") && (sym == name || sym == format!("_{name}"));
            hit.then(|| u64::from_str_radix(addr, 16).ok())?
        })
    }

    #[test]
    fn real_llvm_symbolizer_resolves_a_c_symbol() {
        let Some((dir, bin, addr)) = compiled_fixture("llvm") else {
            return;
        };
        if find_tool("llvm-symbolizer").is_none() {
            return;
        }
        let chains = LlvmSymbolizer::new(&bin, &[])
            .symbolize(&bin, &[addr])
            .unwrap();
        let frame = chains.first().unwrap().first().unwrap();
        assert!(frame.name.contains("add"), "unexpected frame: {frame:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn real_atos_resolves_a_c_symbol_via_slid_addresses() {
        let Some((dir, bin, addr)) = compiled_fixture("atos") else {
            return;
        };
        let base = addr & !u64::from(u32::MAX);
        let image = Image {
            path: bin.to_string_lossy().into_owned(),
            base,
            slide: 0,
        };
        let sym = LlvmSymbolizer::new(&bin, &[image]);
        let Some(chains) = sym.try_atos(&bin, &[addr]) else {
            return;
        };
        let frame = chains.first().unwrap().first().unwrap();
        assert!(frame.name.contains("add"), "unexpected frame: {frame:?}");
        assert!(frame.file.ends_with("t.c"), "unexpected file: {frame:?}");
        assert!(frame.line >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

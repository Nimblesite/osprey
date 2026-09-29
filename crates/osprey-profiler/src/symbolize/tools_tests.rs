
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
    assert_eq!(
        resolve_object(&binary, Path::new("/fallback")),
        Some(dwarf.clone())
    );
    // The MAIN image under a moved path still stands in: same file name,
    // so it is the same object.
    assert_eq!(resolve_object(&dir.join("moved/app"), &binary), Some(dwarf));
    std::fs::remove_dir_all(dir.join("app.dSYM")).unwrap();
    assert_eq!(
        resolve_object(&binary, Path::new("/fallback")),
        Some(binary.clone())
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A foreign image with no file on disk resolves to NO object. Standing the
/// profiled binary in for it is what named `mach_absolute_time` as
/// `task_get_special_port`: every dyld-shared-cache dylib is absent from
/// disk, so their addresses were read against Osprey's own symbol table
/// [PROF-SYMBOLIZE-OFFLINE].
/// Linux reports the main executable with an empty `dlpi_name`. It must
/// still resolve to the CLI binary, or every Osprey frame goes unnamed on
/// the platform CI actually runs.
#[test]
fn resolve_object_treats_an_empty_image_path_as_the_main_binary() {
    let dir = testutil::temp_dir("emptypath");
    let binary = dir.join("app");
    std::fs::write(&binary, b"bin").unwrap();
    assert_eq!(resolve_object(Path::new(""), &binary), Some(binary.clone()));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn resolve_object_refuses_to_stand_a_foreign_image_in() {
    let dir = testutil::temp_dir("foreign");
    let binary = dir.join("app");
    std::fs::write(&binary, b"bin").unwrap();
    let cache_only = Path::new("/usr/lib/system/libsystem_malloc.dylib");
    assert!(!cache_only.exists(), "fixture assumes a cache-only dylib");
    assert_eq!(resolve_object(cache_only, &binary), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// [PROF-SYMBOLIZE-OFFLINE] The slice must be pinned on the command line.
/// Drop `-arch` and `atos` reads a universal dylib on its host default,
/// which for an `arm64e` process is the wrong slice: the same address is
/// `task_get_special_port + 40` under `arm64` and `mach_absolute_time +
/// 108` under `arm64e`. Nothing downstream can tell those apart, because a
/// wrong name looks exactly like a right one. The addresses are re-slid
/// here too, since `atos` undoes the slide itself.
#[test]
fn atos_argv_pins_the_slice_the_image_was_mapped_as() {
    let args = atos_args(
        "arm64e",
        Path::new("/usr/lib/system/libsystem_kernel.dylib"),
        0x1_8DE9_1000,
        0x0D9E_4000,
        &[0x1_804A_E10C],
    );
    let flat: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        flat,
        [
            "-arch",
            "arm64e",
            "-o",
            "/usr/lib/system/libsystem_kernel.dylib",
            "-l",
            "0x18de91000",
            "0x18de9210c",
        ]
    );
}

/// An image that recorded no slice must not grow an empty `-arch`, which
/// `atos` rejects outright — that would turn every frame of that image hex.
#[test]
fn atos_argv_omits_the_slice_when_none_was_recorded() {
    let args = atos_args("", Path::new("/bin/app"), 0, 0, &[0x20]);
    let flat: Vec<String> = args
        .iter()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert_eq!(flat, ["-o", "/bin/app", "-l", "0x0", "0x20"]);
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
    if find_symbolizer().is_none() {
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
        text: 0,
        text_size: 0,
        arch: String::new(),
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

//! Apple driver contracts, independent of SDK availability.

use super::*;
use crate::ios_abi::ExportScope;

#[test]
fn slices_choose_distinct_sdk_triple_and_runtime() {
    assert_eq!(Target::parse("ios"), Some(Target::Device));
    assert_eq!(Target::parse("ios-sim"), Some(Target::Simulator));
    assert_eq!(Target::parse("macos"), Some(Target::Mac));
    assert_eq!(Target::parse("native"), None);
    assert_eq!(Target::Device.sdk(), "iphoneos");
    assert_eq!(Target::Simulator.sdk(), "iphonesimulator");
    assert_eq!(Target::Mac.sdk(), "macosx");
    assert_eq!(Target::Mac.triple(), "arm64-apple-macos12.0");
    for target in TARGETS {
        let args = compile_args(
            Path::new("p.ll"),
            Path::new("p.o"),
            Path::new("/sdk path"),
            target,
        );
        assert!(args.contains(&format!("--target={}", target.triple())));
        assert!(args.contains(&"/sdk path".to_string()));
        assert!(args.contains(&"-c".to_string()));
    }
    let runtimes: std::collections::BTreeSet<_> = TARGETS.iter().map(|t| t.runtime()).collect();
    assert_eq!(
        runtimes.len(),
        TARGETS.len(),
        "every slice links its own runtime"
    );
}

const TARGETS: [Target; 3] = [Target::Device, Target::Simulator, Target::Mac];

#[test]
fn archive_contains_program_and_matching_runtime_without_an_executable_entry() {
    let args = archive_args(Path::new("app.o"), "runtime.a", Path::new("out.a"));
    assert_eq!(
        args,
        ["libtool", "-static", "-o", "out.a", "app.o", "runtime.a"]
    );
    assert!(validate_output(Path::new("out.a"), Target::Mac).is_ok());
    let error = validate_output(Path::new("out.h"), Target::Mac).expect_err("header output");
    assert!(error.starts_with("macOS output"), "{error}");
}

#[test]
fn rejects_unsupported_build_options_before_toolchain_work() {
    for option in [
        "--memory=gc",
        "--memory=arc",
        "--debug",
        "--profile",
        "--run",
    ] {
        for target in TARGETS {
            let args =
                ["app.osp", &format!("--target={}", target.name()), option].map(str::to_string);
            let cli = crate::parse_args(&args).expect("valid arguments");
            assert!(
                validate(&cli, target).is_err(),
                "{option} {}",
                target.name()
            );
        }
    }
    for target in TARGETS {
        let args = [
            "app.osp",
            &format!("--target={}", target.name()),
            "--compile",
        ]
        .map(str::to_string);
        let cli = crate::parse_args(&args).expect("valid arguments");
        assert!(validate(&cli, target).is_ok(), "{}", target.name());
    }
}

#[test]
fn scratch_paths_are_unique_and_cleaned_on_failure() {
    let first = Scratch::new("app.osp", Target::Device.sdk()).expect("temp directory");
    let second = Scratch::new("app.osp", Target::Device.sdk()).expect("temp directory");
    let path = first.path.clone();
    assert_ne!(path, second.path);
    assert!(path.is_dir());
    drop(first);
    assert!(!path.exists());
}

#[test]
fn publishes_archive_and_header_in_new_output_directory() {
    let scratch = Scratch::new("publish.osp", Target::Device.sdk()).expect("scratch");
    let archive = scratch.path.join("source.a");
    write(&archive, "archive").expect("fixture");
    let out = scratch.path.join("new/app.a");
    publish(&archive, &out, "header").expect("published");
    assert_eq!(std::fs::read_to_string(&out).expect("archive"), "archive");
    assert_eq!(
        std::fs::read_to_string(out.with_extension("h")).expect("header"),
        "header"
    );
}

#[test]
fn app_globals_remain_initialized_after_the_host_calls_entry() {
    // [IOS-TARGET-ENTRY] An app library outlives its initialization call.
    let parsed = osprey_syntax::parse_program(
        "let greeting = \"hello\" + \" world\"\nfn greet() = greeting\n",
    );
    assert_eq!(
        osprey_types::check_program(&parsed.program),
        Vec::<osprey_types::TypeError>::new()
    );
    let (ir, header) = source(
        &parsed.program,
        "globals.osp",
        Target::Device,
        &ExportScope::EntrySource(None),
    )
    .expect("library source");
    crate::testkit::shows(&header, &["osprey_greet(void)"]);
    crate::testkit::contents(
        &ir,
        &["define i32 @osprey_main()"],
        &["store i8* null, i8** @"],
    );
    let executable = osprey_codegen::compile_program(&parsed.program).expect("native source");
    crate::testkit::shows(&executable, &["store i8* null, i8** @"]);
}

#[test]
fn each_slice_names_itself_in_its_header_and_diagnostics() {
    // A simulator build used to emit a header saying `--target=ios` and to
    // report ABI errors against `ios`. Following that instruction rebuilds the
    // DEVICE archive, which cannot link into a simulator host — the generated
    // artifact told the reader to undo the choice they had just made.
    let parsed = osprey_syntax::parse_program("fn greet(name: string) = \"hi ${name}\"\n");
    for (target, name) in TARGETS.map(|target| (target, target.name())) {
        let (_, header) = source(
            &parsed.program,
            "app.osp",
            target,
            &ExportScope::EntrySource(None),
        )
        .expect("abi");
        assert!(
            header.contains(&format!("--target={name}")),
            "{name} header must name its own slice:\n{header}"
        );
        crate::testkit::shows(&header, &["[IOS-HOST-ABI]"]);
    }

    // The same slice name must appear in a rejection, so a diagnostic never
    // sends the reader to the other slice.
    let rejected = osprey_syntax::parse_program("extern fn host(values: List<int>) -> int\n");
    for (target, name) in TARGETS.map(|target| (target, target.name())) {
        let error = source(
            &rejected.program,
            "app.osp",
            target,
            &ExportScope::EntrySource(None),
        )
        .expect_err("aggregate extern");
        assert!(
            error.contains(&format!("target `{name}`")),
            "{name} diagnostic must name its own slice: {error}"
        );
    }
}

#[test]
fn a_missing_runtime_names_the_make_goal_that_builds_it() {
    assert_eq!(
        TARGETS.map(Target::missing_runtime),
        [
            "libosprey_runtime_ios.a not found — run `make ios` to build the iOS runtime",
            "libosprey_runtime_ios_sim.a not found — run `make ios` to build the iOS runtime",
            "libosprey_runtime_macos.a not found — run `make macos` to build the macOS runtime",
        ]
    );
}

#[test]
fn mac_and_iphone_slices_share_one_boundary() {
    // [IOS-TARGET-MACOS] One Swift bridge serves every Apple host only if the
    // same program yields the same C declarations and the same adapted IR on
    // each slice; the header may differ in nothing but the slice it names.
    let parsed = osprey_syntax::parse_program(
        "extern fn host_log(message: string, urgent: bool) -> Unit\n\
         fn ready(count: int) = count > 2\n\
         fn label(count: int, scale: float) = \"${count} x ${scale}\"\n",
    );
    let (ios_ir, ios_header) = source(
        &parsed.program,
        "app.osp",
        Target::Device,
        &ExportScope::EntrySource(None),
    )
    .expect("ios abi");
    let (mac_ir, mac_header) = source(
        &parsed.program,
        "app.osp",
        Target::Mac,
        &ExportScope::EntrySource(None),
    )
    .expect("macos abi");
    assert_eq!(mac_ir, ios_ir);
    assert_eq!(
        mac_header,
        ios_header
            .replace("Osprey ios ", "Osprey macos ")
            .replace("--target=ios", "--target=macos")
    );
    for declaration in [
        "void host_log(const char *message, bool urgent);",
        "bool osprey_ready(int64_t count);",
        "const char *osprey_label(int64_t count, double scale);",
    ] {
        assert!(
            mac_header.contains(declaration),
            "{declaration}\n{mac_header}"
        );
    }
}

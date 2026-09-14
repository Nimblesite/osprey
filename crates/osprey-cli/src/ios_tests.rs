//! iOS driver contracts, independent of SDK availability.

use super::*;

#[test]
fn slices_choose_distinct_sdk_triple_and_runtime() {
    assert_eq!(Target::parse("ios"), Some(Target::Device));
    assert_eq!(Target::parse("ios-sim"), Some(Target::Simulator));
    assert_eq!(Target::parse("native"), None);
    assert_eq!(Target::Device.sdk(), "iphoneos");
    assert_eq!(Target::Simulator.sdk(), "iphonesimulator");
    for target in [Target::Device, Target::Simulator] {
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
    assert_ne!(Target::Device.runtime(), Target::Simulator.runtime());
}

#[test]
fn archive_contains_program_and_matching_runtime_without_an_executable_entry() {
    let args = archive_args(Path::new("app.o"), "runtime.a", Path::new("out.a"));
    assert_eq!(
        args,
        ["libtool", "-static", "-o", "out.a", "app.o", "runtime.a"]
    );
    assert!(validate_output(Path::new("out.a")).is_ok());
    assert!(validate_output(Path::new("out.h")).is_err());
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
        let args = ["app.osp", "--target=ios", option].map(str::to_string);
        let cli = crate::parse_args(&args).expect("valid arguments");
        assert!(validate(&cli).is_err(), "{option}");
    }
    let args = ["app.osp", "--target=ios-sim", "--compile"].map(str::to_string);
    assert!(validate(&crate::parse_args(&args).expect("valid arguments")).is_ok());
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
    assert!(osprey_types::check_program(&parsed.program).is_empty());
    let (ir, header) =
        source(&parsed.program, "globals.osp", Target::Device).expect("library source");
    assert!(header.contains("osprey_greet(void)"));
    assert!(
        !ir.contains("store i8* null, i8** @"),
        "entry must keep globals live:\n{ir}"
    );
    assert!(ir.contains("define i32 @osprey_main()"));
    let executable = osprey_codegen::compile_program(&parsed.program).expect("native source");
    assert!(
        executable.contains("store i8* null, i8** @"),
        "executables still release globals at exit"
    );
}

#[test]
fn each_slice_names_itself_in_its_header_and_diagnostics() {
    // A simulator build used to emit a header saying `--target=ios` and to
    // report ABI errors against `ios`. Following that instruction rebuilds the
    // DEVICE archive, which cannot link into a simulator host — the generated
    // artifact told the reader to undo the choice they had just made.
    let parsed = osprey_syntax::parse_program("fn greet(name: string) = \"hi ${name}\"\n");
    for (target, name) in [(Target::Device, "ios"), (Target::Simulator, "ios-sim")] {
        let (_, header) = source(&parsed.program, "app.osp", target).expect("abi");
        assert!(
            header.contains(&format!("--target={name}")),
            "{name} header must name its own slice:\n{header}"
        );
        assert!(header.contains("[IOS-HOST-ABI]"), "{header}");
    }

    // The same slice name must appear in a rejection, so a diagnostic never
    // sends the reader to the other slice.
    let rejected = osprey_syntax::parse_program("extern fn host(values: List<int>) -> int\n");
    for (target, name) in [(Target::Device, "ios"), (Target::Simulator, "ios-sim")] {
        let error = source(&rejected.program, "app.osp", target).expect_err("aggregate extern");
        assert!(
            error.contains(&format!("target `{name}`")),
            "{name} diagnostic must name its own slice: {error}"
        );
    }
}

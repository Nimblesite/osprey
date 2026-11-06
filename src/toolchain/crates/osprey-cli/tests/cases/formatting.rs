use super::{read_text, run_args, run_args_with_stdin, set_mode, temp_dir, temp_osp};

#[test]
fn fmt_stdout_check_and_rewrite_modes() {
    let prog = temp_osp("fmt_modes", "fn main() = {\nprint(1)\n}\n");
    let path = prog.to_string_lossy().into_owned();
    let shown = run_args(&["fmt", "--stdout", "--flavor", "default", &path]);
    shown.expect_stdout(0, &[" print(1)"], &[]);
    let unchanged = read_text(&prog);
    assert!(unchanged.contains("\nprint(1)\n"), "{unchanged}");

    let checked = run_args(&["fmt", "--check", &path]);
    assert_ne!(checked.code, Some(0));
    assert!(
        checked.stdout.contains("would reformat"),
        "{}",
        checked.stdout
    );

    let quiet = run_args(&["fmt", "--quiet", &path]);
    assert_eq!(quiet.code, Some(0), "stderr={}", quiet.stderr);
    assert!(quiet.stdout.trim().is_empty(), "{}", quiet.stdout);
    let rewritten = read_text(&prog);
    assert!(rewritten.contains("    print(1)"), "{rewritten}");

    let clean = temp_osp("fmt_clean", "fn main() = {\n    print(1)\n}\n");
    let clean_path = clean.to_string_lossy().into_owned();
    let no_change = run_args(&["fmt", &clean_path]);
    assert_eq!(no_change.code, Some(0), "stderr={}", no_change.stderr);
    assert!(no_change.stdout.trim().is_empty(), "{}", no_change.stdout);

    let loud = temp_osp("fmt_loud", "fn main() = {\nprint(2)\n}\n");
    let loud_path = loud.to_string_lossy().into_owned();
    let rewritten_loud = run_args(&["fmt", &loud_path]);
    rewritten_loud.expect_stdout(0, &["formatted"], &[]);
}

#[test]
fn fmt_recurses_directories_and_formats_stdin() {
    let dir = temp_dir("fmt_tree");
    let nested = dir.join("nested");
    let _ = std::fs::create_dir_all(&nested);
    let osp = dir.join("a.osp");
    let ml = nested.join("b.ospml");
    let ignored = nested.join("ignored.txt");
    let _ = std::fs::write(&osp, "fn main() = {\nprint(1)\n}\n");
    let _ = std::fs::write(&ml, "print 2\n");
    let _ = std::fs::write(&ignored, "not osprey\n");

    let dir_arg = dir.to_string_lossy().into_owned();
    let shown = run_args(&["fmt", "--stdout", &dir_arg]);
    shown.expect_stdout(0, &["print(1)", "print 2"], &["not osprey"]);

    let piped = run_args_with_stdin(&["fmt", "--flavor=ml", "-"], "print 3\n");
    piped.expect_stdout(0, &["print 3"], &[]);

    let bad_stdin = run_args_with_stdin(&["fmt", "-"], "fn main( = {\n");
    assert_ne!(bad_stdin.code, Some(0));
    assert!(bad_stdin.stderr.contains("stdin:"), "{}", bad_stdin.stderr);
}

#[test]
fn fmt_reports_usage_parse_and_read_errors() {
    let no_paths = run_args(&["fmt"]);
    no_paths.expect_stderr(2, &["usage: osprey fmt"], &[]);

    let missing_flavor = run_args(&["fmt", "--flavor"]);
    missing_flavor.expect_stderr(2, &["--flavor requires"], &[]);

    let bad_flavor = run_args(&["fmt", "--flavor=bogus", "x.osp"]);
    bad_flavor.expect_stderr(2, &["unknown flavor"], &[]);

    let bad_flag = run_args(&["fmt", "--bogus", "x.osp"]);
    bad_flag.expect_stderr(2, &["unknown flag --bogus"], &[]);

    let missing = run_args(&["fmt", "/no/such/osprey_fmt_missing.osp"]);
    assert_ne!(missing.code, Some(0));
    assert!(missing.stderr.contains("cannot read"), "{}", missing.stderr);

    let broken = temp_osp("fmt_broken", "fn main( = {\n");
    let path = broken.to_string_lossy().into_owned();
    let parsed = run_args(&["fmt", &path]);
    assert_ne!(parsed.code, Some(0));
    assert_ne!(parsed.stderr, "");
}

#[cfg(unix)]
#[test]
fn fmt_reports_write_errors() {
    let blocked_dir = temp_dir("fmt_blocked");
    set_mode(&blocked_dir, 0o000);
    let blocked_arg = blocked_dir.to_string_lossy().into_owned();
    let blocked = run_args(&["fmt", &blocked_arg]);
    set_mode(&blocked_dir, 0o755);
    assert_eq!(blocked.code, Some(0), "stderr={}", blocked.stderr);

    let prog = temp_osp("fmt_readonly", "fn main() = {\nprint(1)\n}\n");
    let path = prog.to_string_lossy().into_owned();
    set_mode(&prog, 0o444);

    let out = run_args(&["fmt", &path]);
    set_mode(&prog, 0o644);
    assert_ne!(out.code, Some(0));
    assert!(out.stderr.contains("cannot write"), "{}", out.stderr);
}

#[test]
fn ml_formatter_repairs_dedented_match_suffix_and_is_idempotent() {
    let source = "choose n = match n\n    0 => 7\n\n_ => 9\n";
    let expected = "choose n = match n\n    0 => 7\n\n    _ => 9\n";
    let repaired = run_args_with_stdin(&["fmt", "--flavor=ml", "-"], source);
    assert_eq!(repaired.code, Some(0), "{}", repaired.stderr);
    assert_eq!(repaired.stdout, expected);
    let repeated = run_args_with_stdin(&["fmt", "--flavor=ml", "-"], &repaired.stdout);
    assert_eq!(repeated.code, Some(0), "{}", repeated.stderr);
    assert_eq!(repeated.stdout, expected);
}

#[test]
fn ml_formatter_rejects_syntax_that_indenting_cannot_repair() {
    let source = "choose n = match n\n    0 => @\n";
    let rejected = run_args_with_stdin(&["fmt", "--flavor=ml", "-"], source);
    assert_ne!(rejected.code, Some(0));
    assert!(rejected.stdout.is_empty(), "{}", rejected.stdout);
    assert!(
        rejected.stderr.contains("unexpected character '@'"),
        "{}",
        rejected.stderr
    );
}

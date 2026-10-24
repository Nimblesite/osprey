//! Saved reports exercise the complete Hunt pipeline using its existing fake engine.
use super::*;

#[test]
#[cfg(unix)]
fn hunt_saves_both_reports_by_default_and_keeps_json_stdout_parseable() {
    let scene = Scene::new("saved-reports", "print(\"hello\")\n");
    scene.canned_finding();
    scene.canned(
        "test000001.ktest",
        &ktest::encode(&[("stdin", b"9\n"), ("entropy", b"\x01\xff")]),
    );
    let (stdout, code) = run_scene(&scene, 0, &["--json", "--no-replay"]).expect("hunt");
    assert_eq!(code, ExitCode::FAILURE);
    let directory = PathBuf::from("target/hunt").join(scratch_stem(&scene.program()));
    assert!(
        directory.join("report.html").is_file(),
        "default HTML report missing"
    );
    let saved = std::fs::read_to_string(directory.join("report.json")).expect("saved JSON");
    assert_eq!(json(&stdout), json(&saved));
    assert_eq!(
        json(&saved)["context"]["symbolicObjects"]["test000001"]["objects"][1]["bytes"],
        serde_json::json!([1, 255])
    );
    let html = std::fs::read_to_string(directory.join("report.html")).expect("HTML");
    shows(
        &html,
        &[
            "For AI",
            "Paths explored",
            "Normal exits",
            "Not confirmed in a normal run",
            "9\\n",
            "main.osp",
        ],
    );
    std::fs::remove_dir_all(directory).expect("remove reports");
}

#[test]
#[cfg(unix)]
fn report_opt_outs_are_independent_and_preserve_the_analysis_result() {
    let scene = Scene::new("report-opt-outs", "print(\"hello\")\n");
    scene.canned_clean_run();
    for (flags, html, json_file) in [
        (vec!["--no-html"], false, true),
        (vec!["--no-json"], true, false),
        (vec!["--no-html", "--no-json"], false, false),
    ] {
        let out = scene.root.join(format!("reports-{html}-{json_file}"));
        let path = out.display().to_string();
        let mut args = vec!["--out", &path, "--json"];
        args.extend(flags);
        let (stdout, code) = run_scene(&scene, 0, &args).expect("hunt");
        assert_eq!(code, ExitCode::SUCCESS);
        assert_eq!(json(&stdout)["engine"], "KLEE");
        assert_eq!(out.join("report.html").exists(), html);
        assert_eq!(out.join("report.json").exists(), json_file);
    }
}

#[test]
#[cfg(unix)]
fn html_safely_explains_an_unsupported_operation_and_embeds_raw_ai_context() {
    let scene = Scene::new("report-blocked", "print(\"<script>alert(1)</script>\")\n");
    scene.canned_clean_run();
    let err = format!(
        "Error: unmodelled: pipe\nStack: \n\t#000000001 in main() at {}:1\n",
        "main.osp"
    );
    scene.canned("test000001.unmodelled.err", err.as_bytes());
    let out = scene.root.join("report").display().to_string();
    let (stdout, _) = run_scene(&scene, 0, &["--out", &out, "--json"]).expect("hunt");
    let value = json(&stdout);
    assert!(
        value["context"]["engineReports"]["test000001.unmodelled.err"]
            .as_str()
            .is_some_and(|s| s.contains("unmodelled: pipe"))
    );
    let html = std::fs::read_to_string(Path::new(&out).join("report.html")).expect("HTML");
    shows(
        &html,
        &[
            "Analysis blocked by an unsupported operation",
            "No program bug was demonstrated",
            "Starting a process needs pipe",
            "For AI",
            "&lt;script&gt;",
            "line selected",
        ],
    );
    assert!(!html.contains("<script>"));
}

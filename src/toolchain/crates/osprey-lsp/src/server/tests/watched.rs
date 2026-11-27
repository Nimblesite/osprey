use super::features::sweep_uri;
use super::*;

/// A project that reaches a library through `source_roots = ["../lib/src"]`
/// follows every edit of that library file, whether its own file is closed or
/// open: each watched change republishes it with the library's current state,
/// never an analysis of an earlier version; a library buffer that does not load
/// yet falls back to the saved file; and a saved fix clears the error.
/// Implements [LSP-DIAGNOSTICS-SCOPE].
#[tokio::test]
async fn projects_follow_edits_to_a_shared_source_root() -> Result<(), Box<dyn std::error::Error>> {
    const MAIN: &str = "namespace app;\nprint(Keys::one())\n";
    const RESERVED: &str =
        "namespace app;\nmodule Keys {\n    export fn one() = 1\n    fn signature() = 2\n}\n";
    const BROKEN: &str = "namespace app;\nmodule Keys {\n    export fn one() = (1 +\n}\n";
    const FIXED: &str = "namespace app;\nmodule Keys {\n    export fn one() = 1\n}\n";
    const RED: Option<&str> = Some("`signature` is reserved");
    let root = std::env::temp_dir().join(format!("osprey shared root {}", std::process::id()));
    let (app, lib) = (root.join("app"), root.join("lib"));
    std::fs::create_dir_all(app.join("src"))?;
    std::fs::create_dir_all(lib.join("src"))?;
    std::fs::write(
        app.join("osprey.toml"),
        "[project]\nsource_roots = [\"src\", \"../lib/src\"]\nentry = \"src/main.osp\"\n",
    )?;
    std::fs::write(
        lib.join("osprey.toml"),
        "[project]\nsource_roots = [\"src\"]\n",
    )?;
    let (main, keys) = (app.join("src/main.osp"), lib.join("src/keys.osp"));
    std::fs::write(&main, MAIN)?;
    std::fs::write(&keys, RESERVED)?;
    let lib = Library {
        main: sweep_uri(&main)?,
        path: keys,
        uri: sweep_uri(&lib.join("src/keys.osp"))?,
    };
    let mut h = Harness::start();
    let folder = json!({"rootUri": sweep_uri(&root)?});
    let _ = h.request(700, "initialize", with_progress(folder)).await;
    h.notify("initialized", json!({})).await;
    assert_latest(&settled(&mut h, 701).await, &lib.main, RED, "the sweep");
    let edits = [
        (BROKEN, Some("syntax error")),
        (RESERVED, RED),
        (FIXED, None),
    ];
    lib.saved(&mut h, 702, &edits, "a disk edit, main closed")
        .await?;
    let _ = h.open_at(&lib.main, MAIN).await;
    lib.saved(&mut h, 712, &edits, "a disk edit, main open")
        .await?;
    let _ = h.open_at(&lib.uri, FIXED).await;
    for (version, text) in (2..).zip([BROKEN, RESERVED, FIXED]) {
        let change = json!({"textDocument":{"uri":lib.uri,"version":version},"contentChanges":[{"text":text}]});
        h.notify("textDocument/didChange", change).await;
        assert_latest(
            &settled(&mut h, 720 + version).await,
            &lib.main,
            None,
            "a live edit over the saved fix",
        );
    }
    std::fs::write(&lib.path, RESERVED)?;
    h.notify("textDocument/didClose", text_doc(&lib.uri)).await;
    assert_latest(
        &settled(&mut h, 730).await,
        &lib.main,
        RED,
        "closing a clean buffer over a red file",
    );
    lib.saved(
        &mut h,
        731,
        &[(FIXED, None)],
        "saving the fix after the close",
    )
    .await?;
    h.shutdown_and_exit().await;
    std::fs::remove_dir_all(root)?;
    Ok(())
}

/// A library file and the program that reaches it through a source root.
struct Library {
    main: String,
    path: std::path::PathBuf,
    uri: String,
}

impl Library {
    /// Saves each text, reports it through the client's watcher, and checks
    /// the program's latest diagnostics against its expected error.
    async fn saved(
        &self,
        h: &mut Harness,
        id: i64,
        edits: &[(&str, Option<&str>)],
        what: &str,
    ) -> std::io::Result<()> {
        for (id, (text, needle)) in (id..).zip(edits) {
            std::fs::write(&self.path, text)?;
            let changes = json!({"changes":[{"uri":self.uri,"type":2}]});
            h.notify("workspace/didChangeWatchedFiles", changes).await;
            assert_latest(&settled(h, id).await, &self.main, *needle, what);
        }
        Ok(())
    }
}

/// The last report for `uri` mentions `needle`, or is clean when it is `None`.
fn assert_latest(reports: &[Value], uri: &str, needle: Option<&str>, what: &str) {
    let Some(latest) = reports.iter().rev().find(|p| p["uri"] == uri) else {
        panic!("{what} never republished {uri}: {reports:?}");
    };
    let shown = format!("{}", latest["diagnostics"]);
    match needle {
        Some(needle) => assert!(
            shown.contains(needle),
            "{what}: {uri} should report {needle}: {shown}"
        ),
        None => assert_eq!(shown, "[]", "{what}: {uri} stays red"),
    }
}

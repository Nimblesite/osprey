//! Discovery lists a project's sources in one order on every OS, whichever way the root is spelled.
//! A Windows canonical root carries the verbatim prefix `\\?\`, under which joining a source root
//! such as `../lib` resolves its `..`; the sources then sorted by their resolved folders, unlike
//! every other OS, and the program was assembled in another order. Implements [PROJECT-SOURCES].

use osprey_project::discover;
use std::path::{Path, PathBuf};

const MANIFEST: &str = "[project]\nname = \"app\"\nsource_roots = [\".\", \"../zlib\", \"../lib\"]\nentry = \"main.osp\"\n";

fn fixture(name: &str) -> std::io::Result<PathBuf> {
    let root = std::env::temp_dir().join(format!("osprey_source_order_{}_{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for folder in ["app", "lib", "zlib"] {
        std::fs::create_dir_all(root.join(folder))?;
    }
    std::fs::write(root.join("app/osprey.toml"), MANIFEST)?;
    for file in ["app/main.osp", "app/b.osp", "lib/a.osp", "lib/m.osp", "zlib/z.osp"] {
        std::fs::write(root.join(file), "print(\"x\")\n")?;
    }
    Ok(root)
}

// Each discovered source as its folder and file name, in discovery order.
fn listed(root: &Path) -> Vec<String> {
    let (_, paths) = discover(root).unwrap_or_else(|errors| panic!("discovery failed: {errors:?}"));
    paths
        .iter()
        .map(|path| {
            let folder = path.parent().and_then(Path::file_name).map(|name| name.to_string_lossy().into_owned());
            format!("{}/{}", folder.unwrap_or_default(), path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default())
        })
        .collect()
}

#[test]
fn a_canonical_root_lists_its_sources_in_the_order_a_written_root_does() -> std::io::Result<()> {
    let root = fixture("canonical")?;
    let written = root.join("app");
    let canonical = std::fs::canonicalize(&written)?;
    // Sources reached through `..` come first, by the root as the manifest spells it, then the project's own.
    let expected = ["lib/a.osp", "lib/m.osp", "zlib/z.osp", "app/b.osp", "app/main.osp"];
    assert_eq!(listed(&written), expected, "written root");
    assert_eq!(listed(&canonical), expected, "canonical root {}", canonical.display());
    let again = listed(&canonical);
    assert_eq!(again, expected, "a second discovery is the same");
    std::fs::remove_dir_all(&root)
}

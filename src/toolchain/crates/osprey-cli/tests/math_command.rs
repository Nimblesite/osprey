//! [MATH-CODE-COMMANDS], [MATH-CODE-SOURCES]: `osprey math` prints the module a Lean source
//! generates and writes nothing, and the real compiler builds and runs a project whose
//! mathematics is a `.lean` file. Each answer is the real binary's: output and exit code.

use std::path::PathBuf;
use std::process::{Command, Output};

const KERNELS: &str = include_str!("../../osprey-math/tests/fixtures/Kernels.lean");
const GENERATED: &str = include_str!("../../osprey-math/tests/fixtures/Kernels.ospml");
const MAIN: &str = "namespace synth\n\nmain () =\n    print \"${Kernels::wrap 2.75} ${Kernels::blep (0.01, 0.02)} ${Kernels::sine 0.25} ${Kernels::inside (0.1, 0.5, true)} ${Kernels::pick 3.0}\"\n";
const MANIFEST: &str = "[project]\nname = \"synth\"\nsource_roots = [\"src\"]\ndefault_namespace = \"synth\"\nentry = \"src/main.ospml\"\n";

/// A project directory of its own, removed when the test ends.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("osprey_math_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            std::fs::create_dir_all(dir.join("src")).is_ok(),
            "{}",
            dir.display()
        );
        Self(dir)
    }

    fn write(&self, file: &str, text: &str) -> PathBuf {
        let path = self.0.join(file);
        assert!(std::fs::write(&path, text).is_ok(), "{}", path.display());
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn osprey(args: &[&str]) -> (Option<i32>, String, String) {
    let ran = Command::new(env!("CARGO_BIN_EXE_osprey"))
        .args(args)
        .output();
    let Ok(Output {
        status,
        stdout,
        stderr,
    }) = ran
    else {
        return (None, String::new(), "the compiler did not start".into());
    };
    let text = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).into_owned();
    (status.code(), text(stdout), text(stderr))
}

#[test]
fn osprey_math_prints_the_generated_module_and_writes_nothing() {
    let scratch = Scratch::new("prints");
    let source = scratch.write("src/Kernels.lean", KERNELS);
    let path = source.to_string_lossy();
    assert_eq!(
        osprey(&["math", &path]),
        (Some(0), GENERATED.into(), String::new())
    );
    assert!(
        !scratch.0.join(".osprey").exists(),
        "printing a module stores nothing"
    );

    let refused = scratch.write(
        "src/Round.lean",
        "def tie (x : Float) : Float :=\n  Float.round x\n",
    );
    let (code, out, err) = osprey(&["math", &refused.to_string_lossy()]);
    assert_eq!((code, out.as_str()), (Some(1), ""), "{err}");
    let expected = format!("{}:2:2: UnsupportedFunction: ", refused.display());
    assert!(
        err.starts_with(&expected) && err.contains("Float.round"),
        "{err}"
    );

    let missing = scratch.0.join("src/Missing.lean");
    let (code, out, err) = osprey(&["math", &missing.to_string_lossy()]);
    assert_eq!((code, out.as_str()), (Some(1), ""), "{err}");
    assert!(
        err.starts_with(&format!("{}: ", missing.display())),
        "{err}"
    );

    for args in [&["math"][..], &["math", &path, &path]] {
        let (code, out, err) = osprey(args);
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (Some(2), "", "usage: osprey math <file.lean>\n")
        );
    }
    let (_, _, usage) = osprey(&["--no-such-flag"]);
    assert!(usage.contains("osprey math <file.lean>"), "{usage}");
}

#[test]
fn a_project_whose_mathematics_is_lean_builds_and_runs() {
    let scratch = Scratch::new("runs");
    let _ = scratch.write("osprey.toml", MANIFEST);
    let _ = scratch.write("src/main.ospml", MAIN);
    let _ = scratch.write("src/Kernels.lean", KERNELS);
    let root = scratch.0.to_string_lossy().into_owned();
    for memory in ["--memory=gc", "--memory=arc"] {
        let (code, out, err) = osprey(&[&root, "--run", "--quiet", "--no-warnings", memory]);
        assert_eq!(
            (code, out.as_str()),
            (Some(0), "0.75 -0.25 1.0 true 0.7320508075688772\n"),
            "{memory}: {err}"
        );
    }
    let stored = scratch.0.join(".osprey/math/src/Kernels.lean.ospml");
    assert_eq!(
        std::fs::read_to_string(stored).ok().as_deref(),
        Some(GENERATED)
    );

    let _ = scratch.write(
        "src/Kernels.lean",
        &KERNELS.replace("Float.floor", "Float.round"),
    );
    let (code, out, err) = osprey(&[&root, "--check", "--quiet"]);
    assert_ne!(
        code,
        Some(0),
        "a refused math source stops the build: {out}{err}"
    );
    assert!(
        err.contains("Kernels.lean") && err.contains("UnsupportedFunction"),
        "{err}"
    );
}

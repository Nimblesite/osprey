//! A union declared in one namespace is matched in another through its
//! qualified constructors, on both surfaces [MODULES-RESOLUTION] (issue #282),
//! and an exported module union is matched by its bare variants wherever the
//! module's members are imported.

mod support;

use osprey_project::ProjectConfig;
use osprey_syntax::Flavor;
use support::{assembles_cleanly, config, parsed};

const ML_LIBRARY: &str =
    "namespace lib\n\ntype Outcome =\n    Done\n        result : string\n    Failed\n        reason : string\n";
const ML_ENTRY: &str = "namespace app\n\nshow outcome = match outcome\n    lib::Done result => result\n    lib::Failed reason => reason\n\nprint (show (lib::Done(result = \"yes\")))\n";
const DEFAULT_LIBRARY: &str =
    "namespace lib;\n\ntype Outcome = Done { result: string } | Failed { reason: string }\n";
const DEFAULT_ENTRY: &str = "namespace app;\n\nfn show(outcome) = match outcome {\n    lib::Done { result } => result\n    lib::Failed { reason } => reason\n}\n\nfn main() = print(show(lib::Done { result: \"yes\" }))\n";

const TS_LIBRARY: &str = "// osprey: namespace=lib\n// osprey: module=Lib\nexport type Outcome = { readonly kind: \"done\"; readonly result: string } | { readonly kind: \"failed\"; readonly reason: string };\nexport function lib(): Outcome {\n  return { kind: \"failed\", reason: \"no\" };\n}\n";
const TS_ENTRY: &str = "import lib from \"lib\";\nfunction show(outcome) {\n  switch (outcome.kind) {\n    case \"done\":\n      return outcome.result;\n    case \"failed\":\n      return \"failed\";\n  }\n}\nconsole.log(show(lib()));\n";

fn checks(flavor: Flavor, extension: &str, library: &str, entry: &str) {
    let entry_path = format!("src/main.{extension}");
    let sources = [
        parsed(&format!("src/lib.{extension}"), flavor, library),
        parsed(&entry_path, flavor, entry),
    ];
    let wildcards = ProjectConfig {
        allow_wildcard_imports: true,
        ..config(&entry_path)
    };
    assembles_cleanly(&wildcards, &sources);
}

#[test]
fn ml_matches_a_union_declared_in_another_namespace() {
    checks(Flavor::Ml, "ospml", ML_LIBRARY, ML_ENTRY);
}

#[test]
fn default_matches_a_union_declared_in_another_namespace() {
    checks(Flavor::Default, "osp", DEFAULT_LIBRARY, DEFAULT_ENTRY);
}

/// Both a destructuring case and a payload-free case name variants of a union
/// another module exports. [FLAVOR-TS-UNIONS]
#[test]
fn typescript_matches_the_variants_of_an_imported_union() {
    checks(Flavor::TypeScript, "ts", TS_LIBRARY, TS_ENTRY);
}

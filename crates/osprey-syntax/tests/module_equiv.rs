//! Cross-flavor canonical-AST contracts for [MODULES-FLAVOR-PROJECTION].

use osprey_syntax::{parse_program_with_flavor, Flavor};

fn canonical(source: &str, flavor: Flavor) -> String {
    let parsed = parse_program_with_flavor(source, flavor);
    assert!(
        parsed.errors.is_empty(),
        "{flavor} syntax errors: {:?}",
        parsed.errors
    );
    scrub_positions(&format!("{:?}", parsed.program))
}

fn scrub_positions(debug: &str) -> String {
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug;
    while let Some(start) = rest.find("Position {") {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        match rest.find('}') {
            Some(end) => rest = &rest[end.saturating_add(1)..],
            None => break,
        }
    }
    out.push_str(rest);
    out.replace("position: Some()", "position: None")
}

fn assert_equivalent(default: &str, ml: &str) {
    assert_eq!(
        canonical(default, Flavor::Default),
        canonical(ml, Flavor::Ml)
    );
}

#[test]
fn file_namespace_and_explicit_export_module_are_equivalent() {
    let default = concat!(
        "namespace billing;\n",
        "module Tax {\n",
        "  export fn addTax(cents) = cents\n",
        "}\n",
    );
    let ml = concat!(
        "namespace billing\n",
        "module Tax\n",
        "    export addTax cents = cents\n",
    );
    assert_equivalent(default, ml);
}

#[test]
fn ascribed_signature_module_needs_no_redundant_ml_export() {
    let default = concat!(
        "signature TaxApi {\n",
        "  fn addTax(cents: int) -> int\n",
        "}\n",
        "module Tax : TaxApi {\n",
        "  fn addTax(cents) = cents\n",
        "}\n",
    );
    let ml = concat!(
        "signature TaxApi\n",
        "    addTax : int -> int\n",
        "module Tax : TaxApi\n",
        "    addTax cents = cents\n",
    );
    assert_equivalent(default, ml);
}

#[test]
fn state_module_shapes_are_equivalent_without_redundant_ml_module() {
    let default = concat!(
        "state module Counter {\n",
        "  mut count = 0\n",
        "  export effect CounterFx { read: fn() -> int }\n",
        "  export fn run(action) =\n",
        "    handle CounterFx read => count in action()\n",
        "}\n",
    );
    let ml = concat!(
        "state Counter\n",
        "    mut count = 0\n",
        "    export effect CounterFx\n",
        "        read : Unit => int\n",
        "    export run action =\n",
        "        handle CounterFx\n",
        "            read => count\n",
        "        in\n",
        "            action ()\n",
    );
    assert_equivalent(default, ml);
}

#[test]
fn whole_alias_and_member_imports_are_equivalent() {
    let default = concat!(
        "import billing::Tax\n",
        "import billing::Tax as T\n",
        "import billing::Tax::{addTax, zero as noTax}\n",
    );
    let ml = concat!(
        "import billing::Tax\n",
        "import billing::Tax as T\n",
        "import billing::Tax\n",
        "    addTax\n",
        "    zero as noTax\n",
    );
    assert_equivalent(default, ml);
}

#[test]
fn qualified_uncurried_calls_are_equivalent() {
    let default = "let gross = Tax::addTax(100, 2)\n";
    let ml = "gross = Tax::addTax (100, 2)\n";
    assert_equivalent(default, ml);
}

#[test]
fn qualified_curried_calls_are_equivalent() {
    let default = "let gross = Tax::addTax(100)(2)\n";
    let ml = "gross = Tax::addTax 100 2\n";
    assert_equivalent(default, ml);
}

#[test]
fn unit_arrow_domains_are_zero_arity_even_when_nested() {
    let default = concat!(
        "signature RunnerApi {\n",
        "  fn tick() -> int\n",
        "  fn run(action: fn() -> int) -> int\n",
        "}\n",
        "module Runner : RunnerApi {\n",
        "  fn tick() -> int = 1\n",
        "  fn run(action: fn() -> int) -> int = action()\n",
        "}\n",
    );
    let ml = concat!(
        "signature RunnerApi\n",
        "    tick : Unit -> int\n",
        "    run : (Unit -> int) -> int\n",
        "module Runner : RunnerApi\n",
        "    tick : Unit -> int\n",
        "    tick () = 1\n",
        "    run : (Unit -> int) -> int\n",
        "    run action = action ()\n",
    );
    assert_equivalent(default, ml);
}

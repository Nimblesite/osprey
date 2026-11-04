use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

const BINDINGS: &str = "double x = x * 2\ndescribe value = \"π${value + 1}\"\n";
const LAYOUT: &str = "effect Clock\n    now : Unit => int\nread () =\n    handle Clock\n        now => 42\n    perform Clock.now ()\n";
const NESTED: &str =
    "work () =\n    value = (\\() =>\n        first = 3\n        first + 1)\n    value ()\n";
const INVALID: &str = "describe x = \"${x + }\"\n";
const ORIGINAL: &str = "sum value = \"${value + 1}\"\n";
const CHANGED: &str = "sum value = \"${value - 1}\"\n";
const OPERATOR_BYTE: usize = 21;
const OPERATOR_END: usize = OPERATOR_BYTE + 1;
const MALFORMED: [&str; 6] = [
    "construct = Box(item =)\n",
    "double x =\n    x +\n",
    "describe x = \"${}\"\n",
    "describe x = \"${f(}\"\n",
    "type Amount\n",
    "type Amount =\n",
];

fn parser() -> Parser {
    let mut parser = Parser::new();
    parser
        .set_language(&super::LANGUAGE.into())
        .expect("ML grammar loads");
    parser
}

#[track_caller]
fn assert_has_error(tree: &Tree, source: &str) {
    assert!(
        tree.root_node().has_error(),
        "{source}: {}",
        tree.root_node().to_sexp()
    );
}

fn parse(source: &str) -> Tree {
    let tree = parser().parse(source, None).expect("native ML tree");
    assert!(
        !tree.root_node().has_error(),
        "{}",
        tree.root_node().to_sexp()
    );
    tree
}

fn nodes<'tree>(node: Node<'tree>, kind: &str) -> Vec<Node<'tree>> {
    let mut found = Vec::new();
    if kind.is_empty() || node.kind() == kind {
        found.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        found.extend(nodes(child, kind));
    }
    found
}

#[track_caller]
fn counted<'tree>(root: Node<'tree>, kind: &str, expected: usize) -> Vec<Node<'tree>> {
    let found = nodes(root, kind);
    assert_eq!(found.len(), expected, "unexpected {kind} count");
    found
}

#[track_caller]
fn sole_slice<'a>(source: &'a str, kind: &str) -> &'a str {
    let tree = parse(source);
    &source[counted(tree.root_node(), kind, 1)[0].byte_range()]
}

fn replace(tree: &mut Tree, byte: usize, old_len: usize, new_len: usize, row: usize, col: usize) {
    tree.edit(&InputEdit {
        start_byte: byte,
        old_end_byte: byte + old_len,
        new_end_byte: byte + new_len,
        start_position: Point::new(row, col),
        old_end_position: Point::new(row, col + old_len),
        new_end_position: Point::new(row, col + new_len),
    });
}

#[test]
fn interpolation_retains_native_operators_and_unicode_byte_ranges() {
    let tree = parse(BINDINGS);
    let root = tree.root_node();
    counted(root, "function_declaration", 2);
    counted(root, "interpolation", 1);
    let expressions = counted(root, "binary_expression", 2);
    let slices: Vec<_> = expressions
        .iter()
        .map(|node| &BINDINGS[node.byte_range()])
        .collect();
    assert_eq!(slices, ["x * 2", "value + 1"]);
    let operator = expressions[1]
        .child_by_field_name("operator")
        .expect("operator");
    assert_eq!(&BINDINGS[operator.byte_range()], "+");
    assert_eq!(operator.start_position().row, 1);
}

#[test]
fn layout_effects_handlers_and_nested_bracket_regions_are_native_nodes() {
    let tree = parse(LAYOUT);
    counted(tree.root_node(), "effect_declaration", 1);
    counted(tree.root_node(), "handler_arm", 1);
    counted(tree.root_node(), "perform_expression", 1);
    let nested = parse(NESTED);
    counted(nested.root_node(), "block", 2);
    counted(nested.root_node(), "function_declaration", 3);
    counted(nested.root_node(), "lambda_expression", 1);
}

#[test]
fn invalid_interpolation_is_a_visible_parse_error() {
    let tree = parser().parse(INVALID, None).expect("error tree");
    assert_has_error(&tree, INVALID);
}

#[test]
fn malformed_fields_bodies_and_interpolation_never_parse_cleanly() {
    for source in MALFORMED {
        let tree = parser().parse(source, None).expect("visible recovery tree");
        assert_has_error(&tree, source);
    }
}

#[test]
fn incremental_operator_edit_matches_fresh_parse_and_ranges() {
    let mut parser = parser();
    let mut original = parse(ORIGINAL);
    replace(&mut original, OPERATOR_BYTE, 1, 1, 0, OPERATOR_BYTE);
    let changed = parser
        .parse(CHANGED, Some(&original))
        .expect("incremental parse");
    assert!(!changed.root_node().has_error());
    assert_eq!(
        changed.root_node().to_sexp(),
        parse(CHANGED).root_node().to_sexp()
    );
    let binaries = counted(changed.root_node(), "binary_expression", 1);
    let operator = binaries[0]
        .child_by_field_name("operator")
        .expect("operator field");
    assert_eq!(operator.byte_range(), OPERATOR_BYTE..OPERATOR_END);
    assert_eq!(&CHANGED[operator.byte_range()], "-");
}

fn ml_sources(directory: &std::path::Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut sources = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .file_name()
            .is_some_and(|name| name == "failscompilation")
        {
            continue;
        }
        if path.is_dir() {
            sources.extend(ml_sources(&path)?);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "ospml")
        {
            sources.push(path);
        }
    }
    sources.sort();
    Ok(sources)
}

#[test]
fn abstract_signature_types_preserve_the_following_members() {
    let source = "signature MoneyApi\n    type Amount\n    fromCents : int -> Amount\n";
    assert_eq!(sole_slice(source, "signature_type"), "type Amount");
    assert_eq!(
        sole_slice(source, "ml_signature"),
        "fromCents : int -> Amount"
    );
}

#[test]
fn empty_maps_and_function_lists_remain_distinct_collections() {
    let source = "empty = [=>]\ncallbacks = [(\\x => x + 1)]\nvalues = [\"key\" => 3]\nbare = [\\x => x + 1]\n";
    let tree = parse(source);
    let maps = counted(tree.root_node(), "map_literal", 2);
    assert_eq!(&source[maps[0].byte_range()], "[=>]");
    let lists = counted(tree.root_node(), "list_literal", 2);
    for list in lists {
        counted(list, "lambda_expression", 1);
    }
    counted(maps[1], "map_entry", 1);
}

#[test]
fn layout_types_and_effect_rows_retain_structural_nodes() {
    let source = "type Envelope T U =\n    payload : T\n    metadata : U\ntype Tree T =\n    Leaf\n    Node\n        value : T\nread : Unit -> List<int> ! [Clock]\n";
    let tree = parse(source);
    let root = tree.root_node();
    counted(root, "type_declaration", 2);
    counted(root, "type_parameter", 3);
    counted(root, "field_declaration", 3);
    counted(root, "ml_variant", 2);
    counted(root, "ml_effect_row", 1);
    counted(root, "generic_type", 1);
}

#[test]
fn multiargument_type_application_stays_one_signature_statement() {
    let source = "value : Result string Error\n";
    let tree = parse(source);
    counted(tree.root_node(), "statement", 1);
    counted(tree.root_node(), "type_identifier", 3);
    assert_eq!(sole_slice(source, "ml_signature"), source.trim_end());
}

#[test]
fn tupled_and_curried_parameter_shapes_remain_distinct() {
    let source = "flat (x, y) = x + y\ncurried x y = x + y\n";
    let tree = parse(source);
    let functions = counted(tree.root_node(), "function_declaration", 2);
    counted(functions[0], "tuple_parameters", 1);
    counted(functions[1], "tuple_parameters", 0);
    for function in functions {
        counted(function, "parameter", 2);
    }
}

#[test]
fn concurrency_keywords_have_native_operation_nodes() {
    let source = "work ch =\n    worker = spawn\n        send ch 7\n        yield 1\n        9\n    seen = recv ch\n    await worker\n";
    let tree = parse(source);
    for kind in [
        "spawn_expression",
        "await_expression",
        "send_expression",
        "recv_expression",
        "yield_expression",
    ] {
        counted(tree.root_node(), kind, 1);
    }
}

#[test]
fn negative_argument_spacing_preserves_calls_and_subtraction() {
    let source = "applied = f -3\ntight = f-3\nspaced = f - 3\n";
    let tree = parse(source);
    let functions = counted(tree.root_node(), "function_declaration", 3);
    counted(functions[0], "call_expression", 1);
    counted(functions[0], "binary_expression", 0);
    for function in &functions[1..] {
        let binary = counted(*function, "binary_expression", 1);
        let operator = binary[0]
            .child_by_field_name("operator")
            .expect("subtraction operator");
        assert_eq!(&source[operator.byte_range()], "-");
        counted(*function, "call_expression", 0);
    }
}

fn ranges(tree: &Tree) -> Vec<(String, std::ops::Range<usize>, Point, Point)> {
    nodes(tree.root_node(), "")
        .into_iter()
        .map(|node| {
            (
                node.kind().to_owned(),
                node.byte_range(),
                node.start_position(),
                node.end_position(),
            )
        })
        .collect()
}

#[test]
fn incremental_nested_layout_edit_preserves_every_node_range(
) -> Result<(), Box<dyn std::error::Error>> {
    let start = NESTED.find('3').ok_or("nested literal")?;
    let changed = NESTED.replacen('3', "30", 1);
    let mut original = parse(NESTED);
    replace(&mut original, start, 1, 2, 2, 16);
    let mut parser = parser();
    let incremental = parser
        .parse(&changed, Some(&original))
        .ok_or("incremental tree")?;
    let fresh = parse(&changed);
    assert!(!incremental.root_node().has_error());
    assert_eq!(
        incremental.root_node().to_sexp(),
        fresh.root_node().to_sexp()
    );
    assert_eq!(ranges(&incremental), ranges(&fresh));
    Ok(())
}

#[test]
fn real_ml_programs_parse_without_errors() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../osprey");
    let files = [
        ml_sources(&root.join("tests"))?,
        ml_sources(&root.join("examples"))?,
    ]
    .concat();
    assert!(
        files.len() >= 203,
        "the real-program corpus must not shrink"
    );
    let mut parser = parser();
    let mut failures = Vec::new();
    for path in &files {
        let source = std::fs::read_to_string(path)?;
        let tree = parser.parse(&source, None).ok_or("missing parse tree")?;
        if tree.root_node().has_error() {
            failures.push(path.strip_prefix(root)?.display().to_string());
        }
    }
    assert!(
        failures.is_empty(),
        "{}/{} native ML files failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
    Ok(())
}

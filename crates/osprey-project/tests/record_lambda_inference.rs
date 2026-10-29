//! Type inference over a record whose field holds a function, called through
//! that field from a mutually recursive descent, stays fast [TYPE-INFERENCE].
//! The 35-line source below checks in about 20 s; with the field replaced by a
//! tag it checks in under one. Found by apps/workbench's formula reader.

mod support;

use std::time::{Duration, Instant};

use osprey_project::assemble;
use osprey_syntax::Flavor;
use support::{config, parsed};

const SOURCE: &str = r#"namespace app

type F = V int | Add F F | Mul F F

type P = Parsed F int | Failed string

type Level =
    mark : int
    build : (F, F) -> F

levels () = [Level(mark = 1, build = \(a, b) => Add a b), Level(mark = 2, build = \(a, b) => Mul a b)]

climb (xs, at, depth) = match listGet (levels ()) depth
    Error _ => atom (xs, at)
    Success level => match climb (xs, at, satAdd depth 1)
        Failed m => Failed m
        Parsed left next => further (xs, depth, level, left, next)

further (xs, depth, level, left, at) = match (listGet xs at ?: 0) == level.mark
    false => Parsed left at
    true => joined (xs, depth, level, left, climb (xs, satAdd at 1, satAdd depth 1))

joined (xs, depth, level, left, right) = match right
    Failed m => Failed m
    Parsed formula next => further (xs, depth, level, level.build (left, formula), next)

atom (xs, at) = match listGet xs at ?: 0
    9 => grouped (xs, at)
    n => Parsed (V n) (satAdd at 1)

grouped (xs, at) = match climb (xs, satAdd at 1, 0)
    Failed m => Failed m
    Parsed inner next => Parsed inner (satAdd next 1)

main () = match climb ([3, 1, 4], 0, 0)
    Parsed _ n => print "${n}"
    Failed m => print m
"#;

const RECORD_DEFAULTS: &str = r"namespace ui
type Frame =
    width : int
    height : int
module Defaults
    export plain = Frame(width = 1, height = 2)
    export altered = plain(width = 430)
    export resize width = plain(width = width)
";
const RECORD_UPDATES: &str = r#"namespace app
import ui::Defaults
    plain
    altered
    resize
main () =
    changed = plain(width = 10)
    print "${changed.width}:${plain.height}:${altered.width}:${(resize 20).height}"
"#;

#[test]
fn imported_record_constants_survive_updates_in_initializers_and_functions() {
    let sources = [
        parsed("src/defaults.ospml", Flavor::Ml, RECORD_DEFAULTS),
        parsed("src/main.ospml", Flavor::Ml, RECORD_UPDATES),
    ];
    let project = assemble(&config("src/main.ospml"), &sources);
    assert!(project.is_ok(), "{project:?}");
    if let Ok(project) = project {
        let errors = osprey_types::check_program(&project.program);
        assert!(errors.is_empty(), "{errors:?}");
    }
}

#[test]
fn a_function_field_called_from_a_recursive_descent_checks_quickly() {
    let entry_path = "src/main.ospml";
    let sources = [parsed(entry_path, Flavor::Ml, SOURCE)];
    let started = Instant::now();
    let project = assemble(&config(entry_path), &sources);
    assert!(project.is_ok(), "{project:?}");
    if let Ok(project) = project {
        let errors = osprey_types::check_program(&project.program);
        assert!(errors.is_empty(), "{errors:?}");
    }
    let spent = started.elapsed();
    assert!(spent < Duration::from_secs(2), "inference took {spent:?}");
}

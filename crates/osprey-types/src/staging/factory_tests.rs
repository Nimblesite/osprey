//! Static specialization of returned computation factories.

use crate::effect_rows_tests::assert_accepted;
use osprey_ast::{AstVisitor, Expr};

const FACTORY: &str = "effect Store<T> { put: fn(T) -> T }\n\
    fn use(work) = {\n\
      handle static Store<int> { put x => x }\n\
      work()\n\
    }\n\
    fn make(i) = if i == 0 { fn() => perform Store.put(1) } else { fn() => perform Store.put(2) }\n\
    fn run(i) = {\n\
      let compute = make(i)\n\
      use(fn() => compute())\n\
    }\n\
    let answer = run(0)\n";

#[derive(Default)]
struct Calls(usize);
impl AstVisitor for Calls {
    fn expression(&mut self, expression: &Expr) {
        if matches!(expression, Expr::Call { function, .. } if matches!(function.as_ref(), Expr::Identifier(name) if name.starts_with("make")))
        {
            self.0 += 1;
        }
    }
}

fn factory_calls(source: &str) -> usize {
    let parsed = osprey_syntax::parse_program(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let lowered = osprey_ast::stage::lower(&parsed.program);
    assert!(lowered.is_ok(), "{lowered:?}");
    let Ok(lowered) = lowered else { return 0 };
    let mut calls = Calls::default();
    osprey_ast::walk_program(&lowered, &mut calls);
    calls.0
}

#[test]
fn an_effectful_factory_is_not_evaluated_again_during_discharge() {
    let source = FACTORY
        .replace("if i == 0", "{ print(\"made\")\n if i == 0")
        .replace("fn run(i)", "}\nfn run(i)");
    assert_eq!(factory_calls(&source), 1);
}

#[test]
fn a_mutable_factory_argument_is_not_read_again_during_discharge() {
    let source = FACTORY.replace(
        "let compute = make(i)",
        "mut selected = i\nlet compute = make(selected)\nselected = 1",
    );
    assert_eq!(factory_calls(&source), 1);
}

const STORAGE_BACKENDS: &str =
    "effect Buffer<B> { fromList: fn(List<float>) -> B toList: fn(B) -> List<float> }\n\
         type Tensor<B> = { data: B }\n\
         fn arr(xs) = Tensor { data: perform Buffer.fromList(xs) }\n\
         fn values(a) = perform Buffer.toList(a.data)\n\
         fn host(work) = {\n\
           handle static Buffer<List<float>> { fromList xs => xs toList xs => xs }\n\
           work()\n\
         }\n\
         fn device(work) = {\n\
           handle static Buffer<int> { fromList xs => length(xs) toList id => [1.0] }\n\
           work()\n\
         }\n\
         fn apply(backend, work) = if backend == \"host\" { host(work) } else { device(work) }\n\
         fn computation(i) = if i == 0 { fn() => arr([1.0]) } else { fn() => arr([2.0]) }\n\
         fn run(backend, i) = {\n\
           let compute = computation(i)\n\
           apply(backend, fn() => values(compute()))\n\
         }\n\
         let answer = listConcat(run(\"host\", 0), run(\"device\", 1))\n";

#[test]
fn returned_tensor_computations_resolve_under_each_storage_handler() {
    assert_accepted(STORAGE_BACKENDS);
    let parsed = osprey_syntax::parse_program(STORAGE_BACKENDS);
    let lowered = crate::staging::lower_static_checked(&parsed.program);
    assert!(lowered.is_ok(), "{lowered:#?}");
    let Ok(lowered) = lowered else { return };
    let errors = crate::check_program(&lowered);
    assert!(errors.is_empty(), "residual diagnostics: {errors:#?}");
}

#[test]
fn a_named_callback_keeps_the_requirements_of_its_factory_result() {
    let source = STORAGE_BACKENDS
        .replace(
            "apply(backend, fn() => values(compute()))",
            "let work = fn() => checked(compute)\napply(backend, work)",
        )
        .replace("fn() => arr([1.0])", "fn() => values(arr([1.0]))")
        .replace("fn() => arr([2.0])", "fn() => values(arr([2.0]))")
        .replace(
            "fn run(backend, i)",
            "fn checked(work) = work()\nfn run(backend, i)",
        );
    assert_accepted(&source);
}

#[cfg(unix)]
mod unix {
    use super::super::*;

    /// Sites are located by their source text rather than by line number: they
    /// live in demo projects edited for their own reasons, and a hard-coded
    /// line makes an unrelated comment fail these tests.
    const CREDIT_PERFORM: &str = "credit id cents note";
    const CREDIT_OPERATION: &str = "credit";
    /// examples/gpt2/model/backends.osp declares `Tensor`; cpu.osp and gpu.osp
    /// discharge it once each, and model.osp performs it. All four are
    /// siblings through the generate and train manifests' `../model` source
    /// root.
    const BACKENDS: &str = "gpt2/model/backends.osp";
    const CPU: &str = "gpt2/model/cpu.osp";
    const GPU: &str = "gpt2/model/gpu.osp";
    const MODEL: &str = "gpt2/model/model.osp";
    const TENSOR: &str = "Tensor";
    const TENSOR_HANDLES: [&str; 2] = [
        "handle static Tensor<List<float>>",
        "handle static Tensor<GpuBuffer<float>>",
    ];

    type Span = (u32, u32, u32, u32);

    fn example_source(relative: &str) -> (String, String) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .join(relative);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {relative}: {error}"));
        (source, format!("file://{}", path.display()))
    }

    /// Zero-based line and column of `needle`'s first occurrence.
    fn locate(source: &str, needle: &str) -> (u32, u32) {
        let (line, column) = source
            .lines()
            .enumerate()
            .find_map(|(line, text)| text.find(needle).map(|column| (line, column)))
            .unwrap_or_else(|| panic!("`{needle}` present in the example source"));
        (
            u32::try_from(line).unwrap_or_default(),
            u32::try_from(column).unwrap_or_default(),
        )
    }

    fn width(text: &str) -> u32 {
        u32::try_from(text.len()).expect("example text fits a u32")
    }

    /// The span of `word` inside the first line holding `context`.
    fn span_in(source: &str, context: &str, word: &str) -> Span {
        let (line, column) = locate(source, context);
        let offset = context.find(word).expect("word inside its context");
        let start = column.saturating_add(width(&context[..offset]));
        (line, start, line, start.saturating_add(width(word)))
    }

    type Feature = fn(&str, &str, u32, u32, PositionEncoding) -> Vec<crate::model::Location>;

    /// Implementations of `word` where it appears inside `context`, as the
    /// file name and span of each location.
    fn implementations_at(relative: &str, context: &str, word: &str) -> Vec<(String, Span)> {
        located_at(
            crate::test_support::implementations,
            relative,
            context,
            word,
        )
    }

    /// The locations `feature` answers for `word` inside `context`, as the
    /// file name and span of each.
    fn located_at(
        feature: Feature,
        relative: &str,
        context: &str,
        word: &str,
    ) -> Vec<(String, Span)> {
        let (source, uri) = example_source(relative);
        let (line, column, ..) = span_in(&source, context, word);
        feature(&source, &uri, line, column, PositionEncoding::Utf16)
            .into_iter()
            .map(|location| {
                let file = location.uri.rsplit('/').next().unwrap_or_default();
                (file.to_owned(), location.span)
            })
            .collect()
    }

    /// The span of `word` inside each of `contexts` in `relative`.
    fn spans_in(relative: &str, contexts: &[&str], word: &str) -> Vec<(String, Span)> {
        let (source, _) = example_source(relative);
        let file = relative.rsplit('/').next().unwrap_or_default();
        contexts
            .iter()
            .map(|context| (file.to_owned(), span_in(&source, context, word)))
            .collect()
    }

    fn credit_hover(source: &str, uri: &str) -> String {
        let (line, column) = locate(source, CREDIT_PERFORM);
        let parsed = osprey_syntax::parse_program_for_path(uri, source);
        operation_hover(
            &parsed.program,
            source,
            &crate::test_support::view(uri).siblings,
            line,
            column,
            PositionEncoding::Utf16,
            parsed.flavor,
        )
        .expect("resolve the Store declaration in the ledger sibling")
    }

    // Hover resolves the operation's declaration across project files
    // ([LSP-HOVER-EFFECT-OPERATIONS]) and go-to-implementations lands on
    // the handler arm ([LSP-IMPLEMENTATIONS-EFFECT-HANDLERS]).
    #[test]
    fn effect_navigation_crosses_project_files() {
        let (source, uri) = example_source("projects/modules/src/api/routes.ospml");
        let hover = credit_hover(&source, &uri);
        crate::testkit::shows(&hover, &["bank::Ledger::Store.credit"]);
        crate::testkit::shows(&hover, &["(int, int, string) => int"]);
        let (line, column) = locate(&source, CREDIT_PERFORM);
        let locations = crate::test_support::implementations(
            &source,
            &uri,
            line,
            column,
            PositionEncoding::Utf16,
        );
        assert_eq!(locations.len(), 1, "{locations:?}");
        let handler = locations.first().expect("one Store.credit handler");
        assert!(handler.uri.ends_with("store/ledger.ospml"), "{handler:?}");
        let (ledger, _) = example_source("projects/modules/src/store/ledger.ospml");
        let (line, column) = locate(&ledger, CREDIT_PERFORM);
        let end = column.saturating_add(width(CREDIT_OPERATION));
        assert_eq!(handler.span, (line, column, line, end));
    }

    // An operation answers with its arms from the declaration and from a
    // `perform` in another file of the project, once per arm even though two
    // manifests share the `../model` source root.
    #[test]
    fn implementations_on_an_operation_list_each_handler_arm_once() {
        let expected = [
            spans_in(CPU, &["get b i => listGet"], "get"),
            spans_in(GPU, &["get b i => gpuGet"], "get"),
        ]
        .concat();
        for (relative, context) in [
            (BACKENDS, "get: fn(B, int) -> float"),
            (MODEL, "perform Backends::Tensor.get"),
        ] {
            let found = implementations_at(relative, context, "get");
            assert_eq!(found, expected, "from `{context}` in {relative}");
        }
    }

    // [LSP-IMPLEMENTATIONS-EFFECT-HANDLERS] on the effect's own name: at its
    // declaration, at a discharge, and as the qualifier of a `perform` in a
    // sibling file, the query lists every handler that discharges the effect.
    // It returned nothing: only operation names were ever resolved.
    #[test]
    fn implementations_on_an_effect_name_list_every_handler_that_discharges_it() {
        let cpu = spans_in(CPU, &TENSOR_HANDLES[..1], TENSOR);
        let gpu = spans_in(GPU, &TENSOR_HANDLES[1..], TENSOR);
        let siblings = [cpu.clone(), gpu.clone()].concat();
        // The open buffer is searched first, so gpu.osp lists its own handler first.
        for (relative, context, expected) in [
            (BACKENDS, "effect Tensor<B>", siblings.clone()),
            (GPU, TENSOR_HANDLES[1], [gpu, cpu].concat()),
            (MODEL, "perform Backends::Tensor.get", siblings),
        ] {
            let found = implementations_at(relative, context, TENSOR);
            assert_eq!(found, expected, "from `{context}` in {relative}");
        }
    }

    // A builtin effect has no declaration in the project, but a discharge in
    // it still names the effect: `Arith` lists every `handle Arith` region.
    #[test]
    fn implementations_on_a_builtin_effect_list_its_project_discharges() {
        let expected = spans_in(BACKENDS, &["handle Arith {"], "Arith");
        let found = implementations_at(BACKENDS, "handle Arith {", "Arith");
        assert_eq!(found, expected);
    }

    // An effect a `signature` declares is the same effect its module declares:
    // an operation queried from the signature lists the module's handler arms.
    #[test]
    fn implementations_on_a_signature_operation_list_the_handler_arms() {
        let metrics = "projects/modules/src/store/metrics.ospml";
        let expected = spans_in(metrics, &["hit _path =>"], "hit");
        let found = implementations_at(metrics, "hit : string => int", "hit");
        assert_eq!(found, expected);
    }

    /// Where `zipWith` is declared, answered and performed, in each flavor's
    /// GPT-2. The Python flavor spells the operation `zip_with`.
    const ZIP_DEFAULT: Operation = Operation {
        word: "zipWith",
        declaration: (BACKENDS, "zipWith: fn(B, B, Combine) -> B"),
        arms: [
            (CPU, "zipWith a b f => range"),
            (GPU, "zipWith a b f => gpuZipWith"),
        ],
        performs: &[
            (MODEL, "perform Backends::Tensor.zipWith(a, b, addF)"),
            (
                "gpt2/train/train.osp",
                "perform Backends::Tensor.zipWith(out.dx",
            ),
            (
                "gpt2/train/train.osp",
                "perform Backends::Tensor.zipWith(p, g",
            ),
        ],
    };
    const ZIP_PYTHON: Operation = Operation {
        word: "zip_with",
        declaration: ("gpt2/python/backends.py", "zip_with(a: B, b: B"),
        arms: [
            ("gpt2/python/cpu.py", "def zip_with(a, b, f)"),
            ("gpt2/python/gpu.py", "def zip_with(a, b, f)"),
        ],
        performs: &[(
            "gpt2/python/model.py",
            "perform(Tensor.zip_with(a, b, add_f))",
        )],
    };

    /// One effect operation's declaration, handler arms and `perform` sites,
    /// each a file and the text that holds the operation's name.
    struct Operation {
        word: &'static str,
        declaration: (&'static str, &'static str),
        arms: [(&'static str, &'static str); 2],
        performs: &'static [(&'static str, &'static str)],
    }

    impl Operation {
        fn span(&self, (relative, context): (&str, &str)) -> (String, Span) {
            let (source, _) = example_source(relative);
            let file = relative.rsplit('/').next().unwrap_or_default();
            (file.to_owned(), span_in(&source, context, self.word))
        }

        fn sites(&self) -> Vec<(&'static str, &'static str)> {
            [&self.arms[..], self.performs, &[self.declaration]].concat()
        }
    }

    // [LSP-DEFINITION-EFFECT-OPERATIONS]: hover on the `zipWith` arm of
    // gpu.osp's `handle static Tensor` showed `Tensor.zipWith`, but
    // go-to-definition found nothing, because it looked only for top-level
    // symbols. Every arm and `perform` lands on the operation's declaration,
    // in both flavors.
    #[test]
    fn definition_on_an_operation_lands_on_its_declaration() {
        for operation in [ZIP_DEFAULT, ZIP_PYTHON] {
            let expected = vec![operation.span(operation.declaration)];
            for (relative, context) in operation.sites() {
                let found = located_at(
                    crate::test_support::definition,
                    relative,
                    context,
                    operation.word,
                );
                assert_eq!(found, expected, "from `{context}` in {relative}");
            }
        }
    }

    // [LSP-REFERENCES-EFFECT-OPERATIONS]: find-references on an operation lists
    // its declaration, every handler arm and every `perform`, each once, in
    // both flavors.
    #[test]
    fn references_on_an_operation_list_its_declaration_arms_and_performs() {
        let with_declaration: Feature = |text, uri, line, character, encoding| {
            crate::test_support::references(text, uri, line, character, encoding, true)
        };
        for operation in [ZIP_DEFAULT, ZIP_PYTHON] {
            let mut expected: Vec<_> = operation
                .sites()
                .into_iter()
                .map(|site| operation.span(site))
                .collect();
            expected.sort();
            for (relative, context) in operation.sites() {
                let mut found = located_at(with_declaration, relative, context, operation.word);
                found.sort();
                assert_eq!(found, expected, "from `{context}` in {relative}");
            }
        }
    }
}

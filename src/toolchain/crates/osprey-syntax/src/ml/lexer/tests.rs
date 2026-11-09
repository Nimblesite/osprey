//! Lexer tests.

use super::*;

fn kinds(source: &str) -> Vec<TokKind> {
    let (tokens, errors) = lex(source);
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    tokens.into_iter().map(|t| t.kind).collect()
}

#[test]
fn lexes_binding_with_no_layout() {
    let k = kinds("x = 42\n");
    assert_eq!(
        k,
        vec![
            TokKind::Ident("x".to_owned()),
            TokKind::Eq,
            TokKind::Int(42),
            TokKind::Eof,
        ]
    );
}

#[test]
fn separates_top_level_lines_with_newline() {
    let k = kinds("a = 1\nb = 2\n");
    let newlines = k.iter().filter(|t| **t == TokKind::Newline).count();
    assert_eq!(newlines, 1, "one separator between two top-level bindings");
}

#[test]
fn indents_and_dedents_a_block() {
    let k = kinds("f =\n    g\nh = 1\n");
    assert!(k.contains(&TokKind::Indent), "block opens with Indent");
    assert!(k.contains(&TokKind::Dedent), "block closes with Dedent");
    // The Dedent must precede the sibling `h` binding.
    let dedent = k.iter().position(|t| *t == TokKind::Dedent);
    let h = k.iter().position(|t| *t == TokKind::Ident("h".to_owned()));
    assert!(dedent.is_some(), "expected a Dedent token");
    assert!(h.is_some(), "expected the `h` binding token");
    // Both positions are asserted present above, so this guard always binds
    // without an `unwrap()`.
    if let (Some(dedent), Some(h)) = (dedent, h) {
        assert!(dedent < h);
    }
}

#[test]
fn suppresses_layout_inside_parentheses() {
    // A line break inside parens must not start a new statement.
    let k = kinds("x = (1 +\n2)\n");
    assert!(!k.contains(&TokKind::Indent), "no layout inside parens");
}

#[test]
fn ignores_blank_and_comment_lines() {
    let k = kinds("a = 1\n\n// note\nb = 2\n");
    let newlines = k.iter().filter(|t| **t == TokKind::Newline).count();
    assert_eq!(newlines, 1, "blank/comment lines are not separators");
}

#[test]
fn lexes_curried_application_and_operators() {
    let k = kinds("r = add 1 2 == 3\n");
    assert!(k.contains(&TokKind::Op("==".to_owned())));
    assert!(k.contains(&TokKind::Int(1)) && k.contains(&TokKind::Int(2)));
}

#[test]
fn reports_unterminated_string() {
    let (_, errors) = lex("x = \"oops\n");
    assert!(errors.iter().any(|e| e.message.contains("unterminated")));
}

#[test]
fn skips_nesting_block_comments() {
    // `(* … *)` block comments (ML-family convention) are trivia and NEST,
    // so an inner `(* *)` does not close the outer early. The binding
    // survives with no stray tokens.
    let k = kinds("x = (* outer (* inner *) still *) 42\n");
    assert_eq!(
        k,
        vec![
            TokKind::Ident("x".to_owned()),
            TokKind::Eq,
            TokKind::Int(42),
            TokKind::Eof,
        ]
    );
}

#[test]
fn reports_unterminated_block_comment() {
    let (_, errors) = lex("x = (* never closed\n");
    assert!(errors
        .iter()
        .any(|e| e.message.contains("unterminated `(* … *)` block comment")));
}

#[test]
fn doc_comment_becomes_a_doc_token_with_stripped_text() {
    // `(** … *)` is retained as a Doc token; leading `*` margins and the
    // sigil are stripped ([DOC-SIGIL-ML]).
    let (tokens, errors) = lex("(** Doubles [x].\n    More. *)\ndouble x = x\n");
    assert!(errors.is_empty(), "{errors:?}");
    let doc = tokens.iter().find_map(|t| match &t.kind {
        TokKind::Doc(s) => Some(s.clone()),
        _ => None,
    });
    assert_eq!(doc.as_deref(), Some("Doubles [x].\nMore."));
}

#[test]
fn doc_code_fences_keep_indentation_relative_to_the_fence() {
    // ML layout is syntax, so a fenced example keeps every line's
    // indentation beyond the fence's own column ([DOC-SIGIL-ML]).
    let doc = |source: &str| {
        lex(source).0.into_iter().find_map(|t| match t.kind {
            TokKind::Doc(s) => Some(s),
            _ => None,
        })
    };
    let flush =
        "(** Wraps.\n```osprey\nw = handler Arith\n    overflow _ _ _ v => v\n```\n*)\nx = 1\n";
    let aligned = "(** Wraps.\n    ```osprey\n    w = handler Arith\n        overflow _ _ _ v => v\n    ```\n*)\nx = 1\n";
    let starred = "(** Wraps.\n * ```osprey\n * w = handler Arith\n *     overflow _ _ _ v => v\n * ```\n *)\nx = 1\n";
    let expected = "Wraps.\n```osprey\nw = handler Arith\n    overflow _ _ _ v => v\n```";
    for source in [flush, aligned, starred] {
        assert_eq!(doc(source).as_deref(), Some(expected), "{source}");
    }
}

#[test]
fn empty_and_banner_doc_openers_stay_ordinary_comments() {
    // `(**)` (empty) and `(*****)` (banner) are ordinary comments, not docs:
    // no Doc token is produced.
    let (tokens, errors) = lex("(**)\n(*****)\nx = 1\n");
    assert!(errors.is_empty(), "{errors:?}");
    assert!(
        !tokens.iter().any(|t| matches!(t.kind, TokKind::Doc(_))),
        "empty/banner openers must not yield a Doc token"
    );
}

#[test]
fn nested_string_in_interpolation_is_one_token() {
    // The inner quotes around `a` sit inside a `${…}` fragment; they must not
    // terminate the outer literal, which lexes as a SINGLE string token whose
    // raw still carries the unresolved `${ f "a" }` span.
    let (tokens, errors) = lex("x = \"v=${f \"a\"}\"\n");
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    let strings: Vec<&String> = tokens
        .iter()
        .filter_map(|t| match &t.kind {
            TokKind::Str(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        strings,
        vec![&"v=${f \"a\"}".to_owned()],
        "exactly one string token carrying the unresolved fragment",
    );
}

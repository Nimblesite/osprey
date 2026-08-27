// External scanner for tree-sitter-osprey.
//
// Two ZERO-WIDTH markers, each answering a question about the LINE a token sits
// on — the one thing the parser cannot see, because whitespace is an extra.
//
//   CALL_OPEN_GAP   succeeds when the next `(` is on the SAME LINE as the
//                   callee it would attach to.
//   STATEMENT_BREAK succeeds when the previous statement's line is over.
//
// Why the lexer and not the parser. A postfix call `f(args)` and a match arm's
// tuple pattern both open with `(`, so after an arm body
//
//     match v {
//         { held, .. } => render(held)
//         (n, s)       => "pair"
//     }
//
// the `(` on the next line is either an argument list extending the body or the
// next arm's pattern. No static precedence can express the truth: a call must
// bind TIGHTER than `+` (so `1 + id (2)` is `1 + id(2)`) yet LOOSER than the
// arm's reduce (so the tuple pattern wins) — and those two demands contradict,
// because the arm reduce must itself sit below `+`. Declaring a GLR conflict
// does not rescue it either: both readings of `1 + id (2)` contain exactly one
// spaced call, so their dynamic precedences tie and the resolver picks blind.
//
// Requiring `token.immediate('(')` instead — no whitespace at all — does settle
// it, but it rejects `print(id (1))`, which the language has always accepted.
// Deciding it HERE keeps both: horizontal space before `(` stays legal, and the
// only spelling given up is a callee and its argument list split across lines,
// which no source in the tree uses.

#include "tree_sitter/parser.h"

// Why STATEMENT_BREAK exists. Default has no statement terminator, so before
// this marker NOTHING delimited two statements: `let r = add 2 3` parsed as
// `let r = add` followed by the orphan expression-statements `2` and `3`, and
// inside a block the orphan was absorbed by the trailing block value, so
// `{ let r = double 5 }` evaluated to the ARGUMENT 5. Both readings are silent
// — well-formed trees for source that means a call. Requiring a break after
// every statement makes the newline the delimiter it always appeared to be,
// and juxtaposition a parse error. Implements [LEX-STATEMENT-BREAK].
enum TokenType {
  CALL_OPEN_GAP,
  STATEMENT_BREAK,
};

void *tree_sitter_osprey_external_scanner_create(void) { return NULL; }

void tree_sitter_osprey_external_scanner_destroy(void *payload) { (void)payload; }

// Stateless: nothing to carry across an incremental reparse.
unsigned tree_sitter_osprey_external_scanner_serialize(void *payload, char *buffer) {
  (void)payload;
  (void)buffer;
  return 0;
}

void tree_sitter_osprey_external_scanner_deserialize(void *payload, const char *buffer,
                                                     unsigned length) {
  (void)payload;
  (void)buffer;
  (void)length;
}

// Skips only HORIZONTAL space. A newline (or any other extra, such as a comment
// running to end of line) ends the line the previous token sat on, and with it
// the expression a `(` could have attached to.
static void skip_blanks(TSLexer *lexer) {
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
    // `skip = true`: consumed as whitespace, never added to the token.
    lexer->advance(lexer, true);
  }
}

// `(` only. Indexing keeps the stricter byte-adjacency rule: list-pattern
// arms really are written on one line (`match xs { [] => 0  [h, ...t] => … }`),
// so accepting a same-line `[` here reads `0  [h` as an index. Tuple-pattern
// arms have no equivalent single-line usage, which is what lets `(` take the
// looser rule. [TYPE-LIST-PATTERNS]
static bool scan_call_open_gap(TSLexer *lexer) {
  // Mark the end BEFORE consuming anything: the token must span zero bytes so
  // the `(` itself is still lexed as the ordinary `"("` the grammar and the
  // highlight queries name.
  lexer->result_symbol = CALL_OPEN_GAP;
  lexer->mark_end(lexer);
  skip_blanks(lexer);
  return lexer->lookahead == '(';
}

// Succeeds once the rest of the line holds nothing that could extend the
// statement: a newline, a `//` comment running to one, the `}` closing the
// enclosing block or namespace body, or end of file.
static bool scan_statement_break(TSLexer *lexer) {
  lexer->result_symbol = STATEMENT_BREAK;
  lexer->mark_end(lexer);
  skip_blanks(lexer);
  if (lexer->lookahead == '/') {
    // `//` and `///` both run to end of line, so either ends the statement.
    // A lone `/` is division, which continues it.
    lexer->advance(lexer, true);
    return lexer->lookahead == '/';
  }
  return lexer->lookahead == '\n' || lexer->lookahead == '\r' ||
         lexer->lookahead == '}' || lexer->eof(lexer);
}

// CALL_OPEN_GAP is tried first, and only its failure can end a statement: where
// both markers are legal — a bare callee in statement position — `f (x)` on one
// line is the call, and a `(` on the next line is a new statement.
bool tree_sitter_osprey_external_scanner_scan(void *payload, TSLexer *lexer,
                                              const bool *valid_symbols) {
  (void)payload;
  if (valid_symbols[CALL_OPEN_GAP] && scan_call_open_gap(lexer)) {
    return true;
  }
  return valid_symbols[STATEMENT_BREAK] && scan_statement_break(lexer);
}

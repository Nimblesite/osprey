// External scanner for tree-sitter-osprey.
//
// One token, one job: CALL_OPEN_GAP is a ZERO-WIDTH marker that succeeds only
// when the next `(` is on the SAME LINE as the callee it would attach to.
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

enum TokenType {
  CALL_OPEN_GAP,
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

bool tree_sitter_osprey_external_scanner_scan(void *payload, TSLexer *lexer,
                                              const bool *valid_symbols) {
  (void)payload;
  if (!valid_symbols[CALL_OPEN_GAP]) {
    return false;
  }

  // Mark the end BEFORE consuming anything: the token must span zero bytes so
  // the `(` itself is still lexed as the ordinary `"("` the grammar and the
  // highlight queries name.
  lexer->result_symbol = CALL_OPEN_GAP;
  lexer->mark_end(lexer);

  // Skip only HORIZONTAL space. A newline (or any other extra, such as a
  // comment running to end of line) ends the callee's line and therefore ends
  // the expression the `(` could have attached to.
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t') {
    // `skip = true`: consumed as whitespace, never added to the token.
    lexer->advance(lexer, true);
  }

  return lexer->lookahead == '(';
}

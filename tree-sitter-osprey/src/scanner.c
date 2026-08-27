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

// Consumes a `//`-to-end-of-line comment. Returns false on a lone `/`, which
// is division and therefore not a line ending.
static bool skip_line_comment(TSLexer *lexer) {
  lexer->advance(lexer, true);
  if (lexer->lookahead != '/') {
    return false;
  }
  while (lexer->lookahead != '\n' && lexer->lookahead != '\r' && !lexer->eof(lexer)) {
    lexer->advance(lexer, true);
  }
  return true;
}

// True when a line beginning with `first` then `second` can only CONTINUE the
// expression before it — an operator with no prefix reading, so no statement
// could start there. Unary-capable `+`, `-`, and `!` are deliberately absent:
// a line they open is a NEW statement (whose discarded value [BLOCK-DISCARD]
// then reports loudly), exactly Go's semicolon rule. `|` continues in EVERY
// spelling: `|>` pipes, `||` disjoins, a bare `|` extends a union type
// declaration (`type J = A` then `| B { .. }`) — and the one statement a `|`
// could open, a bare lambda, is itself a discarded value that cannot compile.
static bool continues_expression(int32_t first, int32_t second) {
  switch (first) {
    case '*': case '%': case '<': case '>': case '?': case ':': case '.':
    case '|':
      return true;
    case '/': return second != '/';  // division; `//` opens a comment
    case '&': return second == '&';
    case '=': return second == '=';
    case '!': return second == '=';
    default:  return false;
  }
}

// Succeeds once the statement's line is over — a newline, a `//` comment
// running to one, the `}` closing the enclosing block or namespace body, or
// end of file — UNLESS the next line's first token can only continue the
// expression (`continues_expression`), which keeps multi-line pipelines and
// trailing conditions legal. Everything here is consumed with skip=true, so a
// refusal costs nothing: the lexer restarts from the token's start.
static bool scan_statement_break(TSLexer *lexer) {
  lexer->result_symbol = STATEMENT_BREAK;
  lexer->mark_end(lexer);
  bool line_ended = false;
  for (;;) {
    skip_blanks(lexer);
    if (lexer->eof(lexer) || lexer->lookahead == '}') {
      return true;  // The encloser terminates the statement outright.
    }
    if (lexer->lookahead == '\n' || lexer->lookahead == '\r') {
      line_ended = true;
      lexer->advance(lexer, true);
      continue;
    }
    if (lexer->lookahead == '/' ) {
      if (skip_line_comment(lexer)) {
        continue;  // A comment ends the line as surely as the newline does.
      }
      return line_ended && !continues_expression('/', lexer->lookahead);
    }
    if (!line_ended) {
      return false;  // Something else on the SAME line: the statement goes on.
    }
    int32_t first = lexer->lookahead;
    lexer->advance(lexer, true);
    return !continues_expression(first, lexer->lookahead);
  }
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

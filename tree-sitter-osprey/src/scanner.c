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
#include <stdlib.h>
#include <string.h>

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
  TYPE_APPLICATION_AHEAD,
  STRING_START, STRING_END, INTERPOLATION_START, INTERPOLATION_END,
  STRING_CONTENT, ESCAPE_SEQUENCE, INTERPOLATION_SPACE,
};

// String scopes distinguish raw outer quotes from quotes escaped for the
// enclosing literal. Interpolation expressions see one layer of decoded escapes.
enum Scope { RAW_STRING = 1, QUOTED_STRING, INTERPOLATION };
#define SCOPE_CAPACITY (TREE_SITTER_SERIALIZATION_BUFFER_SIZE - 2)
typedef struct { unsigned depth; unsigned char scopes[SCOPE_CAPACITY]; } Scanner;

void *tree_sitter_osprey_external_scanner_create(void) { return calloc(1, sizeof(Scanner)); }
void tree_sitter_osprey_external_scanner_destroy(void *payload) { free(payload); }

unsigned tree_sitter_osprey_external_scanner_serialize(void *payload, char *buffer) {
  const Scanner *scanner = payload;
  if (!scanner) return 0;
  buffer[0] = (char)(scanner->depth & 255);
  buffer[1] = (char)(scanner->depth >> 8);
  memcpy(buffer + 2, scanner->scopes, scanner->depth);
  return scanner->depth + 2;
}

void tree_sitter_osprey_external_scanner_deserialize(void *payload, const char *buffer,
                                                     unsigned length) {
  Scanner *scanner = payload;
  if (!scanner) return;
  scanner->depth = 0;
  if (length < 2) return;
  unsigned depth = (unsigned char)buffer[0] | ((unsigned char)buffer[1] << 8);
  if (depth > SCOPE_CAPACITY || length != depth + 2) return;
  memcpy(scanner->scopes, buffer + 2, depth);
  scanner->depth = depth;
}

static unsigned scope(const Scanner *scanner) {
  return scanner->depth ? scanner->scopes[scanner->depth - 1] : 0;
}

static bool push_scope(Scanner *scanner, unsigned value) {
  if (scanner->depth == SCOPE_CAPACITY) return false;
  scanner->scopes[scanner->depth++] = (unsigned char)value;
  return true;
}

static void advance(TSLexer *lexer) {
  lexer->advance(lexer, false);
  lexer->mark_end(lexer);
}

// Read one source character after the enclosing string's escape decoding.
static int32_t decoded_character(TSLexer *lexer) {
  int32_t ch = lexer->lookahead;
  advance(lexer);
  if (ch != '\\') return ch;
  ch = lexer->lookahead;
  if (ch == '"' || ch == '\\') { advance(lexer); return ch; }
  if (ch == 'n' || ch == 'r' || ch == 't' || ch == 'e' || ch == '0') {
    advance(lexer);
    return ch == 'n' ? '\n' : ch == 'r' ? '\r' : ch == 't' ? '\t' : ch == 'e' ? 27 : 0;
  }
  return '\\';
}

static bool string_start(Scanner *scanner, TSLexer *lexer) {
  unsigned kind = RAW_STRING;
  if (lexer->lookahead == '\\' && scope(scanner) == INTERPOLATION) {
    advance(lexer);
    kind = QUOTED_STRING;
  }
  if (lexer->lookahead != '"' || !push_scope(scanner, kind)) return false;
  advance(lexer);
  lexer->result_symbol = STRING_START;
  return true;
}

static bool string_end(Scanner *scanner, TSLexer *lexer) {
  advance(lexer);
  scanner->depth--;
  lexer->result_symbol = STRING_END;
  return true;
}

static bool string_slash(Scanner *scanner, TSLexer *lexer, const bool *valid) {
  advance(lexer);
  if (scope(scanner) == QUOTED_STRING && lexer->lookahead == '"')
    return valid[STRING_END] && string_end(scanner, lexer);
  if (lexer->lookahead == '$') {
    lexer->result_symbol = STRING_CONTENT;
    return valid[STRING_CONTENT];
  }
  if (lexer->eof(lexer) || !valid[ESCAPE_SEQUENCE]) return false;
  if (scope(scanner) == QUOTED_STRING && lexer->lookahead == '\\') {
    advance(lexer);
    if (!lexer->eof(lexer)) decoded_character(lexer);
  } else { advance(lexer); }
  lexer->result_symbol = ESCAPE_SEQUENCE;
  return true;
}

static bool string_dollar(Scanner *scanner, TSLexer *lexer, const bool *valid) {
  advance(lexer);
  if (lexer->lookahead == '{') {
    if (!valid[INTERPOLATION_START] || !push_scope(scanner, INTERPOLATION)) return false;
    advance(lexer);
    lexer->result_symbol = INTERPOLATION_START;
    return true;
  }
  lexer->result_symbol = STRING_CONTENT;
  return valid[STRING_CONTENT];
}

static bool string_content(TSLexer *lexer) {
  bool consumed = false;
  while (!lexer->eof(lexer) && lexer->lookahead != '"' &&
         lexer->lookahead != '\\' && lexer->lookahead != '$') {
    advance(lexer);
    consumed = true;
  }
  lexer->result_symbol = STRING_CONTENT;
  return consumed;
}

static bool string_token(Scanner *scanner, TSLexer *lexer, const bool *valid) {
  if (lexer->lookahead == '"')
    return scope(scanner) == RAW_STRING && valid[STRING_END] && string_end(scanner, lexer);
  if (lexer->lookahead == '\\') return string_slash(scanner, lexer, valid);
  if (lexer->lookahead == '$') return string_dollar(scanner, lexer, valid);
  return valid[STRING_CONTENT] && string_content(lexer);
}

static bool interpolation_slash(Scanner *scanner, TSLexer *lexer, const bool *valid) {
  advance(lexer);
  if (lexer->lookahead == '"' && valid[STRING_START]) {
    if (!push_scope(scanner, QUOTED_STRING)) return false;
    advance(lexer);
    lexer->result_symbol = STRING_START;
    return true;
  }
  if (!valid[INTERPOLATION_SPACE]) return false;
  if (lexer->lookahead != 'n' && lexer->lookahead != 'r' && lexer->lookahead != 't') return false;
  advance(lexer);
  lexer->result_symbol = INTERPOLATION_SPACE;
  return true;
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

// A glued angle run belongs to a call only when its complete type-shaped
// contents close immediately before '('. A failed lookahead leaves '<' to
// the ordinary comparison/constructor grammar. [TYPE-GENERICS-APPLY]
static bool scan_type_application(TSLexer *lexer) {
  lexer->result_symbol = TYPE_APPLICATION_AHEAD;
  lexer->mark_end(lexer);
  unsigned angles = 0;
  bool saw_type = false;
  bool word = false;
  while (!lexer->eof(lexer)) {
    int32_t ch = lexer->lookahead;
    if (ch == '<') {
      angles++;
      word = false;
    } else if (ch == '>') {
      if (--angles == 0) {
        lexer->advance(lexer, false);
        return saw_type && lexer->lookahead == '(';
      }
      word = false;
    } else if (ch == '-') {
      lexer->advance(lexer, false);
      if (lexer->lookahead != '>') return false;
      word = false;
    } else if ((ch >= 'a' && ch <= 'z') || (ch >= 'A' && ch <= 'Z') || ch == '_') {
      saw_type = true;
      word = true;
    } else if (ch >= '0' && ch <= '9') {
      if (!word) return false;
    } else if (ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' ||
               ch == ',' || ch == ':' || ch == '(' || ch == ')' ||
               ch == '[' || ch == ']' || ch == '|') {
      word = false;
    } else {
      return false;
    }
    lexer->advance(lexer, false);
  }
  return false;
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
static bool expression_string(Scanner *scanner, TSLexer *lexer, const bool *valid) {
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
         lexer->lookahead == '\n' || lexer->lookahead == '\r') lexer->advance(lexer, true);
  if (scope(scanner) == INTERPOLATION) {
    if (valid[INTERPOLATION_END] && lexer->lookahead == '}') {
      advance(lexer); scanner->depth--; lexer->result_symbol = INTERPOLATION_END; return true;
    }
    if (lexer->lookahead == '\\') return interpolation_slash(scanner, lexer, valid);
  }
  return valid[STRING_START] && lexer->lookahead == '"' && string_start(scanner, lexer);
}

bool tree_sitter_osprey_external_scanner_scan(void *payload, TSLexer *lexer,
                                              const bool *valid_symbols) {
  Scanner *scanner = payload;
  if (!scanner) return false;
  if (scope(scanner) == RAW_STRING || scope(scanner) == QUOTED_STRING)
    return string_token(scanner, lexer, valid_symbols);
  if (valid_symbols[TYPE_APPLICATION_AHEAD] && lexer->lookahead == '<')
    return scan_type_application(lexer);
  if (valid_symbols[CALL_OPEN_GAP] && scan_call_open_gap(lexer)) return true;
  if (valid_symbols[STATEMENT_BREAK] && scan_statement_break(lexer)) return true;
  return expression_string(scanner, lexer, valid_symbols);
}

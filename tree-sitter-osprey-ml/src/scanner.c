// Native offside tokens share the default flavor's interpolation scanner.
// Implements [FLAVOR-ML-LAYOUT], [STRING-INTERPOLATION].
#define tree_sitter_osprey_external_scanner_create tree_sitter_osprey_ml_shared_create
#define tree_sitter_osprey_external_scanner_destroy tree_sitter_osprey_ml_shared_destroy
#define tree_sitter_osprey_external_scanner_scan tree_sitter_osprey_ml_shared_scan
#define tree_sitter_osprey_external_scanner_serialize tree_sitter_osprey_ml_shared_serialize
#define tree_sitter_osprey_external_scanner_deserialize tree_sitter_osprey_ml_shared_deserialize
#include "../../tree-sitter-osprey/src/scanner.c"
#define shared_scan tree_sitter_osprey_ml_shared_scan
#define shared_serialize tree_sitter_osprey_ml_shared_serialize
#define shared_deserialize tree_sitter_osprey_ml_shared_deserialize

enum { NEWLINE = INTERPOLATION_SPACE + 1, INDENT, DEDENT,
  OPEN_PAREN, CLOSE_PAREN, OPEN_LIST, CLOSE_LIST, OPEN_RECORD, CLOSE_RECORD, ARROW, SOFT_NEWLINE, BLOCK_COMMENT, NEGATIVE_GAP };
#define INDENT_CAPACITY 128
#define LAYOUT_HEADER_SIZE 8
#define INDENT_ENTRY_SIZE 4
typedef struct {
  Scanner strings;
  unsigned short indents[INDENT_CAPACITY];
  unsigned short regions[INDENT_CAPACITY];
  unsigned depth;
  unsigned brackets;
  bool after_arrow;
  unsigned short arrow_column;
  bool pending_line;
  unsigned short pending_column;
} MlScanner;

void *tree_sitter_osprey_ml_external_scanner_create(void) {
  return calloc(1, sizeof(MlScanner));
}

void tree_sitter_osprey_ml_external_scanner_destroy(void *payload) { free(payload); }

static void write_short(char *buffer, unsigned value) {
  buffer[0] = (char)value; buffer[1] = (char)(value >> 8);
}

static unsigned read_short(const char *buffer) {
  return (unsigned char)buffer[0] | ((unsigned char)buffer[1] << 8);
}

unsigned tree_sitter_osprey_ml_external_scanner_serialize(void *payload, char *buffer) {
  MlScanner *scanner = payload;
  const unsigned layout = LAYOUT_HEADER_SIZE + scanner->depth * INDENT_ENTRY_SIZE;
  if (layout + scanner->strings.depth + 2 > TREE_SITTER_SERIALIZATION_BUFFER_SIZE) return 0;
  buffer[0] = (char)scanner->depth;
  write_short(buffer + 1, scanner->brackets);
  buffer[3] = scanner->after_arrow | (scanner->pending_line ? 2 : 0);
  write_short(buffer + 4, scanner->pending_column);
  write_short(buffer + 6, scanner->arrow_column);
  for (unsigned index = 0; index < scanner->depth; index++) {
    const unsigned offset = LAYOUT_HEADER_SIZE + index * INDENT_ENTRY_SIZE;
    write_short(buffer + offset, scanner->indents[index]);
    write_short(buffer + offset + 2, scanner->regions[index]);
  }
  return layout + shared_serialize(&scanner->strings, buffer + layout);
}

static void restore_layout(MlScanner *scanner, const char *buffer, unsigned depth) {
  scanner->depth = depth;
  scanner->brackets = read_short(buffer + 1);
  scanner->after_arrow = buffer[3] & 1;
  scanner->pending_line = buffer[3] & 2;
  scanner->pending_column = read_short(buffer + 4);
  scanner->arrow_column = read_short(buffer + 6);
  for (unsigned index = 0; index < depth; index++) {
    const unsigned offset = LAYOUT_HEADER_SIZE + index * INDENT_ENTRY_SIZE;
    scanner->indents[index] = read_short(buffer + offset);
    scanner->regions[index] = read_short(buffer + offset + 2);
  }
}

void tree_sitter_osprey_ml_external_scanner_deserialize(void *payload, const char *buffer, unsigned length) {
  MlScanner *scanner = payload;
  memset(scanner, 0, sizeof(*scanner));
  if (length < LAYOUT_HEADER_SIZE + 2) return;
  const unsigned depth = (unsigned char)buffer[0];
  const unsigned layout = LAYOUT_HEADER_SIZE + depth * INDENT_ENTRY_SIZE;
  if (depth > INDENT_CAPACITY || layout + 2 > length) return;
  restore_layout(scanner, buffer, depth);
  shared_deserialize(&scanner->strings, buffer + layout, length - layout);
}

static bool punctuation(MlScanner *scanner, TSLexer *lexer, const bool *valid) {
  const char characters[] = {'(', ')', '[', ']', '{', '}'};
  for (unsigned index = 0; index < sizeof(characters); index++) {
    if (lexer->lookahead != characters[index] || !valid[OPEN_PAREN + index]) continue;
    if (index % 2 == 0 && scanner->brackets >= UINT16_MAX) return false;
    advance(lexer);
    if (index == 0 && lexer->lookahead == '*') return false;
    if (index % 2 == 0) scanner->brackets++;
    else if (scanner->brackets) scanner->brackets--;
    scanner->after_arrow = false;
    lexer->result_symbol = OPEN_PAREN + index; return true;
  }
  if (!valid[ARROW] || lexer->lookahead != '=') return false;
  advance(lexer);
  if (lexer->lookahead != '>') return false;
  advance(lexer); scanner->after_arrow = true; scanner->arrow_column = lexer->get_column(lexer);
  lexer->result_symbol = ARROW; return true;
}

static bool comment_after_open(TSLexer *lexer) {
  if (lexer->lookahead != '*') return false;
  advance(lexer);
  unsigned depth = 1;
  while (depth && !lexer->eof(lexer)) {
    const int32_t previous = lexer->lookahead;
    advance(lexer);
    if (previous == '(' && lexer->lookahead == '*') { advance(lexer); depth++; }
    if (previous == '*' && lexer->lookahead == ')') { advance(lexer); depth--; }
  }
  lexer->result_symbol = BLOCK_COMMENT;
  return depth == 0;
}

static bool skip_layout_space(TSLexer *lexer) {
  bool newline = false;
  lexer->mark_end(lexer);
  while (lexer->lookahead == ' ' || lexer->lookahead == '\t' ||
         lexer->lookahead == '\r' || lexer->lookahead == '\n') {
    newline = newline || lexer->lookahead == '\n';
    lexer->advance(lexer, true);
  }
  return newline;
}

static bool emit(TSLexer *lexer, unsigned symbol) {
  lexer->mark_end(lexer); lexer->result_symbol = symbol; return true;
}

static bool dedent(MlScanner *scanner, TSLexer *lexer, const bool *valid, bool newline) {
  if (!valid[DEDENT] || !scanner->depth) return false;
  const unsigned column = lexer->get_column(lexer);
  const unsigned current = scanner->indents[scanner->depth - 1];
  const unsigned region = scanner->regions[scanner->depth - 1];
  const bool closing = lexer->lookahead == ')' || lexer->lookahead == ']' || lexer->lookahead == '}';
  const bool line_start = newline || (scanner->pending_line && scanner->pending_column == column);
  if (!((line_start && column < current) || lexer->eof(lexer) || (closing && region >= scanner->brackets))) return false;
  scanner->pending_line = true; scanner->pending_column = column;
  scanner->depth--; return emit(lexer, DEDENT);
}

static bool indent(MlScanner *scanner, TSLexer *lexer, const bool *valid, bool newline) {
  const unsigned column = lexer->get_column(lexer);
  const unsigned current = scanner->depth ? scanner->indents[scanner->depth - 1] : 0;
  const unsigned budget = LAYOUT_HEADER_SIZE + (scanner->depth + 1) * INDENT_ENTRY_SIZE + scanner->strings.depth + 2;
  if (!newline || column <= current || column > UINT16_MAX || !valid[INDENT] || scanner->depth == INDENT_CAPACITY ||
      budget > TREE_SITTER_SERIALIZATION_BUFFER_SIZE) return false;
  scanner->regions[scanner->depth] = (unsigned short)scanner->brackets;
  scanner->indents[scanner->depth++] = (unsigned short)column;
  scanner->after_arrow = false; return emit(lexer, INDENT);
}

static bool layout_token(MlScanner *scanner, TSLexer *lexer, const bool *valid, bool newline) {
  const unsigned region = scanner->depth ? scanner->regions[scanner->depth - 1] : 0;
  if (newline && scanner->brackets > region && !scanner->after_arrow && valid[SOFT_NEWLINE])
    return emit(lexer, SOFT_NEWLINE);
  if (dedent(scanner, lexer, valid, newline) || indent(scanner, lexer, valid, newline)) return true;
  if (newline && valid[NEWLINE]) {
    scanner->pending_line = true; scanner->pending_column = lexer->get_column(lexer);
    return emit(lexer, NEWLINE);
  }
  return false;
}

static bool open_comment_or_paren(MlScanner *scanner, TSLexer *lexer, const bool *valid) {
  advance(lexer);
  if (lexer->lookahead == '*') return comment_after_open(lexer);
  if (valid[OPEN_PAREN] && scanner->brackets < UINT16_MAX) {
    scanner->brackets++; scanner->after_arrow = false;
    lexer->result_symbol = OPEN_PAREN; return true;
  }
  return false;
}

static bool code_token(MlScanner *scanner, TSLexer *lexer, const bool *valid) {
  if (valid[BLOCK_COMMENT] && lexer->lookahead == '(') return open_comment_or_paren(scanner, lexer, valid);
  if (valid[ARROW] && lexer->lookahead == '=') return punctuation(scanner, lexer, valid);
  if (punctuation(scanner, lexer, valid)) return true;
  if (valid[STATEMENT_BREAK] && lexer->lookahead == '/')
    return scan_statement_break(lexer, scope(&scanner->strings) == INTERPOLATION);
  return expression_string(&scanner->strings, lexer, valid);
}

static bool ml_layout(MlScanner *scanner, TSLexer *lexer, const bool *valid) {
  const unsigned start_column = lexer->get_column(lexer);
  if (scanner->after_arrow && lexer->get_column(lexer) != scanner->arrow_column) scanner->after_arrow = false;
  const bool newline = skip_layout_space(lexer);
  if (valid[STATEMENT_BREAK] && (newline || lexer->eof(lexer) ||
      (scanner->pending_line && lexer->get_column(lexer) == scanner->pending_column) ||
      (scanner->depth && scanner->regions[scanner->depth - 1] >= scanner->brackets &&
       (lexer->lookahead == ')' || lexer->lookahead == ']' || lexer->lookahead == '}')))) {
    lexer->result_symbol = STATEMENT_BREAK; return true;
  }
  if (layout_token(scanner, lexer, valid, newline)) return true;
  if (!newline && valid[NEGATIVE_GAP] && lexer->get_column(lexer) > start_column && lexer->lookahead == '-') {
    lexer->advance(lexer, true);
    if (lexer->lookahead < '0' || lexer->lookahead > '9') return false;
    lexer->result_symbol = NEGATIVE_GAP; return true;
  }
  return !newline && code_token(scanner, lexer, valid);
}

bool tree_sitter_osprey_ml_external_scanner_scan(void *payload, TSLexer *lexer, const bool *valid) {
  MlScanner *scanner = payload;
  if (LAYOUT_HEADER_SIZE + scanner->depth * INDENT_ENTRY_SIZE + scanner->strings.depth + 2 >= TREE_SITTER_SERIALIZATION_BUFFER_SIZE) return false;
  if (scope(&scanner->strings) == RAW_STRING || scope(&scanner->strings) == QUOTED_STRING)
    return string_token(&scanner->strings, lexer, valid);
  return ml_layout(scanner, lexer, valid);
}

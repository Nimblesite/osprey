// Mock <ctype.h> for symbolic execution [SYMEX-MOCK-HEADERS]: ASCII classes
// inline; tolower/toupper are `--libc=klee`.
#ifndef OSP_HUNT_CTYPE_H
#define OSP_HUNT_CTYPE_H
int tolower(int c);
int toupper(int c);
static inline int isdigit(int c) { return c >= '0' && c <= '9'; }
static inline int isupper(int c) { return c >= 'A' && c <= 'Z'; }
static inline int islower(int c) { return c >= 'a' && c <= 'z'; }
static inline int isalpha(int c) { return isupper(c) || islower(c); }
static inline int isalnum(int c) { return isalpha(c) || isdigit(c); }
static inline int isspace(int c) {
  return c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v';
}
static inline int isxdigit(int c) {
  return isdigit(c) || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
}
static inline int isprint(int c) { return c >= 0x20 && c < 0x7f; }
static inline int iscntrl(int c) { return c < 0x20 || c == 0x7f; }
static inline int ispunct(int c) { return isprint(c) && !isalnum(c) && c != ' '; }
#endif

// Mock <inttypes.h> for symbolic execution [SYMEX-MOCK-HEADERS]: LP64 format
// macros. Clang's own inttypes.h insists on a system copy to include next.
#ifndef OSP_HUNT_INTTYPES_H
#define OSP_HUNT_INTTYPES_H
#include <stdint.h>
#define PRId8 "d"
#define PRId16 "d"
#define PRId32 "d"
#define PRId64 "ld"
#define PRIi64 "li"
#define PRIu8 "u"
#define PRIu16 "u"
#define PRIu32 "u"
#define PRIu64 "lu"
#define PRIx8 "x"
#define PRIx16 "x"
#define PRIx32 "x"
#define PRIx64 "lx"
#define PRIX64 "lX"
#define PRIdMAX "ld"
#define PRIuMAX "lu"
#define PRIdPTR "ld"
#define PRIuPTR "lu"
#define PRIxPTR "lx"
#define SCNd64 "ld"
#define SCNu64 "lu"
#endif

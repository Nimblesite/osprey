/* [MEM-OWNERSHIP-STATIC-VERIFICATION-COMPILER] Specialize the shipped drop
 * walk. */
static __attribute__((noinline)) void arc_drop_child(void *);
#include "memory_ownership.c"
typedef struct {
  int64_t tag;
  void *left, *right;
} Pair;
_Static_assert(sizeof(void *) == 8 && sizeof(Pair) == 24,
               "native record proof ABI");
_Static_assert(offsetof(OspArcHdr, rc) == 8, "native ownership state offset");
#define PAIR_LAYOUT                                                            \
  OSP_MEM_LAYOUT(OSP_MEM_WORD(offsetof(Pair, left)) |                          \
                 OSP_MEM_WORD(offsetof(Pair, right)))
const uint64_t proof_pair_layout = PAIR_LAYOUT;
void proof_pair_drop(void *body) {
  arc_drop_masked(body, sizeof(Pair), OSP_ARC_MASK_BITS(PAIR_LAYOUT),
                  OSP_ARC_KIND(PAIR_LAYOUT) == OSP_MEM_MASK_DIRECT);
}

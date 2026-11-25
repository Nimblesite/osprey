// Size-class pages of the tracing collector [MEM-BACKENDS-GC-HEAP]
// (docs/specs/compiler/0018-MemoryManagement.md): a small block is a cell of a
// page that holds one size, a larger one stays a block of its own in the heap
// table. Part of the memory_gc suite (memory_gc_tests.c runs these).

#include "gc_tests_shared.h"
#include "test_alloc.h"

#define PAGE_HIDE ((uintptr_t)0x3C3C3C3C3C3C3C3Cull)
#define SMALL_MAX 1024 // the largest size a page serves
#define GRAIN 16
#define PER_SIZE 3
// Every class boundary: one byte under, on and over each multiple of the grain,
// from the empty block to the first size past the largest class.
#define SIZES (3 * (SMALL_MAX / GRAIN) + 1)
#define BLOCKS (SIZES * PER_SIZE)

static char *g_kept[BLOCKS];       // real addresses: roots while set
static uintptr_t g_masked[BLOCKS]; // the same addresses, hidden from every scan

static size_t boundary_size(int index) {
  if (index == 0) {
    return 1;
  }
  size_t multiple = (size_t)(index - 1) / 3 + 1;
  return multiple * GRAIN - 1 + (size_t)(index - 1) % 3;
}

static char fill_byte(int block) { return (char)('a' + block % 26); }

// A string block of `size` bytes: its recorded length is how a later check
// tells a live block from a reclaimed one.
static char *text_block(size_t size, char fill) {
  char *text = osp_alloc_tagged((int64_t)size, OSP_MEM_STRING);
  CHECK(text != NULL && ((uintptr_t)text & (GRAIN - 1)) == 0);
  memset(text, fill, size - 1);
  text[size - 1] = '\0';
  return text;
}

__attribute__((noinline)) static void fill_boundaries(void) {
  for (int block = 0; block < BLOCKS; block++) {
    g_kept[block] = text_block(boundary_size(block / PER_SIZE), fill_byte(block));
    g_masked[block] = (uintptr_t)g_kept[block] ^ PAGE_HIDE;
  }
}

static void check_boundaries_alive(void) {
  for (int block = 0; block < BLOCKS; block++) {
    size_t size = boundary_size(block / PER_SIZE);
    const char *text = g_kept[block];
    CHECK(osp_mem_string_length(text) == (int64_t)size - 1);
    CHECK(text[size - 1] == '\0' && (size == 1 || text[0] == fill_byte(block)));
    CHECK(size == 1 || text[size - 2] == fill_byte(block));
    for (int other = block + 1; other < block + PER_SIZE && other < BLOCKS; other++) {
      CHECK(g_kept[other] != text); // no two live blocks share a cell
    }
  }
}

// Rooted blocks of every size survive collections byte for byte, whichever
// side of the page limit they fall on; unrooted, every one is reclaimed.
static void t_every_size_lives_and_dies(void) {
  CHECK(boundary_size(0) == 1 && boundary_size(1) == 15 && boundary_size(2) == 16);
  CHECK(boundary_size(3) == 17 && boundary_size(4) == 31 && boundary_size(SIZES - 1) == SMALL_MAX + 1);
  fill_boundaries();
  (void)alloc_churn(100000, 40);
  osp_collect();
  check_boundaries_alive();
  osp_collect();
  check_boundaries_alive();
  memset(g_kept, 0, sizeof g_kept);
  scrub_stack();
  (void)alloc_churn(100000, 40);
  scrub_stack();
  osp_collect();
  osp_collect();
  size_t reclaimed = 0;
  for (int block = 0; block < BLOCKS; block++) {
    reclaimed += osp_mem_string_length((const char *)(g_masked[block] ^ PAGE_HIDE)) == -1;
  }
  // A stale word in a register may keep a block or two; never a whole class.
  CHECK(reclaimed >= BLOCKS - 4);
}

// A word is a block's address only when it is the first byte of a block in
// use: not a byte inside it, not the cell after it, not the page's own header.
static void t_only_a_base_names_a_block(void) {
  char *text = text_block(40, 'q');
  uintptr_t base = (uintptr_t)text;
  CHECK(osp_mem_string_length(text) == 39);
  CHECK(osp_mem_string_length((const char *)(base + 1)) == -1);
  CHECK(osp_mem_string_length((const char *)(base + 8)) == -1);
  CHECK(osp_mem_string_length((const char *)(base + GRAIN)) == -1);
  CHECK(osp_mem_string_length((const char *)(base & ~(uintptr_t)0xFFFF)) == -1);
  CHECK(osp_mem_string_length((const char *)((base & ~(uintptr_t)0xFFFF) + 8)) == -1);
  osp_mem_immortal((void *)(base + 8)); // a pin on no block pins nothing
  CHECK(osp_mem_string_length(text) == 39 && text[0] == 'q');
}

#define CROWD 60000
#define CROWD_BYTES 64
#define STALE 8 // blocks a stale stack word may have kept at the first count
static void *g_crowd[CROWD];

__attribute__((noinline)) static void fill_crowd(int stride) {
  for (int i = 0; i < CROWD; i++) {
    if (g_crowd[i] == NULL && i % stride == 0) {
      g_crowd[i] = osp_alloc(CROWD_BYTES);
      CHECK(g_crowd[i] != NULL);
      memset(g_crowd[i], i & 0x7F, CROWD_BYTES);
    }
  }
}

static void check_crowd(int stride) {
  for (int i = 0; i < CROWD; i += stride) {
    const unsigned char *body = g_crowd[i];
    CHECK(body[0] == (i & 0x7F) && body[CROWD_BYTES - 1] == (i & 0x7F));
  }
}

// Small blocks live in pages and never enter the heap table. A page with a
// live cell stays and its dead cells serve the next blocks of that size; a
// page with none is handed back.
static void t_pages_fill_reuse_and_return(void) {
  osp_collect();
  size_t table = osp_gc_table_capacity();
  size_t idle = osp_gc_pages();
  size_t others = osp_gc_live_objects(); // what earlier cases still root
  fill_crowd(1);
  size_t full = osp_gc_pages();
  CHECK(full > idle + (size_t)CROWD * CROWD_BYTES / (1u << 16) / 2);
  CHECK(osp_gc_table_capacity() <= table); // no small block took a table slot
  CHECK(osp_gc_live_objects() + STALE >= others + CROWD);
  for (int i = 0; i < CROWD; i++) {
    g_crowd[i] = i % 8 == 0 ? g_crowd[i] : NULL;
  }
  scrub_stack();
  osp_collect();
  check_crowd(8);
  CHECK(osp_gc_live_objects() + STALE >= others + CROWD / 8 && osp_gc_live_objects() < others + CROWD / 4);
  CHECK(osp_gc_pages() > full / 2); // one live cell in eight keeps almost every page
  fill_crowd(1);
  check_crowd(1);
  CHECK(osp_gc_pages() <= full + 2); // the refill went into the dead cells
  memset(g_crowd, 0, sizeof g_crowd);
  scrub_stack();
  osp_collect();
  osp_collect();
  CHECK(osp_gc_pages() < idle + full / 8 && osp_gc_live_objects() < others + 1000);
}

// A block past the largest class is tracked by the table and takes no page;
// growing a block across the limit, and shrinking it back, keeps its bytes.
static void t_large_blocks_stay_in_the_table(void) {
  osp_collect();
  size_t pages = osp_gc_pages();
  static void *large[64];
  for (int i = 0; i < 64; i++) {
    large[i] = osp_alloc(SMALL_MAX + 1 + (int64_t)i * 512);
    CHECK(large[i] != NULL);
    memset(large[i], i, SMALL_MAX + 1);
  }
  CHECK(osp_gc_pages() == pages);
  osp_collect();
  for (int i = 0; i < 64; i++) {
    CHECK(((unsigned char *)large[i])[0] == i && ((unsigned char *)large[i])[SMALL_MAX] == i);
  }
  memset(large, 0, sizeof large);
  unsigned char *grown = osp_gc_malloc(1000);
  memset(grown, 0x6B, 1000);
  grown = osp_gc_realloc(grown, 3000);
  CHECK(grown != NULL && grown[0] == 0x6B && grown[999] == 0x6B);
  grown = osp_gc_realloc(grown, 100);
  osp_collect();
  CHECK(grown != NULL && grown[0] == 0x6B && grown[99] == 0x6B);
}

// OSPREY_GC_PAGES=0 turns the pages off: every block is a table block, and
// collection reclaims them as before. The knob is read once, at the first
// allocation, so it is checked in a child of this suite.
void pages_off_child(void) {
  void *keep = osp_alloc(48);
  memset(keep, 0x4D, 48);
  size_t table = osp_gc_table_capacity();
  (void)alloc_churn(20000, 32);
  CHECK(osp_gc_pages() == 0 && osp_gc_table_capacity() > table);
  osp_collect();
  CHECK(osp_gc_pages() == 0 && osp_gc_live_objects() < 1000);
  CHECK(((unsigned char *)keep)[0] == 0x4D && ((unsigned char *)keep)[47] == 0x4D);
  printf("pages off\n");
}

// [GC-TRACE-CONSERVATIVE] The mark stack is refused the memory it asks for
// while a collection is tracing. The block whose entry found no room is marked
// all the same, so everything it points at must still be traced and kept.
#define FAMILIES 64
#define MARK_STACK_BYTES (((size_t)1u << 14) * sizeof(uintptr_t)) // the stack's first request
static void **g_family[FAMILIES];

__attribute__((noinline)) static void build_families(void) {
  for (int i = 0; i < FAMILIES; i++) {
    void **parent = osp_alloc(2 * sizeof(void *));
    void **child = osp_alloc(2 * sizeof(void *));
    CHECK(parent != NULL && child != NULL);
    child[0] = text_block(40, fill_byte(i));
    parent[0] = child;
    g_family[i] = parent;
  }
}

void mark_stack_refused_child(void) {
  build_families();
  scrub_stack();
  osp_alloc_fail_size(MARK_STACK_BYTES);
  osp_collect();
  CHECK(atomic_load(&osp_alloc_successes_left) == -1); // the refusal was drawn
  osp_alloc_fail_off();
  (void)alloc_churn(20000, 40); // takes over any cell that was wrongly freed
  for (int i = 0; i < FAMILIES; i++) {
    void *const *child = g_family[i][0];
    const char *text = child[0];
    CHECK(osp_mem_string_length(text) == 39 && text[0] == fill_byte(i) && text[38] == fill_byte(i));
  }
  printf("mark stack refused\n");
}

static void t_child_prints(const char *self, const char *setup, const char *flag, const char *want) {
  char command[4096], line[64] = "";
  int length = snprintf(command, sizeof command, "%s '%s' %s", setup, self, flag);
  CHECK(length > 0 && (size_t)length < sizeof command);
  FILE *child = popen(command, "r");
  CHECK(child != NULL);
  CHECK(fgets(line, sizeof line, child) != NULL && strcmp(line, want) == 0);
  CHECK(pclose(child) == 0);
}

void t_size_class_pages(const char *self) {
  t_every_size_lives_and_dies();
  t_only_a_base_names_a_block();
  t_pages_fill_reuse_and_return();
  t_large_blocks_stay_in_the_table();
  t_child_prints(self, "OSPREY_GC_PAGES=0", "--pages-off", "pages off\n");
  t_child_prints(self, "", "--mark-stack-refused", "mark stack refused\n");
}

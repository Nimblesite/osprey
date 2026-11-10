// The ARC registry under churn, the libc-shaped shim, diagnostics counters,
// threads and owned text [GC-ARC-PERCEUS] [MEM-BACKENDS-CUSTOM]. Part of the
// memory_arc suite; see arc_tests_shared.h.
#include "arc_tests_shared.h"

// --- registry growth, tombstone churn, slot reuse ----------------------------
static void churn_slot(void **live, size_t i, uint64_t *seed) {
  if (live[i]) { CHECK(rc_of(live[i]) == 1); osp_release(live[i]); }
  size_t bytes = (size_t)(rnd(seed) % 64) + W;
  live[i] = mk(bytes, OSP_MEM_RAW);
  memset(live[i], (int)(i & 0xFF), bytes);
}

static void churn_probe(void **live, size_t i) {
  if (!live[i]) { return; }
  osp_retain(live[i]); // probes travel through tombstoned chains
  CHECK(rc_of(live[i]) == 2); osp_release(live[i]);
  CHECK(rc_of(live[i]) == 1);
}

void t_registry_churn(void) {
  void **live = (void **)calloc(CHURN_LIVE, sizeof(void *));
  CHECK(live != NULL); uint64_t seed = 0x0DDBA11ull;
  for (int n = 0; n < CHURN_ITERS; n++) {
    churn_slot(live, (size_t)(rnd(&seed) % CHURN_LIVE), &seed);
    churn_probe(live, (size_t)(rnd(&seed) % CHURN_LIVE));
  }
  for (size_t i = 0; i < CHURN_LIVE; i++) {
    CHECK(live[i] != NULL && rc_of(live[i]) == 1);
    osp_release(live[i]);
  }
  free(live);
  CHECK((size_t)CHURN_LIVE * 4 > ARC_INIT_CAP);
}

void t_slot_reuse(void) {
  void *a = mk(24, OSP_MEM_LAYOUT(OSP_MEM_WORD(0)));
  memset(a, 0, 24); osp_release(a);
  for (int i = 0; i < 32; i++) { // re-registering freed (tombstoned) slots
    void *b = mk(24, OSP_MEM_PTR_ARRAY);
    memset(b, 0, 24);
    CHECK(meta_of(b) == OSP_MEM_PTR_ARRAY); // never a stale header
    osp_retain(b); CHECK(rc_of(b) == 2);
    osp_release(b); osp_release(b);
  }
  CHECK(1);
}

// --- osp_arc_shim.h entry points ---------------------------------------------
void t_shim_malloc_calloc_free(void) {
  void *m = osp_arc_malloc(0);
  CHECK(m != NULL && size_of(m) == 0u && rc_of(m) == 1);
  osp_arc_free(m);
  unsigned char *c = (unsigned char *)osp_arc_calloc(4, 8);
  CHECK(c != NULL && size_of(c) == 32u && meta_of(c) == OSP_MEM_RAW);
  for (size_t i = 0; i < 32; i++) { CHECK(c[i] == 0); } // calloc zeroes
  osp_arc_free(c);
  CHECK(osp_arc_calloc((size_t)-1, 2) == NULL); // overflow guard
  CHECK(osp_arc_calloc(2, (size_t)-1) == NULL);
  void *z = osp_arc_calloc(0, 8); // n == 0 short-circuits the guard
  CHECK(z != NULL && size_of(z) == 0u); osp_arc_free(z);
  void *w = witness(), *p = owner(w, 1);
  osp_retain(p); osp_retain(p); CHECK(rc_of(p) == 3);
  osp_arc_free(p); // authoritative: frees at any rc, and walks the layout
  survived(w);
}

void t_shim_realloc_grow(void) {
  unsigned char *p = (unsigned char *)osp_arc_malloc(32);
  CHECK(p != NULL);
  for (size_t i = 0; i < 32; i++) { p[i] = (unsigned char)(i + 1); }
  unsigned char *q = (unsigned char *)osp_arc_realloc(p, 128);
  CHECK(q != NULL && size_of(q) == 128u && rc_of(q) == 1);
  for (size_t i = 0; i < 32; i++) {
    CHECK(q[i] == (unsigned char)(i + 1)); // growth preserves the old bytes
  }
  unsigned char *r = (unsigned char *)osp_arc_realloc(q, 8);
  CHECK(r != NULL && size_of(r) == 8u);
  for (size_t i = 0; i < 8; i++) {
    CHECK(r[i] == (unsigned char)(i + 1)); // shrink preserves the prefix
  }
  osp_arc_free(r);
}

void t_shim_realloc_edges(void) {
  void *fresh = osp_arc_realloc(NULL, 48); // realloc(NULL) == malloc
  CHECK(fresh != NULL && size_of(fresh) == 48u && rc_of(fresh) == 1);
  void *zero = osp_arc_realloc(fresh, 0);
  CHECK(zero != NULL && size_of(zero) == 0u); osp_arc_free(zero);
  void *up = osp_arc_realloc(osp_arc_malloc(0), 16); // oldsize 0: copy skipped
  CHECK(up != NULL && size_of(up) == 16u); osp_arc_free(up);
  // Realloc of a TAGGED object: the copy aliases the children, so ownership
  // MOVES — the new block inherits the layout word and the old one is stripped
  // to RAW before it dies, or the drop walk would decrement `w` twice.
  void *w = witness();
  void **tagged = (void **)mk(16, OSP_MEM_PTR_ARRAY);
  tagged[0] = w; tagged[1] = NULL; osp_retain(w);
  void **grown = (void **)osp_arc_realloc(tagged, 32);
  CHECK(grown && meta_of(grown) == OSP_MEM_PTR_ARRAY && grown[0] == w);
  CHECK(rc_of(w) == 2); // moved, not double-dropped
  osp_arc_free(grown); survived(w); // grown owned it until IT died
  unsigned char *fgn = (unsigned char *)malloc(16);
  CHECK(fgn != NULL); memset(fgn, 0x33, 16);
  unsigned char *g = (unsigned char *)osp_arc_realloc(fgn, 64); // libc owns it
  CHECK(g != NULL);
  for (size_t i = 0; i < 16; i++) { CHECK(g[i] == 0x33); }
  free(g);
}

void t_shim_strdup(void) {
  char *e = osp_arc_strdup("");
  CHECK(e != NULL && e[0] == '\0' && size_of(e) == 1u && rc_of(e) == 1);
  osp_arc_free(e); char *s = osp_arc_strdup(kRodata);
  CHECK(s != NULL && strcmp(s, kRodata) == 0);
  CHECK(size_of(s) == (uint32_t)(strlen(kRodata) + 1));
  osp_arc_free(s);
  char *big = (char *)malloc(LONG_LEN + 1);
  CHECK(big != NULL);
  memset(big, 'x', LONG_LEN); big[LONG_LEN] = '\0';
  char *d = osp_arc_strdup(big);
  CHECK(d != NULL && strlen(d) == LONG_LEN && memcmp(d, big, LONG_LEN) == 0);
  CHECK(size_of(d) == (uint32_t)(LONG_LEN + 1));
  osp_arc_free(d); free(big);
}

// The adaptive lock (memory_hooks.h): after notify (idempotent) every op takes
// the mutex, proved under real contention by two racing churn threads.
static void *mt_churn(void *arg) {
  for (int i = 0; i < FANOUT; i++) {
    void *w = witness(), *p = owner(w, 1);
    osp_retain(p); osp_release(p); osp_arc_free(p); survived(w);
  }
  return arg;
}

// --- diagnostics ---------------------------------------------------------------
// The read-only snapshot surface the memory GOLDEN tests pin exact numbers
// against: the live set must return to a known baseline, and the high-water
// mark must stay visible after it does — that is the property an RSS
// measurement cannot give. Armed by OSPREY_ARC_DEBUG, which main() sets before
// the first allocation. [GC-ARC-PERCEUS]
void t_diagnostic_counters(void) {
  osp_mem_boot(); // arming latches once: booting again changes nothing below
  const size_t objects_before = osp_arc_live_objects();
  const size_t bytes_before = osp_arc_live_bytes();
  osp_arc_peak_reset();
  CHECK(osp_arc_peak_bytes() == bytes_before); // a reset starts at the live level

  void *first = mk(DIAG_BYTES, OSP_MEM_RAW);
  CHECK(osp_arc_live_objects() == objects_before + 1);
  CHECK(osp_arc_live_bytes() == bytes_before + DIAG_BYTES); // the BODY, not the header
  CHECK(osp_arc_peak_bytes() == bytes_before + DIAG_BYTES);
  void *second = mk(DIAG_BYTES, OSP_MEM_RAW);
  const size_t peak_at_two = bytes_before + 2 * DIAG_BYTES;
  CHECK(osp_arc_live_objects() == objects_before + 2);
  CHECK(osp_arc_live_bytes() == peak_at_two);
  CHECK(osp_arc_peak_bytes() == peak_at_two);

  osp_release(first);
  CHECK(osp_arc_live_objects() == objects_before + 1);
  CHECK(osp_arc_live_bytes() == bytes_before + DIAG_BYTES);
  osp_release(second);
  CHECK(osp_arc_live_objects() == objects_before); // exactly back to baseline
  CHECK(osp_arc_live_bytes() == bytes_before);
  // The spike survives the live set returning to baseline. Losing it here is
  // how a transient allocation blow-up becomes invisible to a golden test.
  CHECK(osp_arc_peak_bytes() == peak_at_two);
  osp_arc_peak_reset();
  CHECK(osp_arc_peak_bytes() == bytes_before);

  // A retained object is still live: only the LAST release retires it, and the
  // counters follow the reference count rather than the release call count.
  void *shared = mk(DIAG_BYTES, OSP_MEM_RAW);
  osp_retain(shared);
  CHECK(rc_of(shared) == 2);
  osp_release(shared);
  CHECK(rc_of(shared) == 1);
  CHECK(osp_arc_live_objects() == objects_before + 1);
  CHECK(osp_arc_live_bytes() == bytes_before + DIAG_BYTES);
  osp_release(shared);
  CHECK(osp_arc_live_objects() == objects_before);
  CHECK(osp_arc_live_bytes() == bytes_before);
  // A zero-byte object is still an object: it counts once and adds no bytes.
  osp_arc_peak_reset();
  void *empty = osp_alloc_tagged(0, OSP_MEM_RAW);
  CHECK(empty != NULL);
  CHECK(osp_arc_live_objects() == objects_before + 1);
  CHECK(osp_arc_live_bytes() == bytes_before);
  osp_release(empty);
  CHECK(osp_arc_live_objects() == objects_before);
  CHECK(osp_arc_peak_bytes() == bytes_before); // and it moved no high-water mark
}

void t_multithreaded(void) {
  osp_mem_notify_multithreaded(); osp_mem_notify_multithreaded();
  pthread_t a, b;
  CHECK(!pthread_create(&a, NULL, mt_churn, NULL) && !pthread_create(&b, NULL, mt_churn, NULL));
  CHECK(pthread_join(a, NULL) == 0 && pthread_join(b, NULL) == 0);
  t_retain_release(); // rc semantics are identical under the locked mode
}

// Deliberate leaks, one per kind, so the OSPREY_ARC_DEBUG=2 atexit report walks
// every bucket: RAW mixes printable/control bytes (both preview sanitiser
// arms), 8-byte bodies fit the preview window, the 1 KiB one overruns it.
void t_leak_report_fixtures(void) {
  void *raw = mk(1024, OSP_MEM_RAW), *msk = mk(W, OSP_MEM_LAYOUT(0));
  void *lp = mk(16, OSP_MEM_LIST_HDR_PTR), *ls = mk(16, OSP_MEM_LIST_HDR_SCALAR);
  void *pa = mk(W, OSP_MEM_PTR_ARRAY);
  memset(raw, 0, 1024); memset(msk, 0, W); memset(pa, 0, W);
  memset(lp, 0, 16); memset(ls, 0, 16);
  for (size_t i = 0; i < PREVIEW_LEN; i++) {
    ((unsigned char *)raw)[i] = (i % 2) ? (unsigned char)'A' : (unsigned char)1;
  }
  void *immortal = mk(W, OSP_MEM_RAW); // excluded from the leak count
  osp_mem_immortal(immortal);
  CHECK(rc_of(immortal) == -1 && rc_of(raw) == 1 && size_of(raw) == 1024u);
}

// An immortal reached AS A CHILD of a dying parent: rc < 0 means unreclaimable
// on every path, not just the top-level release, so the walk must skip it.
void t_immortal_child(void) {
  void *forever = witness(); osp_mem_immortal(forever);
  void *p = mk(W, OSP_MEM_LAYOUT(OSP_MEM_WORD(0))); put(p, 0, forever);
  osp_release(p); // parent dies, walks word 0 (p is freed: do not read it)
  CHECK(rc_of(forever) == -1); // untouched, live for its other holders
}

// [PORT-STRING-NUL-OWNERSHIP] Inspect a live String while its owner is finalized under the lock.
static void note_text_finalized(void *body) {
  CHECK(osp_mem_string_length(load(body, 1)) == 9);
  g_finalized = body;
}

void t_owned_text_lifetime(void) {
  void *text = mk(10, OSP_MEM_STRING), *owner = mk(2 * W, OSP_MEM_LIST_HDR_EXTERN);
  memcpy(text, "head\0tail", 10);
  CHECK(osp_mem_string_length(text) == 9 && osp_mem_string_length(NULL) == -1);
  CHECK(osp_mem_string_length("borrowed") == -1);
  osp_retain(text);
  put_i64(owner, 0, 0); put(owner, 1, text);
  osp_mem_set_finalizer(note_text_finalized);
  osp_release(owner);
  CHECK(g_finalized == owner && osp_mem_string_length(text) == 9);
  osp_mem_set_finalizer(NULL);
  osp_release(text);
  CHECK(osp_mem_string_length(text) == -1); // Recycled storage is not a live String.
  CHECK(osp_alloc_tagged_noinit(0, OSP_MEM_STRING) == NULL);
  CHECK(osp_alloc_tagged(-1, OSP_MEM_STRING) == NULL);
}

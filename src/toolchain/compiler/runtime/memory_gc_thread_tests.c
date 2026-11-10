// The collector among threads [MEM-GC-SAFEPOINT] [MEM-EFFECT-BATON]: scanned
// blocks as roots, threads collecting together, a parked thread's stack kept
// alive, a busy thread never blocking an allocator, unregistered exits, the
// multithreaded latch and its debug report. Part of the memory_gc suite; see
// memory_gc_tests_shared.h.
#include "memory_gc_tests_shared.h"
#include "memory_parked_wait.h"

static void *mt_alloc(void *arg) {
  for (int i = 0; i < 2000; i++) {
    void *p = osp_alloc(32);
    if (p) {
      memset(p, 0x11, 32);
    }
  }
  return arg;
}

// --- [MEM-GC-SAFEPOINT] collecting while other threads run -------------------
#define THREAD_CHURN_ROUNDS 200000
#define THREAD_CHURN_SIZE ((size_t)64)
#define THREAD_COUNT 4
#define THREAD_LIVE_CEILING ((size_t)(THREAD_CHURN_ROUNDS / 2))
#define MARK_BYTE 0xAB
#define MARK_SIZE ((size_t)64)

static void *marked_object(void) {
  void *object = osp_alloc((int64_t)MARK_SIZE);
  CHECK(object != NULL);
  memset(object, MARK_BYTE, MARK_SIZE);
  return object;
}

static void check_marked(const void *object) {
  const unsigned char *bytes = (const unsigned char *)object;
  for (size_t i = 0; i < MARK_SIZE; i++) {
    CHECK(bytes[i] == MARK_BYTE);
  }
}

// A scanned block is a root, keeps its bytes when it grows, and stops being a
// root when it is freed. Blocks are unlinked from the head, middle and tail.
void t_scanned_blocks_are_roots(void) {
  void *first = osp_mem_scanned_alloc(2 * WORD);
  void *middle = osp_mem_scanned_alloc(2 * WORD);
  void *last = osp_mem_scanned_alloc(2 * WORD);
  CHECK(first && middle && last);
  put(middle, 1, marked_object());
  (void)alloc_churn(1000, 32);
  osp_collect();
  check_marked(load(middle, 1));
  void *grown = osp_mem_scanned_realloc(middle, 8 * WORD);
  CHECK(grown != NULL && load(grown, 0) == NULL);
  check_marked(load(grown, 1));
  CHECK(load(grown, 7) == NULL);
  void *fresh = osp_mem_scanned_realloc(NULL, WORD);
  CHECK(fresh != NULL && load(fresh, 0) == NULL);
  osp_mem_scanned_free(NULL);
  osp_mem_scanned_free(fresh);
  osp_mem_scanned_free(first);
  osp_mem_scanned_free(last);
  osp_mem_scanned_free(grown);
  osp_collect();
}

static void *churning_thread(void *arg) {
  osp_mem_baton_thread_enter();
  void *volatile kept = marked_object();
  (void)alloc_churn(THREAD_CHURN_ROUNDS, THREAD_CHURN_SIZE);
  check_marked(kept);
  osp_mem_baton_thread_exit();
  return arg;
}

// Its frame sits below the caller's, so the caller's locals stay in what is scanned.
__attribute__((noinline)) static void parked_join(pthread_t *threads, int count) {
  char lowest = 0;
  osp_mem_baton_park(&lowest);
  for (int i = 0; i < count; i++) {
    CHECK(pthread_join(threads[i], NULL) == 0);
  }
  osp_mem_baton_unpark();
}

// Threads that all allocate stop for each other: garbage is reclaimed, and what
// each one and the parked main thread still hold survives.
void t_threads_collect_together(void) {
  osp_mem_notify_multithreaded();
  osp_mem_baton_prepare();
  void *volatile mine = marked_object();
  size_t collections = osp_gc_collections();
  pthread_t threads[THREAD_COUNT];
  for (int i = 0; i < THREAD_COUNT; i++) {
    CHECK(pthread_create(&threads[i], NULL, churning_thread, NULL) == 0);
  }
  parked_join(threads, THREAD_COUNT);
  CHECK(osp_gc_collections() > collections);
  check_marked(mine);
  osp_collect();
  CHECK(osp_gc_live_objects() < THREAD_LIVE_CEILING);
  check_marked(mine);
}

static pthread_mutex_t g_park_lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t g_park_cond = PTHREAD_COND_INITIALIZER;
static int g_park_ready = 0;
static int g_park_released = 0;

static void *parked_thread(void *arg) {
  osp_mem_baton_thread_enter();
  void *volatile kept = marked_object();
  pthread_mutex_lock(&g_park_lock);
  g_park_ready = 1;
  pthread_cond_broadcast(&g_park_cond);
  while (!g_park_released) {
    (void)osp_parked_wait(&g_park_cond, &g_park_lock, NULL);
  }
  pthread_mutex_unlock(&g_park_lock);
  check_marked(kept);
  osp_mem_baton_thread_exit();
  return arg;
}

// A thread blocked in a parked wait holds nothing up, and its stack stays a root.
void t_parked_thread_stays_a_root(void) {
  pthread_t thread;
  CHECK(pthread_create(&thread, NULL, parked_thread, NULL) == 0);
  pthread_mutex_lock(&g_park_lock);
  while (!g_park_ready) {
    pthread_cond_wait(&g_park_cond, &g_park_lock);
  }
  pthread_mutex_unlock(&g_park_lock);
  size_t collections = osp_gc_collections();
  osp_collect();
  (void)alloc_churn(THREAD_CHURN_ROUNDS, THREAD_CHURN_SIZE);
  CHECK(osp_gc_collections() > collections);
  pthread_mutex_lock(&g_park_lock);
  g_park_released = 1;
  pthread_cond_broadcast(&g_park_cond);
  pthread_mutex_unlock(&g_park_lock);
  CHECK(pthread_join(thread, NULL) == 0);
}

static _Atomic int g_busy_done = 0;

static void *allocating_past_a_busy_main(void *arg) {
  osp_mem_baton_thread_enter();
  (void)alloc_churn(THREAD_CHURN_ROUNDS, THREAD_CHURN_SIZE);
  osp_mem_baton_thread_exit();
  atomic_store(&g_busy_done, 1);
  return arg;
}

// Main runs without allocating or parking, so it can never reach a safepoint. The
// other thread must finish regardless, without collecting under main, and
// collecting must resume once main allocates.
void t_busy_thread_cannot_block_an_allocator(void) {
  size_t collections = osp_gc_collections();
  pthread_t thread;
  CHECK(pthread_create(&thread, NULL, allocating_past_a_busy_main, NULL) == 0);
  while (!atomic_load(&g_busy_done)) {
    sched_yield();
  }
  CHECK(osp_gc_collections() == collections);
  CHECK(pthread_join(thread, NULL) == 0);
  (void)alloc_churn(THREAD_CHURN_ROUNDS, THREAD_CHURN_SIZE);
  CHECK(osp_gc_collections() > collections);
}

// A thread that never registered has nothing to leave.
void t_unregistered_thread_exit_is_a_no_op(void) {
  size_t collections = osp_gc_collections();
  osp_mem_baton_thread_exit();
  osp_collect();
  CHECK(osp_gc_collections() == collections + 1);
}

// Once a SECOND thread allocates, collection latches off for the whole process
// (the collector can only scan the sole registered stack safely). Must run LAST
// — it permanently disables reclamation.
void t_multithread_disables_collection(void) {
  osp_mem_notify_multithreaded();
  pthread_t th;
  CHECK(pthread_create(&th, NULL, mt_alloc, NULL) == 0);
  CHECK(pthread_join(th, NULL) == 0);
  size_t frozen = osp_gc_collections();
  (void)alloc_churn(400000, 32); // would normally cross the budget many times
  CHECK(osp_gc_collections() == frozen); // but collection never fires again
  osp_collect();                         // nor when asked for explicitly
  CHECK(osp_gc_collections() == frozen);
}

// The OSPREY_GC_DEBUG child: collect at least once, then let a thread outside
// the baton allocate, which turns collection off.
void debug_stats_child(void) {
  (void)alloc_churn(400000, 32);
  osp_mem_notify_multithreaded();
  pthread_t th;
  CHECK(pthread_create(&th, NULL, mt_alloc, NULL) == 0);
  CHECK(pthread_join(th, NULL) == 0);
}

// OSPREY_GC_DEBUG reports every collection and the moment a non-baton thread
// turned collection off, on stderr only. The switch is read once, at the first
// allocation, so the report is checked in a child of this suite.
void t_debug_reports(const char *self) {
  char command[4096];
  int length = snprintf(command, sizeof command, "OSPREY_GC_DEBUG=1 '%s' --debug-stats 2>&1 >/dev/null", self);
  CHECK(length > 0 && (size_t)length < sizeof command);
  FILE *child = popen(command, "r");
  CHECK(child != NULL);
  int collected = 0, disabled = 0;
  char line[256];
  while (fgets(line, sizeof line, child)) {
    collected |= strncmp(line, "[osp-gc] collect #1: ", 21) == 0;
    disabled |= strstr(line, "[osp-gc] collection disabled: a non-baton thread allocated after ") == line;
  }
  CHECK(pclose(child) == 0);
  CHECK(collected && disabled);
}

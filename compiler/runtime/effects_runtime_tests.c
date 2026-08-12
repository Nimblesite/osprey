// Assertion-driven tests for effects_runtime.c — the dynamic handler stack and
// the thread-based effect continuations behind `handle ... in` and `resume`
// ([EFFECTS-FIBER-PERFORM], [EFFECTS-RESUME], docs/specs/0009). Linked with
// profiler_runtime.c/profiler_sampler.c (the coro thread registers itself) by
// the Makefile's _test_c_runtime. POSIX-only harness (fork/waitpid) for the
// multi-shot rejection, which by contract exits the process.
#include <assert.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <unistd.h>

int __osprey_handler_push(const char *effect_name, const char *operation_name,
                          void *handler_func_ptr, void *env);
int __osprey_handler_pop(void);
void *__osprey_handler_lookup(const char *effect_name,
                              const char *operation_name);
void *__osprey_handler_lookup_env(const char *effect_name,
                                  const char *operation_name);
int __osprey_handler_stack_depth(void);
void __osprey_handler_stack_cleanup(void);
void *__osprey_handler_snapshot(void);
void __osprey_handler_restore(void *snap);
void *__osprey_coro_new(void *env);
void __osprey_coro_start(void *coro, int64_t (*body)(void *), void *body_env,
                         void *snapshot);
int64_t __osprey_coro_suspend(void *coro, int64_t op_id, int64_t *args,
                              int64_t arg_count);
int64_t __osprey_coro_resume(void *coro, int64_t value);
int64_t __osprey_coro_done(void *coro);
int64_t __osprey_coro_op(void *coro);
int64_t __osprey_coro_arg(void *coro, int64_t index);
int64_t __osprey_coro_result(void *coro);
void __osprey_coro_abort(void *coro);
void __osprey_coro_free(void *coro);

static long g_checks = 0;
#define CHECK(c)                                                               \
  do {                                                                         \
    g_checks++;                                                                \
    assert(c);                                                                 \
  } while (0)

#define MAX_DEPTH 1024   // mirrors MAX_HANDLER_STACK_DEPTH
#define NAME_MAX_LEN 128 // mirrors MAX_EFFECT_NAME_LENGTH (incl. NUL)
#define LONG_NAME_LEN 200

static void fn_a(void) {}
static void fn_b(void) {}
static int env_a, env_b;

// Push/lookup/pop with exact depths; the INNERMOST matching handler wins and
// its fnptr and env always come from the SAME entry.
static void t_stack_shadowing(void) {
  CHECK(__osprey_handler_stack_depth() == 0);
  CHECK(__osprey_handler_lookup("State", "get") == NULL);
  CHECK(__osprey_handler_push("State", "get", (void *)fn_a, &env_a) == 0);
  CHECK(__osprey_handler_stack_depth() == 1);
  CHECK(__osprey_handler_lookup("State", "get") == (void *)fn_a);
  CHECK(__osprey_handler_lookup_env("State", "get") == &env_a);
  CHECK(__osprey_handler_lookup("State", "put") == NULL); // op must match too
  CHECK(__osprey_handler_lookup("Log", "get") == NULL);
  CHECK(__osprey_handler_push("State", "get", (void *)fn_b, &env_b) == 0);
  CHECK(__osprey_handler_lookup("State", "get") == (void *)fn_b);
  CHECK(__osprey_handler_lookup_env("State", "get") == &env_b);
  CHECK(__osprey_handler_stack_depth() == 2);
  CHECK(__osprey_handler_pop() == 0);
  CHECK(__osprey_handler_lookup("State", "get") == (void *)fn_a);
  CHECK(__osprey_handler_lookup_env("State", "get") == &env_a);
  CHECK(__osprey_handler_pop() == 0);
  CHECK(__osprey_handler_stack_depth() == 0);
  CHECK(__osprey_handler_pop() == -1); // underflow is rejected, not UB
  CHECK(__osprey_handler_stack_depth() == 0);
}

// Names are stored truncated to the 127-char capacity: the truncated spelling
// resolves, the full over-long spelling does not. Pins the name-length
// contract instead of leaving it as silent behavior.
static void t_name_truncation(void) {
  char full[LONG_NAME_LEN + 1];
  memset(full, 'E', LONG_NAME_LEN);
  full[LONG_NAME_LEN] = '\0';
  char truncated[NAME_MAX_LEN];
  memcpy(truncated, full, NAME_MAX_LEN - 1);
  truncated[NAME_MAX_LEN - 1] = '\0';
  CHECK(__osprey_handler_push(full, "op", (void *)fn_a, NULL) == 0);
  CHECK(__osprey_handler_lookup(truncated, "op") == (void *)fn_a);
  CHECK(__osprey_handler_lookup(full, "op") == NULL);
  CHECK(__osprey_handler_lookup_env(truncated, "op") == NULL); // no captures
  CHECK(__osprey_handler_pop() == 0);
}

// The stack holds exactly MAX_DEPTH entries; the next push reports -1 and
// leaves the depth unchanged, and every entry pops back off cleanly.
static void t_overflow_exact(void) {
  for (int i = 0; i < MAX_DEPTH; i++) {
    CHECK(__osprey_handler_push("Deep", "op", (void *)fn_a, NULL) == 0);
  }
  CHECK(__osprey_handler_stack_depth() == MAX_DEPTH);
  CHECK(__osprey_handler_push("Deep", "op", (void *)fn_a, NULL) == -1);
  CHECK(__osprey_handler_stack_depth() == MAX_DEPTH);
  for (int i = 0; i < MAX_DEPTH; i++) {
    CHECK(__osprey_handler_pop() == 0);
  }
  CHECK(__osprey_handler_stack_depth() == 0);
}

// Snapshot freezes the stack; restore replaces the CURRENT stack with the
// frozen one (entries pushed after the snapshot disappear).
static void t_snapshot_restore(void) {
  CHECK(__osprey_handler_push("A", "op", (void *)fn_a, &env_a) == 0);
  CHECK(__osprey_handler_push("B", "op", (void *)fn_b, &env_b) == 0);
  void *snap = __osprey_handler_snapshot();
  CHECK(snap != NULL);
  CHECK(__osprey_handler_push("C", "op", (void *)fn_a, NULL) == 0);
  CHECK(__osprey_handler_stack_depth() == 3);
  __osprey_handler_restore(snap); // frees snap
  CHECK(__osprey_handler_stack_depth() == 2);
  CHECK(__osprey_handler_lookup("C", "op") == NULL);
  CHECK(__osprey_handler_lookup("B", "op") == (void *)fn_b);
  CHECK(__osprey_handler_lookup_env("A", "op") == &env_a);
  CHECK(__osprey_handler_pop() == 0 && __osprey_handler_pop() == 0);
  __osprey_handler_restore(NULL); // tolerated
}

// After cleanup the thread's stack re-initializes lazily and works again.
static void t_cleanup_reinit(void) {
  __osprey_handler_stack_cleanup();
  CHECK(__osprey_handler_stack_depth() == 0);
  CHECK(__osprey_handler_push("Re", "op", (void *)fn_a, NULL) == 0);
  CHECK(__osprey_handler_lookup("Re", "op") == (void *)fn_a);
  CHECK(__osprey_handler_pop() == 0);
}

// --- continuations -----------------------------------------------------------

typedef struct {
  void *coro;
  int64_t base;
} CoroEnv;

#define OP_FIRST 11
#define OP_SECOND 22
#define RESUME_FIRST 5
#define RESUME_SECOND 7
#define CORO_BASE 100

// Performs twice, then finishes with a value derived from both resumes — so
// the final result proves each resume value reached the body exactly once.
static int64_t body_two_performs(void *raw) {
  CoroEnv *e = raw;
  int64_t args[2] = {10, 20};
  int64_t r1 = __osprey_coro_suspend(e->coro, OP_FIRST, args, 2);
  int64_t r2 = __osprey_coro_suspend(e->coro, OP_SECOND, NULL, 0);
  return e->base + r1 * r2;
}

static void t_coro_ping_pong(void) {
  CoroEnv env = {.coro = __osprey_coro_new(NULL), .base = CORO_BASE};
  CHECK(env.coro != NULL);
  __osprey_coro_start(env.coro, body_two_performs, &env, NULL);
  CHECK(__osprey_coro_done(env.coro) == 0);
  CHECK(__osprey_coro_op(env.coro) == OP_FIRST);
  CHECK(__osprey_coro_arg(env.coro, 0) == 10);
  CHECK(__osprey_coro_arg(env.coro, 1) == 20);
  CHECK(__osprey_coro_arg(env.coro, 2) == 0);  // past arg_count
  CHECK(__osprey_coro_arg(env.coro, -1) == 0); // negative index
  CHECK(__osprey_coro_arg(env.coro, 16) == 0); // past the hard cap
  CHECK(__osprey_coro_resume(env.coro, RESUME_FIRST) == 0); // re-suspended
  CHECK(__osprey_coro_done(env.coro) == 0);
  CHECK(__osprey_coro_op(env.coro) == OP_SECOND);
  CHECK(__osprey_coro_arg(env.coro, 0) == 0); // zero-arg perform
  int64_t want = CORO_BASE + RESUME_FIRST * RESUME_SECOND;
  CHECK(__osprey_coro_resume(env.coro, RESUME_SECOND) == want);
  CHECK(__osprey_coro_done(env.coro) == 1);
  CHECK(__osprey_coro_result(env.coro) == want);
  __osprey_coro_free(env.coro);
}

// The snapshot passed to start is restored ON the continuation's thread: the
// body observes the parent's handlers. Cross-thread handler propagation is
// what makes `perform` inside a handled fiber resolve at all.
static int64_t body_sees_handlers(void *raw) {
  (void)raw;
  int see = __osprey_handler_lookup("Xfer", "op") == (void *)fn_b;
  int depth_one = __osprey_handler_stack_depth() == 1;
  __osprey_handler_stack_cleanup(); // this thread's copy dies with it
  return see && depth_one ? 1 : 0;
}

static void t_coro_snapshot_transfer(void) {
  CHECK(__osprey_handler_push("Xfer", "op", (void *)fn_b, NULL) == 0);
  void *coro = __osprey_coro_new(NULL);
  __osprey_coro_start(coro, body_sees_handlers, NULL, __osprey_handler_snapshot());
  CHECK(__osprey_coro_done(coro) == 1);   // never performed: ran straight through
  CHECK(__osprey_coro_result(coro) == 1); // ...and saw the parent's handler
  __osprey_coro_free(coro);
  CHECK(__osprey_handler_pop() == 0); // parent stack untouched by the child
  CHECK(__osprey_handler_stack_depth() == 0);
}

static int64_t body_one_perform(void *raw) {
  CoroEnv *e = raw;
  (void)__osprey_coro_suspend(e->coro, OP_FIRST, NULL, 0);
  return 99;
}

// Aborting a suspended continuation terminates its thread without running the
// rest of the body; the coro lands done with a zero result.
static void t_coro_abort(void) {
  CoroEnv env = {.coro = __osprey_coro_new(NULL), .base = 0};
  __osprey_coro_start(env.coro, body_one_perform, &env, NULL);
  CHECK(__osprey_coro_done(env.coro) == 0);
  __osprey_coro_abort(env.coro);
  CHECK(__osprey_coro_done(env.coro) == 1);
  CHECK(__osprey_coro_result(env.coro) == 0); // body never completed
  __osprey_coro_free(env.coro);
}

// Freeing a still-suspended continuation aborts it internally — no hang, no
// leak of the parked thread.
static void t_coro_free_while_suspended(void) {
  CoroEnv env = {.coro = __osprey_coro_new(NULL), .base = 0};
  __osprey_coro_start(env.coro, body_one_perform, &env, NULL);
  CHECK(__osprey_coro_done(env.coro) == 0);
  __osprey_coro_free(env.coro);
  CHECK(1); // reaching here means the free did not deadlock
}

// Every continuation entry point tolerates NULL.
static void t_coro_null_safety(void) {
  CHECK(__osprey_coro_suspend(NULL, 1, NULL, 0) == 0);
  CHECK(__osprey_coro_resume(NULL, 1) == 0);
  CHECK(__osprey_coro_done(NULL) == 1);
  CHECK(__osprey_coro_op(NULL) == 0);
  CHECK(__osprey_coro_arg(NULL, 0) == 0);
  CHECK(__osprey_coro_result(NULL) == 0);
  __osprey_coro_abort(NULL);
  __osprey_coro_free(NULL);
}

static int64_t body_immediate(void *raw) {
  (void)raw;
  return 3;
}

// Multi-shot resume is REJECTED LOUDLY [EFFECTS-RESUME]: resuming a FINISHED
// continuation must exit(1), never return the stale first result as if the
// body had run again. Forked so the contractual process exit is observable.
static void t_multishot_resume_exits(void) {
  pid_t pid = fork();
  CHECK(pid >= 0);
  if (pid == 0) {
    void *coro = __osprey_coro_new(NULL);
    __osprey_coro_start(coro, body_immediate, NULL, NULL); // completes at once
    (void)__osprey_coro_resume(coro, 0); // resume-after-done: must exit(1)
    _exit(0);                            // unreachable if the guard holds
  }
  int status = 0;
  CHECK(waitpid(pid, &status, 0) == pid);
  CHECK(WIFEXITED(status) && WEXITSTATUS(status) == 1);
}

int main(void) {
  t_stack_shadowing();
  t_name_truncation();
  t_overflow_exact();
  t_snapshot_restore();
  t_cleanup_reinit();
  t_coro_ping_pong();
  t_coro_snapshot_transfer();
  t_coro_abort();
  t_coro_free_while_suspended();
  t_coro_null_safety();
  t_multishot_resume_exits();
  printf("[ok] effects_runtime: %ld assertions\n", g_checks);
  return 0;
}

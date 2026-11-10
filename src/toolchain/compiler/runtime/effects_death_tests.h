// The effect runtime's fatal guards [FAILURE-EXPLICIT]: a mailbox read with no
// mailbox, out of its arity or before its first slot, a start with no
// continuation or body, a lying arity and a second resume all abort naming
// what went wrong, and the death watchdog kills a body that stalls. Included
// once by effects_runtime_tests.c, like effects_scope_tests.h.
#ifndef OSPREY_EFFECTS_DEATH_TESTS_H
#define OSPREY_EFFECTS_DEATH_TESTS_H

static void death_mail_op_without_a_mailbox(void) {
  (void)__osprey_coro_mail_op(NULL);
}

static void death_mail_arg_without_a_mailbox(void) {
  (void)__osprey_coro_mail_arg(NULL, 0);
}

// A REAL mailbox, read one slot past the arity it carries. This is the arm that
// matters most: a NULL mailbox is obviously wrong, whereas a live one answering
// 0 for a slot nobody filled looks exactly like a legitimate zero argument.
static void death_mail_arg_past_the_arity_sent(void) {
  CoroEnv env = {.coro = __osprey_coro_new(NULL), .base = CORO_BASE};
  __osprey_coro_start(env.coro, body_two_performs, &env, NULL);
  void *mail = __osprey_coro_take_args(env.coro);
  (void)__osprey_coro_mail_arg(mail, WIDE_ARITY); // slots 0..WIDE_ARITY-1 exist
}

// The same slot read on the low side. `index < 0` is a separate branch from
// `index >= count`, and a negative subscript reads BEHIND the argument array.
static void death_mail_arg_before_the_first_slot(void) {
  CoroEnv env = {.coro = __osprey_coro_new(NULL), .base = CORO_BASE};
  __osprey_coro_start(env.coro, body_two_performs, &env, NULL);
  void *mail = __osprey_coro_take_args(env.coro);
  (void)__osprey_coro_mail_arg(mail, -1);
}

static void death_start_without_a_continuation(void) {
  __osprey_coro_start(NULL, body_immediate, NULL, NULL);
}

static void death_start_without_a_body(void) {
  __osprey_coro_start(__osprey_coro_new(NULL), NULL, NULL, NULL);
}

// A positive arity with a NULL argument array can only come from a codegen bug.
// Suspending is what builds the mailbox, so this has to be performed from
// inside a running continuation rather than by calling the constructor.
static int64_t body_lying_arity(void *raw) {
  return __osprey_coro_suspend(raw, OP_FIRST, NULL, NULL, 2);
}

static void death_perform_declaring_arguments_it_did_not_send(void) {
  void *coro = __osprey_coro_new(NULL);
  __osprey_coro_start(coro, body_lying_arity, coro, NULL);
}

// Resuming a continuation that has already run to completion. The
// thread-as-continuation model is single-shot: a consumed pthread stack cannot
// be re-run, so a second resume would silently return the stale first result —
// a wrong answer with exit 0.
//
// [MULTI-HANDLE-ONCE] moved this verdict to the checker, and Osprey installs
// handlers lexically, so NO source program reaches this guard any more. That is
// exactly why it is proven here instead of through a compiled program: a
// backstop against invalid compiler output has no source-level path by
// construction, and an unproven backstop is indistinguishable from a removed
// one. It exits rather than aborts, so the exit status is what this asserts.
static void death_second_resume_of_a_finished_continuation(void) {
  CoroEnv env = {.coro = __osprey_coro_new(NULL), .base = 0};
  __osprey_coro_start(env.coro, body_one_perform, &env, NULL);
  (void)__osprey_coro_resume(env.coro, 0); // the body runs to completion
  (void)__osprey_coro_resume(env.coro, 0); // nothing left to resume
}

static void check_death(OspDeathBody body, const char *what) {
  int signalled = osp_death_signal(body);
  if (signalled != SIGABRT) {
    fprintf(stderr, "expected %s to abort, got signal %d\n", what, signalled);
  }
  CHECK(signalled == SIGABRT);
}

// Not a guard: the harness's own watchdog. `check_death` above waits on the
// child, so a body that neither aborts nor returns would park this suite for
// good -- and a hung suite is read as broken infrastructure, never as the
// failing assertion it is.
static void body_that_never_returns(void) {
  for (;;) {
    (void)pause();
  }
}

// The same stall, with every polite way of being stopped disarmed: SIGALRM
// ignored AND every signal blocked. An in-child `alarm` would leave this
// running forever, which is why the deadline is enforced from the parent with
// SIGKILL -- the one signal a process cannot refuse.
static void body_that_refuses_to_be_alarmed(void) {
  (void)signal(SIGALRM, SIG_IGN);
  sigset_t everything;
  (void)sigfillset(&everything);
  (void)sigprocmask(SIG_BLOCK, &everything, NULL);
  for (;;) {
    (void)pause();
  }
}

// One second rather than OSP_DEATH_BUDGET_SECONDS: what is under test is the
// deadline, and proving it at the real budget would cost the suite 30 seconds.
static void check_stall(OspDeathBody body, const char *what) {
  int signalled = osp_death_signal_within(body, 1);
  if (signalled != OSP_DEATH_STALLED) {
    fprintf(stderr, "expected %s to be killed at its deadline, got %d\n", what,
            signalled);
  }
  CHECK(signalled == OSP_DEATH_STALLED);
  CHECK(signalled != SIGABRT); // and never mistaken for a guard that fired
}

static void t_death_watchdog_kills_a_body_that_stalls(void) {
  check_stall(body_that_never_returns, "a body that never returns");
  check_stall(body_that_refuses_to_be_alarmed, "a body that refuses SIGALRM");
}

// Every state effects_coro.c refuses to guess a value for.
static void t_fatal_guards_abort(void) {
  check_death(death_mail_op_without_a_mailbox, "dispatch with no mailbox");
  check_death(death_mail_arg_without_a_mailbox, "argument read with no mailbox");
  check_death(death_mail_arg_past_the_arity_sent, "argument past the arity sent");
  check_death(death_mail_arg_before_the_first_slot, "argument before the first slot");
  check_death(death_start_without_a_continuation, "start with no continuation");
  check_death(death_start_without_a_body, "start with no body");
  check_death(death_perform_declaring_arguments_it_did_not_send,
              "perform declaring arguments it did not send");
  // The one guard that exits instead of aborting [EFFECTS-RESUME],
  // [MULTI-HANDLE-ONCE].
  int ended = osp_death_exit(death_second_resume_of_a_finished_continuation);
  if (ended != OSP_DEATH_EXITED + 1) {
    fprintf(stderr, "expected a second resume to exit 1, got %d\n", ended);
  }
  CHECK(ended == OSP_DEATH_EXITED + 1);
}

#endif

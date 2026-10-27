// Finalizer, mask and capability-instance tests; they share the effects runtime
// suite's assertion counter. Canonical contract:
// docs/specs/language/0017-AlgebraicEffects.md, [EFFECTS-FINALIZATION],
// [EFFECTS-MASK] and [EFFECTS-INSTANCE].
#ifndef OSPREY_EFFECTS_FINALLY_TESTS_H
#define OSPREY_EFFECTS_FINALLY_TESTS_H

#define FINALIZER_LOG_CAPACITY 32

// Each run records the finalizer's id and the handler depth it ran at, so the
// order and the "outside its own activation" contract are both observable.
static int g_finalized[FINALIZER_LOG_CAPACITY];
static int g_finalized_depth[FINALIZER_LOG_CAPACITY];
static int g_finalized_count;
static int g_finalizer_ids[FINALIZER_LOG_CAPACITY];

static void record_finalizer(void *env) {
  CHECK(g_finalized_count < FINALIZER_LOG_CAPACITY);
  g_finalized_depth[g_finalized_count] = __osprey_handler_depth();
  g_finalized[g_finalized_count++] = *(int *)env;
}

// A finalizer that transfers control the way an aborting perform does: it
// unwinds the finalizers still pending from inside its own run.
static void unwinding_finalizer(void *env) {
  record_finalizer(env);
  __osprey_finalizer_unwind();
}

static void push_finalizer(OspFinalizer run, int id) {
  g_finalizer_ids[id] = id;
  __osprey_finalizer_push(run, &g_finalizer_ids[id]);
}

// Normal completion runs the innermost cleanup once, after unlinking every
// entry its activation installed, and leaves the enclosing activation intact.
static void t_finally_runs_outside_its_activation(void) {
  g_finalized_count = 0;
  CHECK(__osprey_handler_push_scoped(OP_A, (void *)fn_a, NULL, __osprey_handler_depth()) == 0);
  push_finalizer(record_finalizer, 1);
  int base = __osprey_handler_depth();
  CHECK(__osprey_handler_push_scoped(OP_B, (void *)fn_b, NULL, base) == 0);
  CHECK(__osprey_handler_push_scoped(OP_C, (void *)fn_b, NULL, base) == 0);
  __osprey_finalizer_finish();
  CHECK(g_finalized_count == 1 && g_finalized[0] == 1);
  CHECK(g_finalized_depth[0] == 1);
  CHECK(__osprey_handler_lookup(OP_B) == NULL && __osprey_handler_lookup(OP_C) == NULL);
  CHECK(__osprey_handler_lookup(OP_A) == (void *)fn_a);
  __osprey_finalizer_unwind(); // nothing pending: a no-op
  CHECK(g_finalized_count == 1);
  __osprey_handler_stack_cleanup();
}

// Abandonment finalizes every pending activation inner to outer, each exactly
// once, across a growth of the finalizer stack past its first capacity.
static void t_finally_unwinds_inner_to_outer(void) {
  enum { PENDING = 20 };
  g_finalized_count = 0;
  for (int id = 0; id < PENDING; id++) {
    push_finalizer(record_finalizer, id);
    CHECK(__osprey_handler_push_scoped(OP_A, (void *)fn_a, NULL, __osprey_handler_depth()) == 0);
  }
  __osprey_finalizer_unwind();
  CHECK(g_finalized_count == PENDING);
  for (int i = 0; i < PENDING; i++) {
    CHECK(g_finalized[i] == PENDING - 1 - i);
    CHECK(g_finalized_depth[i] == PENDING - 1 - i);
  }
  CHECK(__osprey_handler_depth() == 0);
  __osprey_handler_stack_cleanup();
}

// A finalizer that itself unwinds continues with the ones still pending;
// popping before running is what keeps it from running twice.
static void t_finally_reentrant_unwind_runs_each_once(void) {
  g_finalized_count = 0;
  push_finalizer(record_finalizer, 0);
  push_finalizer(unwinding_finalizer, 1);
  push_finalizer(record_finalizer, 2);
  __osprey_finalizer_unwind();
  CHECK(g_finalized_count == 3);
  CHECK(g_finalized[0] == 2 && g_finalized[1] == 1 && g_finalized[2] == 0);
  __osprey_handler_stack_cleanup();
}

static void death_finish_without_a_finalizer(void) {
  __osprey_handler_stack_cleanup();
  __osprey_finalizer_finish();
}

static void t_owner_frames_preserve_returned_frames_and_clear_abandoned_slots(void) {
  void *outer = &env_a;
  void *inner = &env_b;
  void *empty = NULL;
  void **outer_slots[] = {&outer};
  void **inner_slots[] = {&inner, &empty};
  OspOwnerFrame first, second;
  __osprey_owner_frame_push(&first, outer_slots, 1);
  __osprey_owner_frame_push(&second, inner_slots, 2);
  __osprey_owner_frame_pop(&second);
  __osprey_owner_frames_unwind();
  CHECK(outer == NULL && inner == &env_b);
  __osprey_owner_frame_push(&second, inner_slots, 2);
  __osprey_owner_frames_unwind();
  CHECK(inner == NULL && empty == NULL);
  __osprey_owner_frames_unwind();
}

static void death_owner_frame_underflow(void) {
  __osprey_owner_frame_pop(NULL);
}

// A mask answers with whatever was visible just outside the activation it
// skips; nested masks skip successive activations, a handler installed under
// the mask shadows it as usual, and popping restores the hidden activation.
static void t_mask_skips_one_activation(void) {
  CHECK(__osprey_handler_push_scoped(OP_A, (void *)fn_a, &env_a, __osprey_handler_depth()) == 0);
  CHECK(__osprey_handler_push_scoped(OP_A, (void *)fn_b, &env_b, __osprey_handler_depth()) == 0);
  CHECK(__osprey_handler_mask(OP_A, 0) == 0);
  CHECK(__osprey_handler_lookup(OP_A) == (void *)fn_a);
  CHECK(__osprey_handler_lookup_env(OP_A) == &env_a);
  CHECK(__osprey_handler_mask(OP_A, 0) == 0);
  CHECK(__osprey_handler_lookup(OP_A) == NULL);
  CHECK(__osprey_handler_push_scoped(OP_A, (void *)fn_b, NULL, __osprey_handler_depth()) == 0);
  CHECK(__osprey_handler_lookup(OP_A) == (void *)fn_b);
  CHECK(__osprey_handler_pop() == 0 && __osprey_handler_pop() == 0);
  CHECK(__osprey_handler_lookup(OP_A) == (void *)fn_a);
  CHECK(__osprey_handler_pop() == 0);
  CHECK(__osprey_handler_lookup_env(OP_A) == &env_b);
  CHECK(__osprey_handler_mask(OP_B, 0) == 0); // nothing to skip
  CHECK(__osprey_handler_lookup(OP_B) == NULL);
  CHECK(__osprey_handler_mask(OP_OUT_OF_RANGE, 0) == -1);
  __osprey_handler_stack_cleanup();
}

// Capability identities are fresh and distinct; each instance sees only its
// own handlers, through entries of other instances, and a mask of one leaves
// the other answered.
static void t_instances_are_independent_capabilities(void) {
  int64_t first = __osprey_instance_new();
  int64_t second = __osprey_instance_new();
  CHECK(first != 0 && second != 0 && first != second);
  CHECK(__osprey_handler_push_in(OP_A, first, (void *)fn_a, &env_a, __osprey_handler_depth()) == 0);
  CHECK(__osprey_handler_push_in(OP_A, second, (void *)fn_b, &env_b, __osprey_handler_depth()) == 0);
  CHECK(__osprey_handler_lookup_in(OP_A, first) == (void *)fn_a);
  CHECK(__osprey_handler_lookup_env_in(OP_A, second) == &env_b);
  CHECK(__osprey_handler_lookup(OP_A) == NULL);
  CHECK(__osprey_handler_mask(OP_A, first) == 0);
  CHECK(__osprey_handler_lookup_in(OP_A, first) == NULL);
  CHECK(__osprey_handler_lookup_in(OP_A, second) == (void *)fn_b);
  __osprey_handler_stack_cleanup();
}

static void t_finally_mask_and_instances(void) {
  t_owner_frames_preserve_returned_frames_and_clear_abandoned_slots();
  CHECK(osp_death_signal(death_owner_frame_underflow) == SIGABRT);
  t_finally_runs_outside_its_activation();
  t_finally_unwinds_inner_to_outer();
  t_finally_reentrant_unwind_runs_each_once();
  CHECK(osp_death_signal(death_finish_without_a_finalizer) == SIGABRT);
  t_mask_skips_one_activation();
  t_instances_are_independent_capabilities();
}

#endif

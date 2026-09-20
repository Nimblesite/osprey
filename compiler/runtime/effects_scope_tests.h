// Scope tests share the existing effects runtime suite's assertion counter.
// Canonical contract: docs/specs/0017-AlgebraicEffects.md, handler scope.
#ifndef OSPREY_EFFECTS_SCOPE_TESTS_H
#define OSPREY_EFFECTS_SCOPE_TESTS_H

static void t_scope_forwarding(void) {
  CHECK(__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_a, NULL, 0) == 0);
  int base = __osprey_handler_depth();
  CHECK(__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_b, NULL, base) == 0);
  CHECK(__osprey_handler_push_scoped(OP_STATE_PUT, (void *)fn_b, NULL, base) == 0);
  CHECK(__osprey_handler_push_scoped(OP_LOG_WRITE, (void *)fn_b, &env_b, __osprey_handler_depth()) == 0);
  HandlerScope *inner = __osprey_handler_suspend_scope(OP_STATE_GET);
  CHECK(__osprey_handler_depth() == 1);
  CHECK(__osprey_handler_lookup(OP_STATE_GET) == (void *)fn_a);
  CHECK(__osprey_handler_lookup(OP_STATE_PUT) == NULL);
  CHECK(__osprey_handler_lookup(OP_LOG_WRITE) == NULL);
  CHECK(__osprey_handler_push_scoped(OP_ARM_LOCAL, (void *)fn_a, &env_a, __osprey_handler_depth()) == 0);
  CHECK(__osprey_handler_pop() == 0);
  __osprey_handler_restore_scope(inner);
  CHECK(__osprey_handler_depth() == 4);
  CHECK(__osprey_handler_lookup(OP_STATE_GET) == (void *)fn_b);
  CHECK(__osprey_handler_lookup_env(OP_LOG_WRITE) == &env_b);
  CHECK(__osprey_handler_lookup(OP_ARM_LOCAL) == NULL);
  __osprey_handler_stack_cleanup();
}

static void t_scope_nested_forwarding(void) {
  CHECK(__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_a, NULL, __osprey_handler_depth()) == 0);
  CHECK(__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_b, NULL, __osprey_handler_depth()) == 0);
  HandlerScope *inner = __osprey_handler_suspend_scope(OP_STATE_GET);
  HandlerScope *outer = __osprey_handler_suspend_scope(OP_STATE_GET);
  CHECK(__osprey_handler_depth() == 0);
  CHECK(__osprey_handler_lookup(OP_STATE_GET) == NULL);
  __osprey_handler_restore_scope(outer);
  CHECK(__osprey_handler_lookup(OP_STATE_GET) == (void *)fn_a);
  __osprey_handler_restore_scope(inner);
  CHECK(__osprey_handler_lookup(OP_STATE_GET) == (void *)fn_b);
  __osprey_handler_stack_cleanup();
}

static void t_scope_snapshot_preserves_activation(void) {
  CHECK(__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_a, NULL, __osprey_handler_depth()) == 0);
  int base = __osprey_handler_depth();
  CHECK(__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_b, NULL, base) == 0);
  CHECK(__osprey_handler_push_scoped(OP_STATE_PUT, (void *)fn_b, NULL, base) == 0);
  HandlerSnapshot *snapshot = __osprey_handler_snapshot();
  __osprey_handler_stack_cleanup();
  __osprey_handler_restore(snapshot);
  HandlerScope *scope = __osprey_handler_suspend_scope(OP_STATE_PUT);
  CHECK(__osprey_handler_depth() == 1);
  CHECK(__osprey_handler_lookup(OP_STATE_GET) == (void *)fn_a);
  __osprey_handler_restore_scope(scope);
  CHECK(__osprey_handler_depth() == 3);
  __osprey_handler_stack_cleanup();
}

static void death_scope_missing_handler(void) {
  (void)__osprey_handler_suspend_scope(OP_MISSING);
}

static void death_scope_unbalanced_restore(void) {
  (void)__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_a, NULL, __osprey_handler_depth());
  HandlerScope *scope = __osprey_handler_suspend_scope(OP_STATE_GET);
  (void)__osprey_handler_push_scoped(OP_ARM_LOCAL, (void *)fn_b, NULL, __osprey_handler_depth());
  __osprey_handler_restore_scope(scope);
}

static int64_t body_suspending_scope(void *raw) {
  (void)__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_b, NULL, __osprey_handler_depth());
  HandlerScope *scope = __osprey_handler_suspend_scope(OP_STATE_GET);
  int64_t value = __osprey_coro_suspend(raw, 31, NULL, NULL, 0);
  __osprey_handler_restore_scope(scope);
  return value + (__osprey_handler_lookup(OP_STATE_GET) == (void *)fn_b);
}

static void t_scope_coro_resume_and_abandon(void) {
  void *coro = __osprey_coro_new(NULL);
  __osprey_coro_start(coro, body_suspending_scope, coro, NULL);
  CHECK(__osprey_handler_depth() == 0);
  CHECK(__osprey_coro_resume(coro, 41) == 42);
  __osprey_coro_free(coro);
  coro = __osprey_coro_new(NULL);
  __osprey_coro_start(coro, body_suspending_scope, coro, NULL);
  __osprey_coro_abort(coro);
  CHECK(__osprey_coro_done(coro));
  __osprey_coro_free(coro);
}

static void t_handler_scope(void) {
  t_scope_forwarding();
  t_scope_nested_forwarding();
  t_scope_snapshot_preserves_activation();
  t_scope_coro_resume_and_abandon();
  CHECK(__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_a, NULL, -1) == -1);
  CHECK(__osprey_handler_push_scoped(OP_STATE_GET, (void *)fn_a, NULL, 1) == -1);
  CHECK(__osprey_handler_depth() == 0);
  CHECK(osp_death_signal(death_scope_missing_handler) == SIGABRT);
  CHECK(osp_death_signal(death_scope_unbalanced_restore) == SIGABRT);
}

#endif

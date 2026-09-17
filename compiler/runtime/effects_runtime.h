// Shared surface between the two halves of the algebraic-effect runtime: the
// dynamic handler stack (effects_runtime.c) and the thread-as-continuation
// machinery that implements `resume` (effects_coro.c).
#ifndef OSPREY_EFFECTS_RUNTIME_H
#define OSPREY_EFFECTS_RUNTIME_H

#include <stdint.h>

// A copy of one thread's handler stack, taken on the thread that installs the
// handlers and restored on the thread that continues the computation. Opaque
// here: only effects_runtime.c knows the layout, everything else moves it by
// pointer.
typedef struct HandlerSnapshot HandlerSnapshot;

HandlerSnapshot *__osprey_handler_snapshot(void);
void __osprey_handler_restore(HandlerSnapshot *snap);

// All entries installed by one activation share its starting depth. Suspending
// an arm removes that activation and intervening scopes until its call returns.
// The returned scope owns a saved tail and must be restored in nesting order.
typedef struct HandlerScope HandlerScope;
int __osprey_handler_depth(void);
int __osprey_handler_push_scoped(const char *effect_name, const char *operation_name,
                                void *handler_func_ptr, void *env, int base);
HandlerScope *__osprey_handler_suspend_scope(const char *effect_name,
                                             const char *operation_name);
void __osprey_handler_restore_scope(HandlerScope *scope);
void __osprey_handler_stack_cleanup(void);

// Operand kinds in an operation mailbox. A MANAGED slot holds a heap pointer
// the mailbox OWNS — the performer hands over its +1 at suspend and retiring
// the mailbox releases it. A SCALAR slot is a bare machine word nobody owns.
// Codegen emits this same numbering (crates/osprey-codegen/src/effects.rs), so
// the two sides must be changed together: a slot mis-tagged MANAGED releases a
// reference that was never taken, and one mis-tagged SCALAR leaks.
// Implements [EFFECTS-OPERATION-MAILBOX].
#define OSP_OP_ARG_SCALAR 0
#define OSP_OP_ARG_MANAGED 1

#endif // OSPREY_EFFECTS_RUNTIME_H

// Shared surface between the two halves of the algebraic-effect runtime: the
// dynamic handler stack (effects_runtime.c) and the thread-as-continuation
// machinery that implements `resume` (effects_coro.c).
#ifndef OSPREY_EFFECTS_RUNTIME_H
#define OSPREY_EFFECTS_RUNTIME_H

#include <stdint.h>

// Every (effect instantiation, operation) pair a program can perform is
// interned by codegen (crates/osprey-codegen/src/effects.rs) to a dense id
// below this bound; the runtime keeps one evidence slot per id. Codegen
// refuses a program that needs more, so the two sides must change together.
#define OSP_MAX_OPERATION_IDS 4096

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
int __osprey_handler_push_scoped(int operation_id, void *handler_func_ptr, void *env, int base);
HandlerScope *__osprey_handler_suspend_scope(int operation_id);
void __osprey_handler_restore_scope(HandlerScope *scope);

// Capability identities and masking [EFFECTS-INSTANCE] [EFFECTS-MASK]. The
// unsuffixed lookups above answer the default identity.
int64_t __osprey_instance_new(void);
int __osprey_handler_push_in(int operation_id, int64_t instance, void *handler_func_ptr,
                             void *env, int base);
int __osprey_handler_mask(int operation_id, int64_t instance);
void *__osprey_handler_lookup_in(int operation_id, int64_t instance);
void *__osprey_handler_lookup_env_in(int operation_id, int64_t instance);
HandlerScope *__osprey_handler_suspend_scope_in(int operation_id, int64_t instance);
void __osprey_handler_stack_cleanup(void);

// `finally` clauses [EFFECTS-FINALIZATION]: a cleanup function consumes its env.
typedef void (*OspFinalizer)(void *env);
void __osprey_finalizer_push(OspFinalizer run, void *env);
void __osprey_finalizer_finish(void);
void __osprey_finalizer_unwind(void);

// Stack-resident owner frames mirror codegen's null-initialized ARC slots.
// Abandonment runs user finalizers before releasing the remaining owners.
typedef struct OspOwnerFrame {
    struct OspOwnerFrame *previous;
    void ***slots;
    int64_t count;
} OspOwnerFrame;
void __osprey_owner_frame_push(void *frame, void ***slots, int64_t count);
void __osprey_owner_frame_pop(void *frame);
void __osprey_owner_frames_unwind(void);

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

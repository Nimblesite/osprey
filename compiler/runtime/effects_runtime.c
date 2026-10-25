// effects_runtime.c - Runtime handler stack for algebraic effects
// Implements dynamic handler resolution for nested effect handlers.
//
// Every operation the program can perform is interned by codegen to a small
// integer, and the stack keeps one evidence slot per operation id holding the
// stack index of its innermost live handler. A `perform` is therefore one
// array index — the runtime-resident form of Koka's evidence vector — never a
// scan and never a string compare. Implements [EFFECTS-HANDLE-REST].
//
// The stack is thread-local and every function here touches only the calling
// thread's copy: a fiber or continuation thread receives its handlers as a
// snapshot that it installs into its own stack, so no lock is needed.
//
// The `resume` half — thread-as-continuation, the operation mailbox and the
// coroutine drive protocol — lives in effects_coro.c, which shares only the
// handler snapshot declared in effects_runtime.h.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>  // int64_t — explicit so the wasm32-wasip1 sysroot resolves it

#include "effects_runtime.h"
#include "memory_hooks.h"

// Maximum handler stack depth per fiber
#define MAX_HANDLER_STACK_DEPTH 1024
// No handler is live for an operation: the empty evidence slot.
#define NO_HANDLER (-1)

// The default capability identity: unnamed `perform E.op` and `handler E`.
#define DEFAULT_INSTANCE 0

// HandlerEntry represents a single handler on the stack
typedef struct {
    int operation_id;
    int activation_base;
    int shadowed;            // Stack index this entry hides for its operation, or NO_HANDLER
    // The entry a lookup through this one resolves to: its own index for an
    // installed handler, the next outer activation for a `mask` entry, or
    // NO_HANDLER when a mask skipped the last one ([EFFECTS-MASK]).
    int answers;
    int64_t instance;        // Capability identity this entry answers ([EFFECTS-INSTANCE])
    void *handler_func_ptr;  // Function pointer to handler
    void *env;               // Captured environment (cells + values), or NULL
} HandlerEntry;

// HandlerStack per thread/fiber
typedef struct {
    HandlerEntry stack[MAX_HANDLER_STACK_DEPTH];
    int evidence[OSP_MAX_OPERATION_IDS];  // operation id -> innermost live entry
    int top;  // Index of top element (-1 means empty)
    HandlerScope *suspended;
} HandlerStack;

struct HandlerScope {
    HandlerScope *previous;
    HandlerStack *owner;
    int base;
    int count;
    HandlerEntry entries[];
};

static __thread HandlerStack *g_handler_stack = NULL;

static void clear_evidence(HandlerStack *stack) {
    for (int id = 0; id < OSP_MAX_OPERATION_IDS; id++) {
        stack->evidence[id] = NO_HANDLER;
    }
}

// Initialize handler stack for current thread
static void ensure_handler_stack_initialized(void) {
    if (g_handler_stack == NULL) {
        g_handler_stack = (HandlerStack *)malloc(sizeof(HandlerStack));
        if (g_handler_stack == NULL) {
            fprintf(stderr, "FATAL: Failed to allocate handler stack\n");
            abort();
        }
        g_handler_stack->top = -1;
        g_handler_stack->suspended = NULL;
        clear_evidence(g_handler_stack);
    }
}

// Make the entry at `index` the innermost handler of its operation, remembering
// which entry it hides so unlinking restores it.
static void link_evidence(HandlerStack *stack, int index) {
    HandlerEntry *entry = &stack->stack[index];
    entry->shadowed = stack->evidence[entry->operation_id];
    stack->evidence[entry->operation_id] = index;
}

// Undo link_evidence for the entry at `index`; entries unlink in LIFO order.
static void unlink_evidence(HandlerStack *stack, int index) {
    HandlerEntry *entry = &stack->stack[index];
    stack->evidence[entry->operation_id] = entry->shadowed;
}

static void link_range(HandlerStack *stack, int from, int to_exclusive) {
    for (int i = from; i < to_exclusive; i++) {
        link_evidence(stack, i);
    }
}

static void unlink_range(HandlerStack *stack, int from, int to_exclusive) {
    for (int i = to_exclusive - 1; i >= from; i--) {
        unlink_evidence(stack, i);
    }
}

static int push_rejected(int operation_id, int base) {
    if (operation_id < 0 || operation_id >= OSP_MAX_OPERATION_IDS) {
        fprintf(stderr, "FATAL: Invalid operation id %d\n", operation_id);
        return 1;
    }
    if (base < 0 || base > g_handler_stack->top + 1) {
        fprintf(stderr, "FATAL: Invalid handler activation depth %d\n", base);
        return 1;
    }
    if (g_handler_stack->top >= MAX_HANDLER_STACK_DEPTH - 1) {
        fprintf(stderr, "FATAL: Handler stack overflow (depth > %d)\n", MAX_HANDLER_STACK_DEPTH);
        return 1;
    }
    return 0;
}

// Push a handler onto the stack, with its captured environment (cells +
// values shared by every arm of one `handle` region; NULL when nothing is
// captured).
// Returns 0 on success, -1 on a bad id, a bad activation depth or overflow
int __osprey_handler_push_in(int operation_id, int64_t instance, void *handler_func_ptr,
                             void *env, int base) {
    ensure_handler_stack_initialized();
    if (push_rejected(operation_id, base)) {
        return -1;
    }
    g_handler_stack->top++;
    HandlerEntry *entry = &g_handler_stack->stack[g_handler_stack->top];
    entry->operation_id = operation_id;
    entry->handler_func_ptr = handler_func_ptr;
    entry->env = env;
    entry->activation_base = base;
    entry->answers = g_handler_stack->top;
    entry->instance = instance;
    link_evidence(g_handler_stack, g_handler_stack->top);
    return 0;
}

int __osprey_handler_push_scoped(int operation_id, void *handler_func_ptr, void *env, int base) {
    return __osprey_handler_push_in(operation_id, DEFAULT_INSTANCE, handler_func_ptr, env, base);
}

// The innermost visible entry for (operation, instance), starting at `index`
// and following each entry's shadowed link. Entries of other instances are
// transparent to it.
static int visible_entry(int index, int64_t instance) {
    while (index != NO_HANDLER && g_handler_stack->stack[index].instance != instance) {
        index = g_handler_stack->stack[index].shadowed;
    }
    return index;
}

static int resolved_entry(int operation_id, int64_t instance) {
    int index = visible_entry(g_handler_stack->evidence[operation_id], instance);
    return index == NO_HANDLER ? NO_HANDLER : g_handler_stack->stack[index].answers;
}

// `mask`: hide the activation that currently answers (operation, instance)
// from the rest of the masked body. The pushed entry answers whatever was
// visible just outside that activation; handlers installed later in the body
// shadow it as usual, and popping it restores the hidden activation. Nested
// masks skip successive activations. Implements [EFFECTS-MASK].
int __osprey_handler_mask(int operation_id, int64_t instance) {
    ensure_handler_stack_initialized();
    if (push_rejected(operation_id, g_handler_stack->top + 1)) {
        return -1;
    }
    int skipped = resolved_entry(operation_id, instance);
    int outer = skipped == NO_HANDLER
                    ? NO_HANDLER
                    : visible_entry(g_handler_stack->stack[skipped].shadowed, instance);
    g_handler_stack->top++;
    HandlerEntry *entry = &g_handler_stack->stack[g_handler_stack->top];
    entry->operation_id = operation_id;
    entry->handler_func_ptr = NULL;
    entry->env = NULL;
    entry->activation_base = g_handler_stack->top;
    entry->answers = outer == NO_HANDLER ? NO_HANDLER : g_handler_stack->stack[outer].answers;
    entry->instance = instance;
    link_evidence(g_handler_stack, g_handler_stack->top);
    return 0;
}

// A fresh capability identity for `instance E` ([EFFECTS-INSTANCE]). Identities
// are process-wide so a capability keeps its meaning across fiber threads.
int64_t __osprey_instance_new(void) {
    static int64_t next = DEFAULT_INSTANCE;
    return __atomic_add_fetch(&next, 1, __ATOMIC_RELAXED);
}

// Pop a handler from the stack
// Returns 0 on success, -1 on stack underflow
int __osprey_handler_pop(void) {
    ensure_handler_stack_initialized();
    if (g_handler_stack->top < 0) {
        fprintf(stderr, "FATAL: Handler stack underflow\n");
        return -1;
    }
    unlink_evidence(g_handler_stack, g_handler_stack->top);
    g_handler_stack->top--;
    return 0;
}

static HandlerEntry *find_handler(int operation_id, int64_t instance) {
    ensure_handler_stack_initialized();
    if (operation_id < 0 || operation_id >= OSP_MAX_OPERATION_IDS) {
        return NULL;
    }
    int index = resolved_entry(operation_id, instance);
    return index == NO_HANDLER ? NULL : &g_handler_stack->stack[index];
}

// Both lookups resolve the same active entry, including its captured environment.
void *__osprey_handler_lookup_in(int operation_id, int64_t instance) {
    HandlerEntry *entry = find_handler(operation_id, instance);
    return entry == NULL ? NULL : entry->handler_func_ptr;
}

void *__osprey_handler_lookup_env_in(int operation_id, int64_t instance) {
    HandlerEntry *entry = find_handler(operation_id, instance);
    return entry == NULL ? NULL : entry->env;
}

void *__osprey_handler_lookup(int operation_id) {
    return __osprey_handler_lookup_in(operation_id, DEFAULT_INSTANCE);
}

void *__osprey_handler_lookup_env(int operation_id) {
    return __osprey_handler_lookup_env_in(operation_id, DEFAULT_INSTANCE);
}

static HandlerScope *save_scope_tail(int base) {
    int count = g_handler_stack->top + 1 - base;
    size_t bytes = (size_t)count * sizeof(HandlerEntry);
    HandlerScope *scope = malloc(sizeof(HandlerScope) + bytes);
    if (scope == NULL) {
        fprintf(stderr, "FATAL: Failed to allocate suspended handler scope\n");
        abort();
    }
    scope->previous = g_handler_stack->suspended;
    scope->owner = g_handler_stack;
    scope->base = base;
    scope->count = count;
    unlink_range(g_handler_stack, base, g_handler_stack->top + 1);
    memcpy(scope->entries, &g_handler_stack->stack[base], bytes);
    g_handler_stack->suspended = scope;
    g_handler_stack->top = base - 1;
    return scope;
}

HandlerScope *__osprey_handler_suspend_scope_in(int operation_id, int64_t instance) {
    HandlerEntry *entry = find_handler(operation_id, instance);
    if (entry == NULL) {
        fprintf(stderr, "FATAL: Cannot suspend missing handler for operation %d\n",
                operation_id);
        abort();
    }
    return save_scope_tail(entry->activation_base);
}

HandlerScope *__osprey_handler_suspend_scope(int operation_id) {
    return __osprey_handler_suspend_scope_in(operation_id, DEFAULT_INSTANCE);
}

void __osprey_handler_restore_scope(HandlerScope *scope) {
    ensure_handler_stack_initialized();
    if (scope == NULL || scope != g_handler_stack->suspended ||
        scope->owner != g_handler_stack || scope->base != g_handler_stack->top + 1) {
        fprintf(stderr, "FATAL: Unbalanced handler scope restoration\n");
        abort();
    }
    memcpy(&g_handler_stack->stack[scope->base], scope->entries,
           (size_t)scope->count * sizeof(HandlerEntry));
    g_handler_stack->top += scope->count;
    link_range(g_handler_stack, scope->base, g_handler_stack->top + 1);
    g_handler_stack->suspended = scope->previous;
    free(scope);
}

// Current activation boundary for scoped handler installation.
int __osprey_handler_depth(void) {
    ensure_handler_stack_initialized();
    return g_handler_stack->top + 1;
}

// One pending `finally` clause: the cleanup function, the environment it
// consumes, and the handler depth of its activation's base, so it runs outside
// that activation. Implements [EFFECTS-FINALIZATION].
typedef struct {
    OspFinalizer run;
    void *env;
    int depth;
} Finalizer;

// This thread's pending finalizers, innermost last. Never part of a handler
// snapshot: a finalizer belongs to the thread that owns its activation.
typedef struct {
    Finalizer *items;
    int count;
    int capacity;
} FinalizerStack;

static __thread FinalizerStack g_finalizers = {NULL, 0, 0};

void __osprey_finalizer_push(OspFinalizer run, void *env) {
    ensure_handler_stack_initialized();
    if (g_finalizers.count == g_finalizers.capacity) {
        int capacity = g_finalizers.capacity == 0 ? 8 : g_finalizers.capacity * 2;
        Finalizer *grown = realloc(g_finalizers.items, (size_t)capacity * sizeof(Finalizer));
        if (grown == NULL) {
            fprintf(stderr, "FATAL: Failed to allocate a handler finalizer\n");
            abort();
        }
        g_finalizers.items = grown;
        g_finalizers.capacity = capacity;
    }
    Finalizer *slot = &g_finalizers.items[g_finalizers.count++];
    slot->run = run;
    slot->env = env;
    slot->depth = g_handler_stack->top + 1;
}

// Pop this thread's innermost finalizer and run it outside its activation:
// entries above its base are unlinked first. Popping BEFORE running is what
// makes a finalizer that itself transfers control never run twice.
static void run_innermost_finalizer(void) {
    Finalizer finalizer = g_finalizers.items[--g_finalizers.count];
    while (g_handler_stack->top + 1 > finalizer.depth) {
        (void)__osprey_handler_pop();
    }
    finalizer.run(finalizer.env);
}

// Normal completion: the activation's return clause has produced its answer.
void __osprey_finalizer_finish(void) {
    ensure_handler_stack_initialized();
    if (g_finalizers.count == 0) {
        fprintf(stderr, "FATAL: Handler finalizer stack underflow\n");
        abort();
    }
    run_innermost_finalizer();
}

// Abandonment: a dropped continuation finalizes every activation it still
// owns, inner to outer, each exactly once ([MULTI-COST-ABORT]).
void __osprey_finalizer_unwind(void) {
    ensure_handler_stack_initialized();
    while (g_finalizers.count > 0) {
        run_innermost_finalizer();
    }
}

// Implements [GC-ARC-PERCEUS] and [MULTI-COST-ABORT]: ordinary function
// epilogues cannot run after pthread_exit abandons their suspended stack.
static __thread OspOwnerFrame *g_owner_frames = NULL;

void __osprey_owner_frame_push(void *raw, void ***slots, int64_t count) {
    OspOwnerFrame *frame = raw;
    frame->previous = g_owner_frames;
    frame->slots = slots;
    frame->count = count;
    g_owner_frames = frame;
}

void __osprey_owner_frame_pop(void *raw) {
    if (g_owner_frames == NULL || g_owner_frames != raw) {
        fprintf(stderr, "FATAL: ARC owner frames returned out of order\n");
        abort();
    }
    g_owner_frames = g_owner_frames->previous;
}

void __osprey_owner_frames_unwind(void) {
    while (g_owner_frames != NULL) {
        OspOwnerFrame *frame = g_owner_frames;
        g_owner_frames = frame->previous;
        for (int64_t i = frame->count; i > 0; i--) {
            void **slot = frame->slots[i - 1];
            void *value = *slot;
            *slot = NULL;
            osp_release(value);
        }
    }
}

// Cleanup handler stack (call at thread exit)
void __osprey_handler_stack_cleanup(void) {
    free(g_finalizers.items);
    g_finalizers.items = NULL;
    g_finalizers.count = 0;
    g_finalizers.capacity = 0;
    if (g_handler_stack != NULL) {
        while (g_handler_stack->suspended != NULL) {
            HandlerScope *scope = g_handler_stack->suspended;
            g_handler_stack->suspended = scope->previous;
            free(scope);
        }
        free(g_handler_stack);
        g_handler_stack = NULL;
    }
}

// HandlerSnapshot for copying handler state across fiber boundaries
struct HandlerSnapshot {
    HandlerEntry entries[MAX_HANDLER_STACK_DEPTH];
    int count;
};

// Snapshot the current thread's handler stack (called in parent before fiber_spawn)
// Returns a heap-allocated snapshot that the caller must pass to __osprey_handler_restore
HandlerSnapshot *__osprey_handler_snapshot(void) {
    ensure_handler_stack_initialized();
    HandlerSnapshot *snap = (HandlerSnapshot *)malloc(sizeof(HandlerSnapshot));
    if (snap == NULL) {
        fprintf(stderr, "FATAL: Failed to allocate handler snapshot\n");
        abort();
    }
    int depth = g_handler_stack->top + 1;
    snap->count = depth;
    memcpy(snap->entries, g_handler_stack->stack, (size_t)depth * sizeof(HandlerEntry));
    return snap;
}

// Restore a snapshot into the current thread's handler stack (called at fiber thread start)
// Frees the snapshot after restoring. The stack's evidence is rebuilt from the
// restored entries, so their origin thread's slots never leak across.
void __osprey_handler_restore(HandlerSnapshot *snap) {
    if (snap == NULL) return;
    ensure_handler_stack_initialized();
    int count = snap->count < MAX_HANDLER_STACK_DEPTH ? snap->count : MAX_HANDLER_STACK_DEPTH;
    memcpy(g_handler_stack->stack, snap->entries, (size_t)count * sizeof(HandlerEntry));
    g_handler_stack->top = count - 1;
    clear_evidence(g_handler_stack);
    link_range(g_handler_stack, 0, count);
    free(snap);
}

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

// Maximum handler stack depth per fiber
#define MAX_HANDLER_STACK_DEPTH 1024
// No handler is live for an operation: the empty evidence slot.
#define NO_HANDLER (-1)

// HandlerEntry represents a single handler on the stack
typedef struct {
    int operation_id;
    int activation_base;
    int shadowed;            // Stack index this entry hides for its operation, or NO_HANDLER
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
int __osprey_handler_push_scoped(int operation_id, void *handler_func_ptr, void *env, int base) {
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
    link_evidence(g_handler_stack, g_handler_stack->top);
    return 0;
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

static HandlerEntry *find_handler(int operation_id) {
    ensure_handler_stack_initialized();
    if (operation_id < 0 || operation_id >= OSP_MAX_OPERATION_IDS) {
        return NULL;
    }
    int index = g_handler_stack->evidence[operation_id];
    return index == NO_HANDLER ? NULL : &g_handler_stack->stack[index];
}

// Both lookups resolve the same active entry, including its captured environment.
void *__osprey_handler_lookup(int operation_id) {
    HandlerEntry *entry = find_handler(operation_id);
    return entry == NULL ? NULL : entry->handler_func_ptr;
}

void *__osprey_handler_lookup_env(int operation_id) {
    HandlerEntry *entry = find_handler(operation_id);
    return entry == NULL ? NULL : entry->env;
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

HandlerScope *__osprey_handler_suspend_scope(int operation_id) {
    HandlerEntry *entry = find_handler(operation_id);
    if (entry == NULL) {
        fprintf(stderr, "FATAL: Cannot suspend missing handler for operation %d\n",
                operation_id);
        abort();
    }
    return save_scope_tail(entry->activation_base);
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

// Cleanup handler stack (call at thread exit)
void __osprey_handler_stack_cleanup(void) {
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

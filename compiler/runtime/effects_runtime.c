// effects_runtime.c - Runtime handler stack for algebraic effects
// Implements dynamic handler resolution for nested effect handlers.
//
// The `resume` half — thread-as-continuation, the operation mailbox and the
// coroutine drive protocol — lives in effects_coro.c, which shares only the
// handler snapshot declared in effects_runtime.h.

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>  // int64_t — explicit so the wasm32-wasip1 sysroot resolves it
#include <pthread.h>

#include "effects_runtime.h"
#ifdef __wasm__
// wasm32-wasip1 is single-threaded: the effect handler stack needs no real
// locking, so the mutex ops become no-ops. effects_coro.c is excluded from the
// wasm archive wholesale — it needs pthread_create/cond/join/exit, which
// wasi-libc cannot honour. With those symbols absent, resumable-effect programs
// link-fail and are SKIPped by the wasm golden suite, exactly like the
// fiber/HTTP runtimes. [WASM-TARGET-EFFECTS]
#define pthread_mutex_init(m, a) ((void)(m), (void)(a), 0)
#define pthread_mutex_lock(m) ((void)(m), 0)
#define pthread_mutex_unlock(m) ((void)(m), 0)
#define pthread_mutex_destroy(m) ((void)(m), 0)
#endif

// Maximum handler stack depth per fiber
#define MAX_HANDLER_STACK_DEPTH 1024
#define MAX_EFFECT_NAME_LENGTH 128
#define MAX_OPERATION_NAME_LENGTH 128

// HandlerEntry represents a single handler on the stack
typedef struct {
    char effect_name[MAX_EFFECT_NAME_LENGTH];
    char operation_name[MAX_OPERATION_NAME_LENGTH];
    void *handler_func_ptr;  // Function pointer to handler
    void *env;               // Captured environment (cells + values), or NULL
    int activation_base;
} HandlerEntry;

// HandlerStack per thread/fiber
typedef struct {
    HandlerEntry stack[MAX_HANDLER_STACK_DEPTH];
    int top;  // Index of top element (-1 means empty)
    pthread_mutex_t lock;  // Thread safety
    HandlerScope *suspended;
} HandlerStack;

struct HandlerScope {
    HandlerScope *previous;
    HandlerStack *owner;
    int base;
    int count;
    HandlerEntry entries[];
};

// Global handler stack (thread-local storage would be better for production)
static __thread HandlerStack *g_handler_stack = NULL;

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
        pthread_mutex_init(&g_handler_stack->lock, NULL);
    }
}

// Push a handler onto the stack, with its captured environment (cells +
// values shared by every arm of one `handle` region; NULL when nothing is
// captured).
// Returns 0 on success, -1 on stack overflow
int __osprey_handler_push(const char *effect_name, const char *operation_name, void *handler_func_ptr, void *env) {
    return __osprey_handler_push_scoped(effect_name, operation_name, handler_func_ptr,
                                         env, __osprey_handler_depth());
}

int __osprey_handler_push_scoped(const char *effect_name, const char *operation_name,
                                void *handler_func_ptr, void *env, int base) {
    ensure_handler_stack_initialized();

    pthread_mutex_lock(&g_handler_stack->lock);

    if (base < 0 || base > g_handler_stack->top + 1) {
        pthread_mutex_unlock(&g_handler_stack->lock);
        fprintf(stderr, "FATAL: Invalid handler activation depth %d\n", base);
        return -1;
    }

    if (g_handler_stack->top >= MAX_HANDLER_STACK_DEPTH - 1) {
        pthread_mutex_unlock(&g_handler_stack->lock);
        fprintf(stderr, "FATAL: Handler stack overflow (depth > %d)\n", MAX_HANDLER_STACK_DEPTH);
        return -1;
    }

    g_handler_stack->top++;
    HandlerEntry *entry = &g_handler_stack->stack[g_handler_stack->top];

    strncpy(entry->effect_name, effect_name, MAX_EFFECT_NAME_LENGTH - 1);
    entry->effect_name[MAX_EFFECT_NAME_LENGTH - 1] = '\0';

    strncpy(entry->operation_name, operation_name, MAX_OPERATION_NAME_LENGTH - 1);
    entry->operation_name[MAX_OPERATION_NAME_LENGTH - 1] = '\0';

    entry->handler_func_ptr = handler_func_ptr;
    entry->env = env;
    entry->activation_base = base;

    pthread_mutex_unlock(&g_handler_stack->lock);
    return 0;
}

// Pop a handler from the stack
// Returns 0 on success, -1 on stack underflow
int __osprey_handler_pop(void) {
    ensure_handler_stack_initialized();

    pthread_mutex_lock(&g_handler_stack->lock);

    if (g_handler_stack->top < 0) {
        pthread_mutex_unlock(&g_handler_stack->lock);
        fprintf(stderr, "FATAL: Handler stack underflow\n");
        return -1;
    }

    g_handler_stack->top--;

    pthread_mutex_unlock(&g_handler_stack->lock);
    return 0;
}

static int find_handler(const char *effect_name, const char *operation_name) {
    for (int i = g_handler_stack->top; i >= 0; i--) {
        HandlerEntry *entry = &g_handler_stack->stack[i];
        if (strcmp(entry->effect_name, effect_name) == 0 &&
            strcmp(entry->operation_name, operation_name) == 0) {
            return i;
        }
    }
    return -1;
}

// Both lookups resolve the same active entry, including its captured environment.
void *__osprey_handler_lookup(const char *effect_name, const char *operation_name) {
    ensure_handler_stack_initialized();
    pthread_mutex_lock(&g_handler_stack->lock);
    int index = find_handler(effect_name, operation_name);
    void *result = index < 0 ? NULL : g_handler_stack->stack[index].handler_func_ptr;
    pthread_mutex_unlock(&g_handler_stack->lock);
    return result;
}

void *__osprey_handler_lookup_env(const char *effect_name, const char *operation_name) {
    ensure_handler_stack_initialized();
    pthread_mutex_lock(&g_handler_stack->lock);
    int index = find_handler(effect_name, operation_name);
    void *result = index < 0 ? NULL : g_handler_stack->stack[index].env;
    pthread_mutex_unlock(&g_handler_stack->lock);
    return result;
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
    memcpy(scope->entries, &g_handler_stack->stack[base], bytes);
    g_handler_stack->suspended = scope;
    g_handler_stack->top = base - 1;
    return scope;
}

HandlerScope *__osprey_handler_suspend_scope(const char *effect_name,
                                             const char *operation_name) {
    ensure_handler_stack_initialized();
    pthread_mutex_lock(&g_handler_stack->lock);
    int index = find_handler(effect_name, operation_name);
    if (index < 0) {
        pthread_mutex_unlock(&g_handler_stack->lock);
        fprintf(stderr, "FATAL: Cannot suspend missing handler %s.%s\n",
                effect_name, operation_name);
        abort();
    }
    HandlerScope *scope = save_scope_tail(g_handler_stack->stack[index].activation_base);
    pthread_mutex_unlock(&g_handler_stack->lock);
    return scope;
}

void __osprey_handler_restore_scope(HandlerScope *scope) {
    ensure_handler_stack_initialized();
    pthread_mutex_lock(&g_handler_stack->lock);
    if (scope == NULL || scope != g_handler_stack->suspended ||
        scope->owner != g_handler_stack || scope->base != g_handler_stack->top + 1) {
        pthread_mutex_unlock(&g_handler_stack->lock);
        fprintf(stderr, "FATAL: Unbalanced handler scope restoration\n");
        abort();
    }
    memcpy(&g_handler_stack->stack[scope->base], scope->entries,
           (size_t)scope->count * sizeof(HandlerEntry));
    g_handler_stack->top += scope->count;
    g_handler_stack->suspended = scope->previous;
    pthread_mutex_unlock(&g_handler_stack->lock);
    free(scope);
}

// Get current stack depth (for debugging)
int __osprey_handler_depth(void) {
    ensure_handler_stack_initialized();

    pthread_mutex_lock(&g_handler_stack->lock);
    int depth = g_handler_stack->top + 1;
    pthread_mutex_unlock(&g_handler_stack->lock);

    return depth;
}

int __osprey_handler_stack_depth(void) {
    return __osprey_handler_depth();
}

// Cleanup handler stack (call at thread exit)
void __osprey_handler_stack_cleanup(void) {
    if (g_handler_stack != NULL) {
        while (g_handler_stack->suspended != NULL) {
            HandlerScope *scope = g_handler_stack->suspended;
            g_handler_stack->suspended = scope->previous;
            free(scope);
        }
        pthread_mutex_destroy(&g_handler_stack->lock);
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

    pthread_mutex_lock(&g_handler_stack->lock);
    int depth = g_handler_stack->top + 1;
    snap->count = depth;
    for (int i = 0; i < depth; i++) {
        snap->entries[i] = g_handler_stack->stack[i];
    }
    pthread_mutex_unlock(&g_handler_stack->lock);

    return snap;
}

// Restore a snapshot into the current thread's handler stack (called at fiber thread start)
// Frees the snapshot after restoring.
void __osprey_handler_restore(HandlerSnapshot *snap) {
    if (snap == NULL) return;

    ensure_handler_stack_initialized();

    pthread_mutex_lock(&g_handler_stack->lock);
    for (int i = 0; i < snap->count && i < MAX_HANDLER_STACK_DEPTH; i++) {
        g_handler_stack->stack[i] = snap->entries[i];
    }
    g_handler_stack->top = snap->count - 1;
    pthread_mutex_unlock(&g_handler_stack->lock);

    free(snap);
}

// effects_runtime.c - Runtime handler stack for algebraic effects
// Implements dynamic handler resolution for nested effect handlers

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdbool.h>
#include <stdint.h>  // int64_t — explicit so the wasm32-wasip1 sysroot resolves it
#include <pthread.h>
#ifdef __wasm__
// wasm32-wasip1 is single-threaded: the effect handler stack needs no real
// locking, so the mutex ops become no-ops. The thread-based coroutine
// continuation section (struct OspreyCoro onward) is excluded wholesale for
// wasm via `#ifndef __wasm__` — it needs pthread_create/cond/join/exit, which
// wasi-libc cannot honour. With those symbols absent from the wasm archive,
// resumable-effect programs link-fail and are SKIPped by the wasm golden suite,
// exactly like the fiber/HTTP runtimes. [WASM-TARGET-EFFECTS]
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
} HandlerEntry;

// HandlerStack per thread/fiber
typedef struct {
    HandlerEntry stack[MAX_HANDLER_STACK_DEPTH];
    int top;  // Index of top element (-1 means empty)
    pthread_mutex_t lock;  // Thread safety
} HandlerStack;

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
        pthread_mutex_init(&g_handler_stack->lock, NULL);
    }
}

// Push a handler onto the stack, with its captured environment (cells +
// values shared by every arm of one `handle` region; NULL when nothing is
// captured).
// Returns 0 on success, -1 on stack overflow
int __osprey_handler_push(const char *effect_name, const char *operation_name, void *handler_func_ptr, void *env) {
    ensure_handler_stack_initialized();

    pthread_mutex_lock(&g_handler_stack->lock);

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

// Look up handler from stack (searches from top to bottom)
// Returns handler function pointer, or NULL if not found
void *__osprey_handler_lookup(const char *effect_name, const char *operation_name) {
    ensure_handler_stack_initialized();

    pthread_mutex_lock(&g_handler_stack->lock);

    // Search from top of stack (most recent handler) to bottom
    for (int i = g_handler_stack->top; i >= 0; i--) {
        HandlerEntry *entry = &g_handler_stack->stack[i];
        if (strcmp(entry->effect_name, effect_name) == 0 &&
            strcmp(entry->operation_name, operation_name) == 0) {
            void *result = entry->handler_func_ptr;
            pthread_mutex_unlock(&g_handler_stack->lock);
            return result;
        }
    }

    pthread_mutex_unlock(&g_handler_stack->lock);
    return NULL;  // Handler not found
}

// Look up the captured environment of the innermost matching handler — the
// companion to __osprey_handler_lookup, resolved the same top-to-bottom way so
// fnptr and env always come from the same handler entry.
// Returns the env pointer, or NULL if not found / no captures.
void *__osprey_handler_lookup_env(const char *effect_name, const char *operation_name) {
    ensure_handler_stack_initialized();

    pthread_mutex_lock(&g_handler_stack->lock);

    for (int i = g_handler_stack->top; i >= 0; i--) {
        HandlerEntry *entry = &g_handler_stack->stack[i];
        if (strcmp(entry->effect_name, effect_name) == 0 &&
            strcmp(entry->operation_name, operation_name) == 0) {
            void *result = entry->env;
            pthread_mutex_unlock(&g_handler_stack->lock);
            return result;
        }
    }

    pthread_mutex_unlock(&g_handler_stack->lock);
    return NULL;  // Handler not found
}

// Get current stack depth (for debugging)
int __osprey_handler_stack_depth(void) {
    ensure_handler_stack_initialized();

    pthread_mutex_lock(&g_handler_stack->lock);
    int depth = g_handler_stack->top + 1;
    pthread_mutex_unlock(&g_handler_stack->lock);

    return depth;
}

// Cleanup handler stack (call at thread exit)
void __osprey_handler_stack_cleanup(void) {
    if (g_handler_stack != NULL) {
        pthread_mutex_destroy(&g_handler_stack->lock);
        free(g_handler_stack);
        g_handler_stack = NULL;
    }
}

// HandlerSnapshot for copying handler state across fiber boundaries
typedef struct {
    HandlerEntry entries[MAX_HANDLER_STACK_DEPTH];
    int count;
} HandlerSnapshot;

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

// Thread-based effect continuations: a handler `resume` is implemented by
// running the handled computation on its own pthread and ping-ponging control
// via a condvar. wasm32-wasip1 has no usable pthreads, so this entire section
// is compiled only for native targets; on wasm the `__osprey_coro_*` symbols
// are intentionally absent, making resumable-effect programs link-fail and be
// SKIPped by the wasm golden suite. [WASM-TARGET-EFFECTS]
#ifndef __wasm__
typedef struct OspreyCoro {
    pthread_mutex_t lock;
    pthread_cond_t cond;
    pthread_t thread;
    bool started;
    bool joined;
    bool suspended;
    bool done;
    bool abort;
    // One perform occupies the op/args/resume_value channel at a time
    // [EFFECTS-FIBER-PERFORM]. Concurrent performers (fibers spawned inside
    // the handled body) queue on this flag instead of overwriting each
    // other's arguments and stealing each other's resume value.
    bool in_flight;
    int64_t op_id;
    int64_t args[16];
    int64_t arg_count;
    int64_t resume_value;
    int64_t result;
    void *region_env;
} OspreyCoro;

typedef struct CoroStartArgs {
    OspreyCoro *coro;
    int64_t (*body)(void *);
    void *body_env;
    HandlerSnapshot *snapshot;
} CoroStartArgs;

void *__osprey_coro_new(void *env) {
    OspreyCoro *coro = (OspreyCoro *)malloc(sizeof(OspreyCoro));
    if (coro == NULL) {
        fprintf(stderr, "FATAL: Failed to allocate effect continuation\n");
        abort();
    }
    pthread_mutex_init(&coro->lock, NULL);
    pthread_cond_init(&coro->cond, NULL);
    coro->started = false;
    coro->joined = false;
    coro->suspended = false;
    coro->done = false;
    coro->abort = false;
    coro->in_flight = false;
    coro->op_id = 0;
    coro->arg_count = 0;
    coro->resume_value = 0;
    coro->result = 0;
    coro->region_env = env;
    for (int i = 0; i < 16; i++) {
        coro->args[i] = 0;
    }
    return coro;
}

static void *__osprey_coro_thread(void *raw) {
    CoroStartArgs *args = (CoroStartArgs *)raw;
    OspreyCoro *coro = args->coro;
    if (args->snapshot != NULL) {
        __osprey_handler_restore(args->snapshot);
        args->snapshot = NULL;
    }
    int64_t result = args->body(args->body_env);
    free(args);

    pthread_mutex_lock(&coro->lock);
    coro->result = result;
    coro->done = true;
    coro->suspended = false;
    pthread_cond_broadcast(&coro->cond);
    pthread_mutex_unlock(&coro->lock);
    return NULL;
}

void __osprey_coro_start(void *raw, int64_t (*body)(void *), void *body_env, HandlerSnapshot *snapshot) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL || body == NULL) {
        fprintf(stderr, "FATAL: Invalid effect continuation start\n");
        abort();
    }
    CoroStartArgs *args = (CoroStartArgs *)malloc(sizeof(CoroStartArgs));
    if (args == NULL) {
        fprintf(stderr, "FATAL: Failed to allocate effect continuation start args\n");
        abort();
    }
    args->coro = coro;
    args->body = body;
    args->body_env = body_env;
    args->snapshot = snapshot;

    int rc = pthread_create(&coro->thread, NULL, __osprey_coro_thread, args);
    if (rc != 0) {
        free(args);
        fprintf(stderr, "FATAL: Failed to start effect continuation thread\n");
        abort();
    }
    pthread_mutex_lock(&coro->lock);
    coro->started = true;
    while (!coro->suspended && !coro->done) {
        pthread_cond_wait(&coro->cond, &coro->lock);
    }
    pthread_mutex_unlock(&coro->lock);
}

int64_t __osprey_coro_suspend(void *raw, int64_t op_id, int64_t *args, int64_t arg_count) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        return 0;
    }
    pthread_mutex_lock(&coro->lock);
    // Claim the channel [EFFECTS-FIBER-PERFORM]: a second concurrent perform
    // (e.g. from a sibling fiber) must wait its turn, or it would overwrite
    // this perform's arguments and both would consume the same resume value —
    // nondeterministic wrong answers with exit 0. The drive loop re-enters on
    // re-suspension, so a queued perform is dispatched as soon as the current
    // one's resume value is consumed.
    while (coro->in_flight && !coro->abort) {
        pthread_cond_wait(&coro->cond, &coro->lock);
    }
    if (coro->abort) {
        pthread_mutex_unlock(&coro->lock);
        pthread_exit(NULL);
    }
    coro->in_flight = true;
    coro->op_id = op_id;
    coro->arg_count = arg_count;
    int64_t capped = arg_count;
    if (capped > 16) {
        capped = 16;
    }
    for (int64_t i = 0; i < capped; i++) {
        coro->args[i] = args == NULL ? 0 : args[i];
    }
    coro->suspended = true;
    pthread_cond_broadcast(&coro->cond);
    while (coro->suspended && !coro->abort) {
        pthread_cond_wait(&coro->cond, &coro->lock);
    }
    if (coro->abort) {
        pthread_mutex_unlock(&coro->lock);
        pthread_exit(NULL);
    }
    int64_t resume_value = coro->resume_value;
    coro->in_flight = false;
    pthread_cond_broadcast(&coro->cond);
    pthread_mutex_unlock(&coro->lock);
    return resume_value;
}

int64_t __osprey_coro_resume(void *raw, int64_t value) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        return 0;
    }
    pthread_mutex_lock(&coro->lock);
    // Multi-shot rejection [EFFECTS-RESUME]: the thread-as-continuation model is
    // single-shot — a consumed (completed) pthread stack cannot be re-run. A
    // second `resume` on an already-finished continuation would silently return
    // the stale first result (a wrong answer with exit 0), so reject it loudly
    // instead. Legitimate re-entry (the body performed again) leaves the coro
    // suspended, not done, and never reaches this guard.
    if (coro->done) {
        pthread_mutex_unlock(&coro->lock);
        fprintf(stderr,
                "fatal: continuation already resumed "
                "(multi-shot resume is not supported)\n");
        exit(1);
    }
    coro->resume_value = value;
    coro->suspended = false;
    pthread_cond_broadcast(&coro->cond);
    while (!coro->suspended && !coro->done) {
        pthread_cond_wait(&coro->cond, &coro->lock);
    }
    int64_t result = coro->done ? coro->result : 0;
    pthread_mutex_unlock(&coro->lock);
    return result;
}

int64_t __osprey_coro_done(void *raw) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        return 1;
    }
    pthread_mutex_lock(&coro->lock);
    int64_t done = coro->done ? 1 : 0;
    pthread_mutex_unlock(&coro->lock);
    return done;
}

int64_t __osprey_coro_op(void *raw) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        return 0;
    }
    pthread_mutex_lock(&coro->lock);
    int64_t op = coro->op_id;
    pthread_mutex_unlock(&coro->lock);
    return op;
}

int64_t __osprey_coro_arg(void *raw, int64_t index) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL || index < 0 || index >= 16) {
        return 0;
    }
    pthread_mutex_lock(&coro->lock);
    int64_t arg = index < coro->arg_count ? coro->args[index] : 0;
    pthread_mutex_unlock(&coro->lock);
    return arg;
}

int64_t __osprey_coro_result(void *raw) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        return 0;
    }
    pthread_mutex_lock(&coro->lock);
    int64_t result = coro->result;
    pthread_mutex_unlock(&coro->lock);
    return result;
}

void __osprey_coro_abort(void *raw) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        return;
    }
    pthread_mutex_lock(&coro->lock);
    if (!coro->done) {
        coro->abort = true;
        coro->suspended = false;
        pthread_cond_broadcast(&coro->cond);
    }
    pthread_mutex_unlock(&coro->lock);
    if (coro->started && !coro->joined) {
        pthread_join(coro->thread, NULL);
        coro->joined = true;
    }
    pthread_mutex_lock(&coro->lock);
    coro->done = true;
    pthread_mutex_unlock(&coro->lock);
}

void __osprey_coro_free(void *raw) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        return;
    }
    if (coro->started && !coro->joined) {
        if (!coro->done) {
            __osprey_coro_abort(coro);
        } else {
            pthread_join(coro->thread, NULL);
            coro->joined = true;
        }
    }
    pthread_cond_destroy(&coro->cond);
    pthread_mutex_destroy(&coro->lock);
    free(coro);
}
#endif // !__wasm__ — thread-based effect continuations excluded on wasm32-wasip1

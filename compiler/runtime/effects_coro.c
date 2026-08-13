// effects_coro.c - Thread-as-continuation for resumable algebraic effects.
//
// A handler arm's `resume` needs the handled computation to be suspendable, so
// that computation runs on its own pthread and control ping-pongs across a
// condvar: the body thread blocks inside a `perform`, the host thread runs the
// matching arm, and `resume` hands a value back and unblocks the body.
//
// wasm32-wasip1 has no usable pthreads, so this whole unit is excluded from the
// wasm archive; resumable-effect programs link-fail there and are SKIPped by
// the wasm golden suite, exactly like the fiber and HTTP runtimes.
// [WASM-TARGET-EFFECTS]

#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <stdint.h>
#include <pthread.h>

#include "effects_runtime.h"
#include "memory_hooks.h"
#include "profiler_runtime.h"

// The payload of one in-flight `perform`, allocated per suspension and sized by
// the operation's REAL arity.
//
// This replaced a fixed `int64_t args[16]`, which kept the caller's declared
// arg_count but copied only sixteen words and answered every later index with
// zero. A seventeen-argument operation therefore made its policy decision from
// a fabricated 0 and the process exited successfully — silent data corruption
// with no diagnostic (#182).
//
// `kinds[i]` records whether `words[i]` is a managed pointer this mailbox OWNS
// or a bare scalar. The performer transfers its +1 at suspend and retiring the
// mailbox releases exactly the managed slots, so an operand cannot outlive the
// perform that sent it (a dangling read after `resume` returns) nor survive it
// (a leak, #185). Implements [EFFECTS-OPERATION-MAILBOX].
typedef struct {
    int64_t op_id;
    int64_t *words;
    uint8_t *kinds;
    int64_t count;
} OpMailbox;

typedef struct OspreyCoro {
    pthread_mutex_t lock;
    pthread_cond_t cond;
    pthread_t thread;
    bool started;
    bool joined;
    bool suspended;
    bool done;
    bool abort;
    // One perform occupies the mailbox/resume_value channel at a time
    // [EFFECTS-FIBER-PERFORM]. Concurrent performers (fibers spawned inside
    // the handled body) queue on this flag instead of overwriting each
    // other's arguments and stealing each other's resume value.
    bool in_flight;
    // The current suspension's payload, owned until a dispatcher takes it.
    OpMailbox *mail;
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

static void *checked_malloc(size_t size, const char *what) {
    void *p = malloc(size);
    if (p == NULL) {
        fprintf(stderr, "FATAL: Failed to allocate %s\n", what);
        abort();
    }
    return p;
}

// Take over the performer's +1 on every managed slot. `args`/`kinds` are the
// performer's stack arrays, valid only for this call, so the words are copied.
static OpMailbox *mailbox_new(int64_t op_id, const int64_t *args, const uint8_t *kinds,
                              int64_t count) {
    OpMailbox *mail = (OpMailbox *)checked_malloc(sizeof(OpMailbox), "effect operation mailbox");
    mail->op_id = op_id;
    mail->count = count > 0 ? count : 0;
    mail->words = NULL;
    mail->kinds = NULL;
    if (mail->count == 0) {
        return mail;
    }
    // A positive arity with no argument array can only come from a codegen bug,
    // and inventing zeros for it is the exact silent corruption this mailbox
    // exists to end. [EFFECTS-OPERATION-MAILBOX]
    if (args == NULL || kinds == NULL) {
        fprintf(stderr, "FATAL: effect operation %lld declares %lld arguments but sent none\n",
                (long long)op_id, (long long)count);
        abort();
    }
    mail->words = (int64_t *)checked_malloc((size_t)mail->count * sizeof(int64_t),
                                            "effect operation arguments");
    mail->kinds = (uint8_t *)checked_malloc((size_t)mail->count, "effect operation argument kinds");
    for (int64_t i = 0; i < mail->count; i++) {
        mail->words[i] = args[i];
        mail->kinds[i] = kinds[i];
    }
    return mail;
}

// Give back the +1 the performer transferred on every managed slot. The single
// place that decodes the OSP_OP_ARG_* kinds, so a mailbox being retired and a
// suspension that never built one cannot drift apart on which words are
// pointers. [EFFECTS-OPERATION-MAILBOX]
static void release_operands(const int64_t *words, const uint8_t *kinds, int64_t count) {
    if (words == NULL || kinds == NULL) {
        return;
    }
    for (int64_t i = 0; i < count; i++) {
        if (kinds[i] == OSP_OP_ARG_MANAGED) {
            osp_release((void *)(uintptr_t)words[i]);
        }
    }
}

static void mailbox_free(OpMailbox *mail) {
    if (mail == NULL) {
        return;
    }
    release_operands(mail->words, mail->kinds, mail->count);
    free(mail->words);
    free(mail->kinds);
    free(mail);
}

void *__osprey_coro_new(void *env) {
    OspreyCoro *coro = (OspreyCoro *)checked_malloc(sizeof(OspreyCoro), "effect continuation");
    pthread_mutex_init(&coro->lock, NULL);
    pthread_cond_init(&coro->cond, NULL);
    coro->started = false;
    coro->joined = false;
    coro->suspended = false;
    coro->done = false;
    coro->abort = false;
    coro->in_flight = false;
    coro->mail = NULL;
    coro->resume_value = 0;
    coro->result = 0;
    coro->region_env = env;
    return coro;
}

static void *__osprey_coro_thread(void *raw) {
    CoroStartArgs *args = (CoroStartArgs *)raw;
    OspreyCoro *coro = args->coro;
    // Effect continuations run on their own pthread; register so profiler
    // samples attribute to them distinctly [PROF-COLLECT-REGISTRY].
    osp_prof_thread_register(-1, "effect");
    if (args->snapshot != NULL) {
        __osprey_handler_restore(args->snapshot);
        args->snapshot = NULL;
    }
    int64_t result = args->body(args->body_env);
    free(args);
    osp_prof_thread_unregister();

    pthread_mutex_lock(&coro->lock);
    coro->result = result;
    coro->done = true;
    coro->suspended = false;
    pthread_cond_broadcast(&coro->cond);
    pthread_mutex_unlock(&coro->lock);
    return NULL;
}

void __osprey_coro_start(void *raw, int64_t (*body)(void *), void *body_env,
                         HandlerSnapshot *snapshot) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL || body == NULL) {
        fprintf(stderr, "FATAL: Invalid effect continuation start\n");
        abort();
    }
    CoroStartArgs *args =
        (CoroStartArgs *)checked_malloc(sizeof(CoroStartArgs), "effect continuation start args");
    args->coro = coro;
    args->body = body;
    args->body_env = body_env;
    args->snapshot = snapshot;

    // The body thread allocates and releases on the shared value heap while the
    // host thread runs handler arms on it, so the memory backend must leave its
    // single-threaded lock-free fast path BEFORE the second thread can exist —
    // pthread_create is the happens-before barrier. Without this every
    // resumable effect raced ARC's refcounts. [MEM-BACKENDS]
    osp_mem_notify_multithreaded();

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

int64_t __osprey_coro_suspend(void *raw, int64_t op_id, const int64_t *args, const uint8_t *kinds,
                              int64_t arg_count) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        // Same rule as the aborted path below: the operands arrived at +1 for a
        // mailbox, and without a continuation there is nothing to build one.
        release_operands(args, kinds, arg_count);
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
        // Killed while queued behind another perform: no mailbox was built, so
        // nothing downstream will ever release these operands. Done outside the
        // lock — the memory backend must not be entered holding it.
        release_operands(args, kinds, arg_count);
        pthread_exit(NULL);
    }
    coro->in_flight = true;
    coro->mail = mailbox_new(op_id, args, kinds, arg_count);
    coro->suspended = true;
    pthread_cond_broadcast(&coro->cond);
    while (coro->suspended && !coro->abort) {
        pthread_cond_wait(&coro->cond, &coro->lock);
    }
    if (coro->abort) {
        pthread_mutex_unlock(&coro->lock);
        // The handoff already happened here, so the operands are NOT this
        // thread's to release: either the dispatcher took the mailbox and
        // retires it with __osprey_coro_mail_free, or it is still in
        // `coro->mail` and __osprey_coro_free retires it. Releasing again would
        // be a double free, not a leak fix.
        pthread_exit(NULL);
    }
    int64_t resume_value = coro->resume_value;
    coro->in_flight = false;
    pthread_cond_broadcast(&coro->cond);
    pthread_mutex_unlock(&coro->lock);
    return resume_value;
}

// Hand the current suspension's mailbox to the dispatcher, which owns it from
// here and must retire it with __osprey_coro_mail_free. Clearing the slot is
// what lets an arm resume and have the body perform again: the nested
// suspension installs a fresh mailbox instead of overwriting one still in use.
void *__osprey_coro_take_args(void *raw) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        return NULL;
    }
    pthread_mutex_lock(&coro->lock);
    OpMailbox *mail = coro->mail;
    coro->mail = NULL;
    pthread_mutex_unlock(&coro->lock);
    return mail;
}

int64_t __osprey_coro_mail_op(void *raw) {
    // Dispatching with no mailbox would select an arm from an invented
    // operation id and silently run the wrong handler. There is no correct
    // value to return. [EFFECTS-OPERATION-MAILBOX]
    if (raw == NULL) {
        fprintf(stderr, "FATAL: effect dispatch with no operation mailbox\n");
        abort();
    }
    return ((OpMailbox *)raw)->op_id;
}

int64_t __osprey_coro_mail_arg(void *raw, int64_t index) {
    OpMailbox *mail = (OpMailbox *)raw;
    // Answering an out-of-range slot with 0 is precisely the corruption this
    // mailbox replaced. A dispatcher only ever reads indices below the arity
    // its own signature declared, so reaching here is a compiler bug and the
    // only honest response is to stop. [EFFECTS-OPERATION-MAILBOX]
    if (mail == NULL || index < 0 || index >= mail->count) {
        fprintf(stderr, "FATAL: effect operation argument %lld is outside the %lld sent\n",
                (long long)index, (long long)(mail == NULL ? 0 : mail->count));
        abort();
    }
    return mail->words[index];
}

void __osprey_coro_mail_free(void *raw) { mailbox_free((OpMailbox *)raw); }

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
    // A suspension nobody dispatched — an aborted region, or an operation whose
    // id matched no arm — still holds its managed operands. Retire it here or
    // they outlive the program. [EFFECTS-OPERATION-MAILBOX]
    mailbox_free(coro->mail);
    coro->mail = NULL;
    pthread_cond_destroy(&coro->cond);
    pthread_mutex_destroy(&coro->lock);
    free(coro);
}

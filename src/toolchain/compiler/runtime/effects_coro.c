// effects_coro.c - Thread-as-continuation for resumable algebraic effects.
//
// A handler arm's `resume` needs the handled computation to be suspendable, so
// that computation runs on its own pthread and control ping-pongs across a
// condvar: the body thread blocks inside a `perform`, the host thread runs the
// matching arm, and `resume` hands a value back and unblocks the body.
//
// wasm32-wasip1 has no usable pthreads, so this whole unit is excluded from the
// wasm archive. The target capability check rejects dynamic resumption before
// code generation rather than letting a program fail at link time.
// [WASM-TARGET-EFFECTS]

#include <stdio.h>
#include <stdlib.h>
#include <stdbool.h>
#include <stdint.h>
#include <string.h>
#include <pthread.h>

#include <ucontext.h>

#include "effects_runtime.h"
#include "memory_hooks.h"
#include "osprey_thread.h"
#include "profiler_runtime.h"
#include "effects_scanned.h"
#include "memory_parked_wait.h"

#include <setjmp.h>

// Silence the deprecation ucontext carries on Apple targets: it is the one
// portable way to save and re-enter a machine context at an exact point, and
// the runtime is compiled with warnings as errors.
#if defined(__clang__)
#define OSP_UC_BEGIN _Pragma("clang diagnostic push") \
    _Pragma("clang diagnostic ignored \"-Wdeprecated-declarations\"")
#define OSP_UC_END _Pragma("clang diagnostic pop")
#else
#define OSP_UC_BEGIN
#define OSP_UC_END
#endif

// The current stack pointer a captured context resumes with. The mcontext
// spelling differs per architecture; a backend without a spelling cannot
// re-enter contexts and must fail the build rather than guess.
static uintptr_t context_sp(const ucontext_t *uc) {
#if defined(__x86_64__)
    return (uintptr_t)uc->uc_mcontext.gregs[REG_RSP];
#elif defined(__aarch64__)
    return (uintptr_t)uc->uc_mcontext.ss.__sp;
#else
#error "osprey reusable continuations need a stack-pointer spelling for this architecture"
#endif
}

static void capture_context(ucontext_t *uc) {
    OSP_UC_BEGIN
    (void)getcontext(uc);
    OSP_UC_END
}

static void reenter_context(const ucontext_t *uc) {
    OSP_UC_BEGIN
    (void)setcontext((ucontext_t *)(uintptr_t)uc);
    OSP_UC_END
}

// Hand the baton over and sleep [MEM-EFFECT-BATON].
static void baton_wait(pthread_cond_t *cond, pthread_mutex_t *lock) {
    (void)osp_parked_wait(cond, lock, NULL);
}

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
    // The continuation token of THIS suspension: the region's coro for a
    // single-shot operation, the reusable snapshot for a `many` one. An arm
    // resumes exactly the suspension it was dispatched for. Implements
    // [MULTI-HANDLE-MANY], [EFFECTS-CONTINUATION-OWNERSHIP].
    void *token;
} OpMailbox;

// One reusable continuation ([MULTI-HANDLE-MANY], plan 0016 phase 3): the
// frozen machine state of one `many` suspension — the register context, the
// stack bytes from the capture point up to the body thread's frame base, and
// the branch-local lifetime markers (pending finalizer count, owner-frame
// chain head). Restoring it means copying the bytes back to the SAME
// addresses and re-entering the captured context, so no pointer into the
// stack is ever translated: branches are exact, and work before the capture
// point never re-runs.
//
// The whole record is one collector-scanned allocation: the frozen frames and
// the saved register file both hold managed pointers that must stay rooted
// while no live stack does ([MEM-EFFECT-BATON]).
typedef struct OspreyCoro OspreyCoro;
typedef struct OspTemplate {
    OspreyCoro *coro;
    struct OspTemplate *parent; // next outer live suspension
    uint8_t *bytes;             // copy of [sp0, stack_top)
    size_t size;
    uintptr_t sp0;              // lowest captured stack address (== context sp)
    ucontext_t uc;              // must sit inside the scanned block
    int finalizer_count;        // pending finalizers at capture
    OspOwnerFrame *owner_head;  // owner-frame chain head at capture
    bool live;
    bool restored;              // a branch ran from this snapshot already
} OspTemplate;

typedef struct OspreyCoro {
    pthread_mutex_t lock;
    pthread_cond_t cond;
    pthread_t thread;
    bool started;
    bool joined;
    bool suspended;
    bool done;
    bool abandoned;
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
    // --- Reusable continuations -------------------------------------------
    // True once any `many` suspension ran: the body thread then parks in a
    // serve loop between branches instead of exiting at body completion, so
    // any live snapshot can still be resumed.
    bool many_region;
    bool branch_done;
    // Stack-address watermark of the body thread's entry frame: every snapshot
    // copies [capture sp, this), so the serve loop's own frames always sit
    // below every live snapshot's range.
    uintptr_t stack_top;
    // The innermost live `many` suspension.
    OspTemplate *templates;
    // Driver -> body commands. A command names its target: a resume targets
    // the snapshot the arm holds; an abandon targets the dispatched snapshot.
    int cmd; // OSP_CMD_*, body thread only clears under lock
    OspTemplate *cmd_target;
    int64_t cmd_value;
    // Set by free/abort when the whole region ends: release every snapshot and
    // end the thread.
    bool exit_requested;
    // Set by the restorer before re-entering a captured context: the
    // re-entered suspension returns the resume value instead of capturing.
    bool resuming;
} OspreyCoro;

#define OSP_CMD_NONE 0
#define OSP_CMD_RESUME 1
#define OSP_CMD_ABANDON 2
#define OSP_CMD_ABANDON_ALL 3

// A continuation token is either the region's coro (single-shot operations,
// and the raw pointers the runtime's own tests pass) or a `many` snapshot
// tagged in its low bit. Malloc guarantees the alignment the tag needs.
static void *template_token(OspTemplate *t) { return (void *)((uintptr_t)t | 1u); }
static bool token_is_template(void *raw) { return ((uintptr_t)raw & 1u) != 0; }
static OspTemplate *token_template(void *raw) {
    return (OspTemplate *)((uintptr_t)raw & ~(uintptr_t)1u);
}
static OspreyCoro *token_coro(void *raw) {
    return token_is_template(raw) ? token_template(raw)->coro : (OspreyCoro *)raw;
}

typedef struct CoroStartArgs {
    OspreyCoro *coro;
    int64_t (*body)(void *);
    void *body_env;
    HandlerSnapshot *snapshot;
} CoroStartArgs;

static void abandon_body(void);
void __osprey_coro_free(void *raw);

static void finish_wait(OspreyCoro *coro) {
    bool abandoned = coro->abandoned && coro->done;
    pthread_mutex_unlock(&coro->lock);
    if (abandoned) {
        pthread_cleanup_push(__osprey_coro_free, coro);
        abandon_body();
        pthread_cleanup_pop(1);
    }
}

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
// `token` is the continuation the dispatcher must resume for THIS suspension.
static OpMailbox *mailbox_new(int64_t op_id, const int64_t *args, const uint8_t *kinds,
                              int64_t count, void *token) {
    OpMailbox *mail = (OpMailbox *)checked_malloc(sizeof(OpMailbox), "effect operation mailbox");
    mail->op_id = op_id;
    mail->count = count > 0 ? count : 0;
    mail->words = NULL;
    mail->kinds = NULL;
    mail->token = token;
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
    coro->abandoned = true;
    coro->abort = false;
    coro->in_flight = false;
    coro->mail = NULL;
    coro->resume_value = 0;
    coro->result = 0;
    coro->region_env = env;
    coro->many_region = false;
    coro->branch_done = false;
    coro->stack_top = 0;
    coro->templates = NULL;
    coro->cmd = OSP_CMD_NONE;
    coro->cmd_target = NULL;
    coro->cmd_value = 0;
    coro->exit_requested = false;
    coro->resuming = false;
    return coro;
}

static void coro_thread_cleanup(void *raw) {
    CoroStartArgs *args = raw;
    __osprey_handler_stack_cleanup();
    // Before `done` wakes the host: an exiting stack must never block a collection.
    osp_mem_baton_thread_exit();
    osp_prof_thread_unregister();
    // Implements [EFFECTS-FINALIZATION]: abandonment must wake a nested
    // continuation's waiter so the same unwind reaches every ancestor.
    pthread_mutex_lock(&args->coro->lock);
    args->coro->done = true;
    args->coro->suspended = false;
    pthread_cond_broadcast(&args->coro->cond);
    pthread_mutex_unlock(&args->coro->lock);
    free(raw);
}

static void *__osprey_coro_thread(void *raw) {
    CoroStartArgs *args = (CoroStartArgs *)raw;
    OspreyCoro *coro = args->coro;
    int64_t result;
    // Abandonment exits from inside suspend, bypassing the normal body return.
    // Release its saved handler tails and startup state on both exit paths.
    pthread_cleanup_push(coro_thread_cleanup, args);
    // Effect continuations run on their own pthread; register so profiler
    // samples attribute to them distinctly [PROF-COLLECT-REGISTRY].
    osp_prof_thread_register(-1, "effect");
    osp_mem_baton_thread_enter();
    // The stack watermark every `many` snapshot copies up to: this frame sits
    // above anything the body can run, so a restored branch can never
    // overwrite the frames that serve later resumptions. [MULTI-HANDLE-MANY]
    uintptr_t entry_marker = 0;
    coro->stack_top = (uintptr_t)&entry_marker;
    if (args->snapshot != NULL) {
        __osprey_handler_restore(args->snapshot);
        args->snapshot = NULL;
    }
    result = args->body(args->body_env);
    pthread_mutex_lock(&coro->lock);
    coro->result = result;
    coro->abandoned = false;
    // A reusable-continuation region does not end when one branch completes:
    // the thread parks serving every live snapshot until the region's drive
    // unwinds and frees it. Implements [MULTI-HANDLE-MANY].
    bool serve = coro->many_region;
    if (serve) {
        coro->branch_done = true;
    }
    pthread_cond_broadcast(&coro->cond);
    pthread_mutex_unlock(&coro->lock);
    if (serve) {
        serve_templates(coro); // exits via pthread_exit: the pushed cleanup runs
    }
    pthread_cleanup_pop(1);
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
    osp_mem_baton_prepare();

    if (osp_thread_start(&coro->thread, __osprey_coro_thread, args, false) != 0) {
        free(args);
        fprintf(stderr, "FATAL: Failed to start effect continuation thread\n");
        abort();
    }
    pthread_mutex_lock(&coro->lock);
    coro->started = true;
    while (!coro->suspended && !coro->done) {
        baton_wait(&coro->cond, &coro->lock);
    }
    finish_wait(coro);
}

// The dropped continuation still owns its activations: finalize them inner to
// outer on this body thread, then end it. A finalizer that performs into the
// aborting region re-enters here and continues with the ones still pending.
// Implements [MULTI-COST-ABORT] and [EFFECTS-FINALIZATION].
static void abandon_body(void) {
    __osprey_finalizer_unwind();
    __osprey_owner_frames_unwind();
    pthread_exit(NULL);
}

// --- Reusable continuations ([MULTI-HANDLE-MANY], plan 0016 phase 3) --------

// One scanned block per snapshot: the frozen stack bytes follow the header so
// the collector roots the frames and the saved register file together.
static OspTemplate *template_alloc(OspreyCoro *coro) {
    OspTemplate *t = (OspTemplate *)osp_mem_scanned_alloc(sizeof(OspTemplate));
    if (t == NULL) {
        fprintf(stderr, "FATAL: Failed to allocate a reusable continuation\n");
        abort();
    }
    memset(t, 0, sizeof(OspTemplate));
    t->coro = coro;
    return t;
}

static void template_free(OspTemplate *t) {
    if (t == NULL) {
        return;
    }
    if (t->bytes != NULL) {
        osp_mem_scanned_free(t->bytes);
    }
    osp_mem_scanned_free(t);
}

// Where a captured stack address lives inside the snapshot's byte copy. Every
// address the snapshot records — owner-frame links, ARC slot pointers — must
// fall inside the captured range; one that does not can only be corruption,
// and guessing would misattribute ownership. [GC-ARC-PERCEUS]
static void *template_at(const OspTemplate *t, uintptr_t addr) {
    if (addr < t->sp0 || addr >= t->sp0 + (uintptr_t)t->size) {
        fprintf(stderr, "FATAL: continuation snapshot references %p outside its range\n",
                (void *)addr);
        abort();
    }
    return t->bytes + (size_t)(addr - t->sp0);
}

// Freeze the current suspension: the machine context, the stack bytes above
// it up to the thread's entry watermark, and the branch-local lifetime
// markers. Called with no lock held — everything it reads belongs to this
// thread. [MULTI-HANDLE-MANY]
static void template_capture(OspTemplate *t, uintptr_t stack_top) {
    capture_context(&t->uc);
    t->sp0 = context_sp(&t->uc);
    if (t->sp0 >= stack_top || stack_top - t->sp0 > (uintptr_t)OSPREY_THREAD_STACK_BYTES) {
        fprintf(stderr, "FATAL: continuation capture point %p outside its thread stack\n",
                (void *)t->sp0);
        abort();
    }
    t->size = (size_t)(stack_top - t->sp0);
    t->bytes = (uint8_t *)osp_mem_scanned_alloc(t->size);
    if (t->bytes == NULL) {
        fprintf(stderr, "FATAL: Failed to allocate continuation stack bytes\n");
        abort();
    }
    memcpy(t->bytes, (const void *)t->sp0, t->size);
    t->finalizer_count = __osprey_finalizer_count();
    t->owner_head = __osprey_owner_frames_head();
}

// Release the claims a snapshot's lifecycle still owes from its byte copy:
// the +1s its frozen frames held at capture, owed separately from the
// live-stack claims once a branch has run from it. [GC-ARC-PERCEUS]
static void template_release_claims(const OspTemplate *t) {
    if (t->owner_head == NULL) {
        return;
    }
    const OspOwnerFrame *frame =
        (const OspOwnerFrame *)template_at(t, (uintptr_t)t->owner_head);
    while (frame != NULL) {
        void ***slots = (void ***)template_at(t, (uintptr_t)frame->slots);
        for (int64_t i = 0; i < frame->count; i++) {
            void **slot = (void **)template_at(t, (uintptr_t)slots[i]);
            osp_release(*slot);
        }
        frame = frame->previous == NULL
                    ? NULL
                    : (const OspOwnerFrame *)template_at(t, (uintptr_t)frame->previous);
    }
}

// Kill one snapshot: run the finalizers its remainder still owed (down to the
// parent snapshot's pending count), release the owned slots of its resident
// frames, and release the captured claims it still owes. [MULTI-COST-ABORT]
static void kill_template(OspreyCoro *coro, OspTemplate *t) {
    if (!t->live) {
        return;
    }
    int parent_finalizers = t->parent != NULL ? t->parent->finalizer_count : 0;
    OspOwnerFrame *parent_head = t->parent != NULL ? t->parent->owner_head : NULL;
    __osprey_finalizer_unwind_to(parent_finalizers);
    __osprey_owner_frames_release_to(parent_head);
    if (t->restored) {
        template_release_claims(t);
    }
    pthread_mutex_lock(&coro->lock);
    t->live = false;
    if (coro->templates == t) {
        coro->templates = t->parent;
    } else {
        OspTemplate *at = coro->templates;
        while (at != NULL && at->parent != t) {
            at = at->parent;
        }
        if (at != NULL) {
            at->parent = t->parent;
        }
    }
    coro->in_flight = false;
    pthread_cond_broadcast(&coro->cond);
    pthread_mutex_unlock(&coro->lock);
    template_free(t);
}

// Kill every live snapshot — the whole running remainder is being abandoned.
static void kill_all_templates(OspreyCoro *coro) {
    __osprey_finalizer_unwind_to(0);
    __osprey_owner_frames_release_to(NULL);
    pthread_mutex_lock(&coro->lock);
    OspTemplate *t = coro->templates;
    coro->templates = NULL;
    coro->in_flight = false;
    pthread_cond_broadcast(&coro->cond);
    pthread_mutex_unlock(&coro->lock);
    while (t != NULL) {
        OspTemplate *parent = t->parent;
        if (t->restored) {
            template_release_claims(t);
        }
        t->live = false;
        template_free(t);
        t = parent;
    }
}

// Kill every snapshot deeper than `target` — a resume of an outer suspension
// supersedes the branches its own branch spawned. [EFFECTS-RESUME-NESTING]
static void kill_deeper(OspreyCoro *coro, OspTemplate *target) {
    while (coro->templates != NULL && coro->templates != target) {
        kill_template(coro, coro->templates);
    }
    if (coro->templates != target) {
        fprintf(stderr, "FATAL: continuation resume does not name a live suspension\n");
        abort();
    }
}

// Restore one snapshot and re-enter its captured context: the branch IS the
// captured remainder, continued with `value` as the operation's result. Never
// returns. [MULTI-HANDLE-MANY]
static void restore_template(OspreyCoro *coro, OspTemplate *t, int64_t value) {
    // Copy the bytes back to the SAME addresses: every pointer into the stack
    // — frame links, owner-frame chains, ARC slots — lands exactly where it
    // was captured, so nothing is translated and work before the capture
    // point cannot re-run.
    memcpy((void *)t->sp0, t->bytes, t->size);
    // The branch re-claims the snapshot's owned slots (+1 each): its epilogues
    // release them again, and the snapshot's own death releases the captured
    // claim — balanced per branch and per lifecycle. [GC-ARC-PERCEUS]
    __osprey_owner_frames_retain_from(t->owner_head);
    __osprey_finalizer_truncate(t->finalizer_count);
    __osprey_owner_frames_set(t->owner_head);
    t->restored = true;
    pthread_mutex_lock(&coro->lock);
    coro->resume_value = value;
    coro->resuming = true;
    coro->suspended = false;
    coro->in_flight = false;
    coro->branch_done = false;
    pthread_cond_broadcast(&coro->cond);
    pthread_mutex_unlock(&coro->lock);
    reenter_context(&t->uc);
}

// Release every snapshot and end the thread; the pushed cleanup wakes the
// driver. Balanced paths leave nothing pending — the unwinds are the defense.
static void exit_serving_thread(OspreyCoro *coro) {
    kill_all_templates(coro);
    __osprey_finalizer_unwind();
    __osprey_owner_frames_unwind();
    pthread_exit(NULL);
}

// Between branches, the body thread serves whichever suspension the driver
// answers next: a resume restores its snapshot (never returns), an abandon
// kills just that snapshot, and the region's end releases everything and ends
// the thread. [MULTI-HANDLE-MANY] [EFFECTS-RESUME]
static void serve_templates(OspreyCoro *coro) {
    for (;;) {
        pthread_mutex_lock(&coro->lock);
        while (coro->cmd == OSP_CMD_NONE && !coro->abort && !coro->exit_requested) {
            baton_wait(&coro->cond, &coro->lock);
        }
        int cmd = coro->cmd;
        OspTemplate *target = coro->cmd_target;
        int64_t value = coro->cmd_value;
        coro->cmd = OSP_CMD_NONE;
        coro->cmd_target = NULL;
        bool ending = coro->exit_requested || coro->abort;
        pthread_mutex_unlock(&coro->lock);
        if (ending) {
            exit_serving_thread(coro);
        }
        if (cmd == OSP_CMD_RESUME) {
            if (target == NULL || !target->live) {
                fprintf(stderr, "FATAL: continuation resume names a dead suspension\n");
                abort();
            }
            kill_deeper(coro, target);
            restore_template(coro, target, value); // never returns
        }
        if (cmd == OSP_CMD_ABANDON) {
            if (target != NULL) {
                kill_template(coro, target);
            }
        } else if (cmd == OSP_CMD_ABANDON_ALL) {
            kill_all_templates(coro);
        }
    }
}

// The running remainder is being abandoned. A reusable-continuation region
// keeps its thread alive for the snapshots that outlive the branch; a
// single-shot region ends with its stack. [MULTI-COST-ABORT]
static void abandon_serving(OspreyCoro *coro) {
    if (coro->many_region) {
        kill_all_templates(coro);
        serve_templates(coro); // never returns
    }
    abandon_body();
}

// Suspend a `many` operation: capture the reusable snapshot, publish the
// mailbox, and serve resumptions. The first resume returns directly (the
// frames below are intact); every later one re-enters through
// `restore_template`, which lands right after `capture_context` with
// `coro->resuming` set — the exact point the branch last suspended at.
// Implements [MULTI-HANDLE-MANY], [MULTI-COST].
int64_t __osprey_coro_suspend_many(void *raw, int64_t op_id, const int64_t *args,
                                   const uint8_t *kinds, int64_t arg_count) {
    OspreyCoro *coro = (OspreyCoro *)raw;
    if (coro == NULL) {
        release_operands(args, kinds, arg_count);
        return 0;
    }
    pthread_mutex_lock(&coro->lock);
    while (coro->in_flight && !coro->abort && !coro->exit_requested) {
        baton_wait(&coro->cond, &coro->lock);
    }
    if (coro->abort || coro->exit_requested) {
        pthread_mutex_unlock(&coro->lock);
        release_operands(args, kinds, arg_count);
        abandon_serving(coro);
    }
    pthread_mutex_unlock(&coro->lock);
    OspTemplate *t = template_alloc(coro);
    capture_context(&t->uc);
    if (coro->resuming) {
        // A restored branch re-entered the captured context: continue the
        // suspension's caller with the resume value. No lock is held — the
        // restorer released it before re-entering.
        coro->resuming = false;
        return coro->resume_value;
    }
    template_capture(t, coro->stack_top);
    pthread_mutex_lock(&coro->lock);
    coro->many_region = true;
    t->parent = coro->templates;
    t->live = true;
    coro->templates = t;
    coro->in_flight = true;
    coro->mail = mailbox_new(op_id, args, kinds, arg_count, template_token(t));
    coro->suspended = true;
    pthread_cond_broadcast(&coro->cond);
    for (;;) {
        while (coro->cmd == OSP_CMD_NONE && !coro->abort && !coro->exit_requested) {
            baton_wait(&coro->cond, &coro->lock);
        }
        if (coro->abort || coro->exit_requested) {
            pthread_mutex_unlock(&coro->lock);
            abandon_serving(coro);
        }
        if (coro->cmd == OSP_CMD_RESUME && coro->cmd_target == t) {
            int64_t value = coro->cmd_value;
            coro->cmd = OSP_CMD_NONE;
            coro->cmd_target = NULL;
            coro->suspended = false;
            coro->in_flight = false;
            pthread_cond_broadcast(&coro->cond);
            pthread_mutex_unlock(&coro->lock);
            return value;
        }
        if (coro->cmd == OSP_CMD_ABANDON && coro->cmd_target == t) {
            coro->cmd = OSP_CMD_NONE;
            coro->cmd_target = NULL;
            pthread_mutex_unlock(&coro->lock);
            kill_template(coro, t);
            serve_templates(coro); // never returns
        }
        // A command for another suspension: the shared server kills this one
        // as deeper-than-target and restores the target. Never returns.
        pthread_mutex_unlock(&coro->lock);
        serve_templates(coro);
    }
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
    while (coro->in_flight && !coro->abort && !coro->exit_requested) {
        baton_wait(&coro->cond, &coro->lock);
    }
    if (coro->abort || coro->exit_requested) {
        pthread_mutex_unlock(&coro->lock);
        // Killed while queued behind another perform: no mailbox was built, so
        // nothing downstream will ever release these operands. Done outside the
        // lock — the memory backend must not be entered holding it.
        release_operands(args, kinds, arg_count);
        abandon_serving(coro);
    }
    coro->in_flight = true;
    coro->mail = mailbox_new(op_id, args, kinds, arg_count, raw);
    coro->suspended = true;
    pthread_cond_broadcast(&coro->cond);
    while (coro->suspended && !coro->abort && !coro->exit_requested &&
           coro->cmd != OSP_CMD_ABANDON_ALL) {
        baton_wait(&coro->cond, &coro->lock);
    }
    if (coro->abort || coro->exit_requested || coro->cmd == OSP_CMD_ABANDON_ALL) {
        pthread_mutex_unlock(&coro->lock);
        // The handoff already happened here, so the operands are NOT this
        // thread's to release: either the dispatcher took the mailbox and
        // retires it with __osprey_coro_mail_free, or it is still in
        // `coro->mail` and __osprey_coro_free retires it. Releasing again would
        // be a double free, not a leak fix.
        abandon_body();
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
// Accepts the region's continuation token or the coro itself.
void *__osprey_coro_take_args(void *raw) {
    if (raw == NULL) {
        return NULL;
    }
    OspreyCoro *coro = token_coro(raw);
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

// The continuation token of the suspension this mailbox belongs to: the coro
// for a single-shot operation, the reusable snapshot for a `many` one. An arm
// resumes exactly the suspension it was dispatched for.
void *__osprey_coro_mail_token(void *raw) {
    if (raw == NULL) {
        return NULL;
    }
    return ((OpMailbox *)raw)->token;
}

int64_t __osprey_coro_resume(void *raw, int64_t value) {
    if (raw == NULL) {
        return 0;
    }
    if (token_is_template(raw)) {
        // A `many` resumption ([MULTI-HANDLE-MANY]): hand the value to the
        // body thread — the first answer returns from the suspension itself,
        // a later one restores the snapshot — and wait for that branch to
        // suspend again or complete, exactly like the single-shot round trip.
        OspTemplate *t = token_template(raw);
        OspreyCoro *coro = t->coro;
        pthread_mutex_lock(&coro->lock);
        if (!t->live) {
            pthread_mutex_unlock(&coro->lock);
            fprintf(stderr, "fatal: continuation already abandoned\n");
            exit(1);
        }
        coro->cmd = OSP_CMD_RESUME;
        coro->cmd_target = t;
        coro->cmd_value = value;
        coro->branch_done = false;
        pthread_cond_broadcast(&coro->cond);
        while (!coro->suspended && !coro->branch_done && !coro->done) {
            baton_wait(&coro->cond, &coro->lock);
        }
        int64_t result = (coro->branch_done || coro->done) ? coro->result : 0;
        pthread_mutex_unlock(&coro->lock);
        return result;
    }
    OspreyCoro *coro = (OspreyCoro *)raw;
    pthread_mutex_lock(&coro->lock);
    // Single-shot rejection [EFFECTS-RESUME]: the thread-as-continuation model
    // is one suspended stack — a consumed (completed) pthread stack cannot be
    // re-run. A second `resume` on an already-finished continuation would
    // silently return the stale first result (a wrong answer with exit 0), so
    // reject it loudly instead. Legitimate re-entry (the body performed again)
    // leaves the coro suspended, not done, and never reaches this guard.
    if (coro->done) {
        pthread_mutex_unlock(&coro->lock);
        fprintf(stderr,
                "fatal: continuation already resumed "
                "(single-shot continuation reused)\n");
        exit(1);
    }
    coro->resume_value = value;
    coro->suspended = false;
    pthread_cond_broadcast(&coro->cond);
    while (!coro->suspended && !coro->branch_done && !coro->done) {
        baton_wait(&coro->cond, &coro->lock);
    }
    int64_t result = (coro->branch_done || coro->done) ? coro->result : 0;
    finish_wait(coro);
    return result;
}

int64_t __osprey_coro_done(void *raw) {
    if (raw == NULL) {
        return 1;
    }
    OspreyCoro *coro = token_coro(raw);
    pthread_mutex_lock(&coro->lock);
    // `branch_done` is the multi-shot region's per-branch completion: the
    // computation finished THIS branch, and the region still serves any live
    // snapshot. [MULTI-HANDLE-MANY]
    int64_t done = (coro->done || coro->branch_done) ? 1 : 0;
    pthread_mutex_unlock(&coro->lock);
    return done;
}

int64_t __osprey_coro_result(void *raw) {
    if (raw == NULL) {
        return 0;
    }
    OspreyCoro *coro = token_coro(raw);
    pthread_mutex_lock(&coro->lock);
    int64_t result = coro->result;
    pthread_mutex_unlock(&coro->lock);
    return result;
}

void __osprey_coro_abort(void *raw) {
    if (raw == NULL) {
        return;
    }
    if (token_is_template(raw)) {
        // Abandoning one `many` suspension kills its snapshot (and the branches
        // it still owed) while the body thread stays available to every other
        // live resumption. [MULTI-COST-ABORT]
        OspTemplate *t = token_template(raw);
        OspreyCoro *coro = t->coro;
        pthread_mutex_lock(&coro->lock);
        if (t->live) {
            coro->cmd = OSP_CMD_ABANDON;
            coro->cmd_target = t;
            pthread_cond_broadcast(&coro->cond);
        }
        pthread_mutex_unlock(&coro->lock);
        return;
    }
    OspreyCoro *coro = (OspreyCoro *)raw;
    pthread_mutex_lock(&coro->lock);
    if (coro->many_region && coro->started && !coro->joined && !coro->done) {
        // A single-shot operation declined inside a reusable-continuation
        // region: the whole running remainder dies, but outer snapshots may
        // still be resumable, so the thread serves on instead of ending.
        coro->cmd = OSP_CMD_ABANDON_ALL;
        pthread_cond_broadcast(&coro->cond);
        pthread_mutex_unlock(&coro->lock);
        return;
    }
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
    if (raw == NULL) {
        return;
    }
    OspreyCoro *coro = token_coro(raw);
    if (coro->started && !coro->joined) {
        pthread_mutex_lock(&coro->lock);
        if (!coro->done) {
            if (coro->many_region) {
                // The region ends: release every snapshot and let the thread
                // exit through its serve loop. [MULTI-HANDLE-MANY]
                coro->exit_requested = true;
            } else {
                coro->abort = true;
            }
            coro->suspended = false;
            pthread_cond_broadcast(&coro->cond);
        }
        pthread_mutex_unlock(&coro->lock);
        pthread_join(coro->thread, NULL);
        coro->joined = true;
    }
    if (coro->templates != NULL) {
        // The serve loop releases every snapshot before the thread exits;
        // survivors here would be leaked claims.
        fprintf(stderr, "FATAL: effect continuation freed with live snapshots\n");
        abort();
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

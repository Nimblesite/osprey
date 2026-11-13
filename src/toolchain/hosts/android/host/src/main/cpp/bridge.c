// The JNI surface of the one Android host every Osprey Native application
// runs on. It owns the shared-frame bridge, calls the application's exports
// on the one host thread, and hands Kotlin the four sealed runs as direct
// buffers over the leased bytes, releasing every lease once Kotlin has read
// the frame. Implements [NATIVE-SHM-OWNERSHIP], [NATIVE-SHM-PUBLISH] and
// [NATIVE-SHM-EVENT]; the row checks live in Rows.kt, as they do in the
// SwiftUI host's SharedRows.swift.
#include <jni.h>
#include <pthread.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "app.h"
#include "native_abi.h"
#include "native_bridge.h"

#define RUNS 4
#define BRIDGE_BYTES (512LL << 20)
#define BRIDGE_CHUNKS 1024
#define BRIDGE_OWNERS 4096

// One host session: the bridge and the child leases of the frame it holds.
typedef struct {
    OspNativeBridge *bridge;
    td_OwnedLease runs[RUNS];
    bool holding;
} Session;

// One host-held slice ([NATIVE-SHM-ASK]): its key, its text and the pinned
// copy the application's import borrows. Superseded pins are never returned
// to the allocator, so a call still reading one stays safe.
typedef struct Slice {
    char *key;
    char *text;
    char *pinned;
    struct Slice *next;
} Slice;

static pthread_mutex_t slice_lock = PTHREAD_MUTEX_INITIALIZER;
static Slice *slices = NULL;
static char no_text[1] = {0};

// Throws the failure of `call`; the caller returns at once, because no JNI
// call may follow a pending exception.
static void fail(JNIEnv *env, const char *call, int status) {
    char message[160];
    snprintf(message, sizeof message, "%s returned %d", call, status);
    jclass kind = (*env)->FindClass(env, "java/lang/IllegalStateException");
    if (kind != NULL)
        (*env)->ThrowNew(env, kind, message);
}

// NUL-terminated UTF-8 copy of a Java byte array, or NULL once it threw: an
// array that cannot be copied, or text holding a NUL the exports would cut at.
static char *bytes(JNIEnv *env, jbyteArray array) {
    jsize size = array == NULL ? 0 : (*env)->GetArrayLength(env, array);
    char *value = malloc((size_t)size + 1);
    if (value == NULL) {
        fail(env, "allocate input", 0);
        return NULL;
    }
    if (size > 0)
        (*env)->GetByteArrayRegion(env, array, 0, size, (jbyte *)value);
    if ((*env)->ExceptionCheck(env)) {
        free(value);
        return NULL;
    }
    if (memchr(value, 0, (size_t)size) != NULL) {
        free(value);
        fail(env, "input holds an embedded NUL", 0);
        return NULL;
    }
    value[size] = 0;
    return value;
}

// Releases the first `count` run leases the session holds. A release that
// fails leaves that storage pinned, which is safe; it is never retried.
static void release_runs(Session *session, int count) {
    for (int run = 0; run < count; run++)
        (void)osp_native_release(session->bridge, &session->runs[run]);
    session->holding = false;
}

// One run of the frame just committed: its child lease and its bytes
// borrowed in place. A lease taken before a failure is released here.
static int take_run(Session *session, td_OwnedLease *root, int run,
                    int64_t *length, const void **pointer) {
    td_OwnedLease *lease = &session->runs[run];
    int status = osp_native_child(session->bridge, root, run, lease);
    if (status != OSP_NATIVE_OK)
        return status;
    status = osp_native_length(session->bridge, lease, length);
    if (status == OSP_NATIVE_OK)
        status = osp_native_view(session->bridge, lease, run + 1, 0, *length, pointer);
    if (status != OSP_NATIVE_OK)
        (void)osp_native_release(session->bridge, lease);
    return status;
}

// Leases the four runs of the root just acquired into the session; a run
// that cannot be leased releases every run taken before it.
static int take_runs(Session *session, td_OwnedLease *root,
                     int64_t length[RUNS], const void *pointer[RUNS]) {
    for (int run = 0; run < RUNS; run++) {
        length[run] = 0;
        pointer[run] = no_text;
        int status = take_run(session, root, run, &length[run], &pointer[run]);
        if (status != OSP_NATIVE_OK) {
            release_runs(session, run);
            return status;
        }
    }
    session->holding = true;
    return OSP_NATIVE_OK;
}

// The held runs as direct buffers over the leased bytes, or NULL with an
// exception pending and every lease released.
static jobjectArray buffers_of(JNIEnv *env, Session *session,
                               const int64_t length[RUNS], const void *pointer[RUNS]) {
    jclass kind = (*env)->FindClass(env, "java/nio/ByteBuffer");
    jobjectArray out = kind == NULL ? NULL : (*env)->NewObjectArray(env, RUNS, kind, NULL);
    for (int run = 0; out != NULL && run < RUNS; run++) {
        jobject buffer = (*env)->NewDirectByteBuffer(env, (void *)pointer[run],
                                                     length[run] < 0 ? 0 : (jlong)length[run]);
        if (buffer != NULL)
            (*env)->SetObjectArrayElement(env, out, run, buffer);
        if (buffer == NULL || (*env)->ExceptionCheck(env))
            out = NULL;
    }
    if (out == NULL)
        release_runs(session, RUNS);
    return out;
}

// Commits the frame the export just wrote and leases its four runs. The
// frame held before it is released first: Kotlin copied its rows already.
static jobjectArray commit(JNIEnv *env, Session *session, const char *call) {
    int64_t revision = 0;
    int status = osp_native_frame_commit(session->bridge, &revision);
    if (status != OSP_NATIVE_OK) {
        fail(env, call, status);
        return NULL;
    }
    if (session->holding)
        release_runs(session, RUNS);
    td_OwnedLease root;
    status = osp_native_acquire(session->bridge, &root, &revision);
    if (status != OSP_NATIVE_OK) {
        fail(env, "acquire", status);
        return NULL;
    }
    int64_t length[RUNS];
    const void *pointer[RUNS];
    status = take_runs(session, &root, length, pointer);
    (void)osp_native_release(session->bridge, &root);
    if (status != OSP_NATIVE_OK) {
        fail(env, "lease a run", status);
        return NULL;
    }
    return buffers_of(env, session, length, pointer);
}

// The ownership ABI this host speaks; false with an exception pending for any other.
static bool abi(JNIEnv *env) {
    int32_t version = osprey_host_abi_version();
    if (version != 1)
        fail(env, "osprey_host_abi_version", version);
    return version == 1;
}

JNIEXPORT jlong JNICALL Java_org_ospreylang_host_Bridge_openBridge(JNIEnv *env, jobject self) {
    (void)self;
    // The instance is random, so a stale pointer from another launch refuses.
    td_ApplicationInstance instance;
    arc4random_buf(&instance, sizeof instance);
    Session *session = calloc(1, sizeof *session);
    if (session == NULL) {
        fail(env, "allocate session", 0);
        return 0;
    }
    int status = osp_native_open(&instance, TD_SHARED_FINGERPRINT, BRIDGE_BYTES,
                                 BRIDGE_CHUNKS, BRIDGE_OWNERS, &session->bridge);
    if (status != OSP_NATIVE_OK) {
        free(session);
        fail(env, "open", status);
        return 0;
    }
    return (jlong)(intptr_t)session;
}

// Runs the application's top level; its rows belong to no frame, so they are dropped.
JNIEXPORT void JNICALL Java_org_ospreylang_host_Bridge_mainEntry(JNIEnv *env, jobject self) {
    (void)self;
    if (!abi(env))
        return;
    int32_t status = osprey_main();
    osp_native_frame_discard();
    if (status != 0)
        fail(env, "osprey_main", status);
}

// One launch argument, in order, before `open`. Its frame is committed and
// returned for its asks, as every export's is.
JNIEXPORT jobjectArray JNICALL Java_org_ospreylang_host_Bridge_argument(JNIEnv *env, jobject self, jlong handle, jbyteArray text) {
    (void)self;
    char *words = abi(env) ? bytes(env, text) : NULL;
    if (words == NULL)
        return NULL;
    (void)osprey_app_argument(words);
    free(words);
    return commit(env, (Session *)(intptr_t)handle, "argument: frame commit");
}

JNIEXPORT jobjectArray JNICALL Java_org_ospreylang_host_Bridge_openApp(JNIEnv *env, jobject self, jlong handle,
                                                                      jlong now, jlong offset, jbyteArray platform,
                                                                      jbyteArray home, jboolean dark) {
    (void)self;
    char *platform_words = abi(env) ? bytes(env, platform) : NULL;
    char *home_words = platform_words != NULL ? bytes(env, home) : NULL;
    if (home_words != NULL)
        (void)osprey_app_open(now, offset, platform_words, home_words, dark);
    free(platform_words);
    free(home_words);
    return home_words == NULL ? NULL : commit(env, (Session *)(intptr_t)handle, "open: frame commit");
}

JNIEXPORT jobjectArray JNICALL Java_org_ospreylang_host_Bridge_event(JNIEnv *env, jobject self, jlong handle,
                                                                    jlong now, jlong kind, jlong ordinal,
                                                                    jbyteArray words, jbyteArray detail,
                                                                    jdouble x, jdouble y, jboolean down, jboolean shift) {
    (void)self;
    char *word_text = abi(env) ? bytes(env, words) : NULL;
    char *detail_text = word_text != NULL ? bytes(env, detail) : NULL;
    if (detail_text != NULL)
        (void)osprey_app_event(now, kind, ordinal, word_text, detail_text, x, y, down, shift);
    free(word_text);
    free(detail_text);
    return detail_text == NULL ? NULL : commit(env, (Session *)(intptr_t)handle, "event: frame commit");
}

JNIEXPORT void JNICALL Java_org_ospreylang_host_Bridge_releaseFrame(JNIEnv *env, jobject self, jlong handle) {
    (void)env;
    (void)self;
    Session *session = (Session *)(intptr_t)handle;
    if (session != NULL && session->holding)
        release_runs(session, RUNS);
}

JNIEXPORT void JNICALL Java_org_ospreylang_host_Bridge_closeBridge(JNIEnv *env, jobject self, jlong handle) {
    (void)env;
    (void)self;
    Session *session = (Session *)(intptr_t)handle;
    if (session == NULL)
        return;
    if (session->holding)
        release_runs(session, RUNS);
    (void)osp_native_close(session->bridge);
    (void)osp_native_destroy(session->bridge);
    free(session);
}

// The slice held under `key`, or NULL. Call with the lock held.
static Slice *slice_named(const char *key) {
    for (Slice *slice = slices; slice != NULL; slice = slice->next)
        if (strcmp(slice->key, key) == 0)
            return slice;
    return NULL;
}

// Answers the application's slice import with the host-held slice `key`.
// The key is borrowed for the call; a missing key answers empty, never NULL.
// A reader borrows a pinned copy made on its first borrow after a write; a
// superseded pin is never returned to the allocator, so a call still reading
// it stays safe ([NATIVE-SHM-ASK]).
const char *native_slice(const char *key) {
    const char *answer = no_text;
    pthread_mutex_lock(&slice_lock);
    Slice *slice = key == NULL ? NULL : slice_named(key);
    if (slice != NULL && slice->pinned == NULL)
        slice->pinned = strdup(slice->text);
    if (slice != NULL && slice->pinned != NULL)
        answer = slice->pinned;
    pthread_mutex_unlock(&slice_lock);
    return answer;
}

// Keeps `body` under `key`, taking ownership of both; false when out of memory.
static bool slice_keep(char *key, char *body) {
    Slice *slice = slice_named(key);
    if (slice == NULL) {
        slice = calloc(1, sizeof *slice);
        if (slice == NULL)
            return false;
        slice->key = key;
        slice->next = slices;
        slices = slice;
    } else {
        free(key);
        free(slice->text);
        // The pin a reader may still hold is abandoned, never freed.
        slice->pinned = NULL;
    }
    slice->text = body;
    return true;
}

JNIEXPORT void JNICALL Java_org_ospreylang_host_Bridge_sliceWrite(JNIEnv *env, jobject self, jbyteArray key, jbyteArray text) {
    (void)self;
    char *key_text = bytes(env, key);
    char *body = key_text != NULL ? bytes(env, text) : NULL;
    if (body == NULL) {
        free(key_text);
        return;
    }
    pthread_mutex_lock(&slice_lock);
    bool kept = slice_keep(key_text, body);
    pthread_mutex_unlock(&slice_lock);
    if (!kept) {
        free(key_text);
        free(body);
        fail(env, "allocate slice", 0);
    }
}

JNIEXPORT jbyteArray JNICALL Java_org_ospreylang_host_Bridge_sliceText(JNIEnv *env, jobject self, jbyteArray key) {
    (void)self;
    char *key_text = bytes(env, key);
    if (key_text == NULL)
        return NULL;
    pthread_mutex_lock(&slice_lock);
    Slice *slice = slice_named(key_text);
    char *copy = strdup(slice != NULL ? slice->text : no_text);
    pthread_mutex_unlock(&slice_lock);
    free(key_text);
    if (copy == NULL) {
        fail(env, "allocate slice text", 0);
        return NULL;
    }
    jsize size = (jsize)strlen(copy);
    jbyteArray out = (*env)->NewByteArray(env, size);
    if (out != NULL)
        (*env)->SetByteArrayRegion(env, out, 0, size, (const jbyte *)copy);
    free(copy);
    return out;
}

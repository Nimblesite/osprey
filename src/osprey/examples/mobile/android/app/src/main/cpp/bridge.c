// Thin UTF-8 byte bridge; every Osprey call is serialized by the host main loop.
// Implements [ANDROID-HOST-ABI] and [MOBILE-REACTIVE-UI].
#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "inbox.h"

static void fail(JNIEnv *env, const char *message) {
    jclass kind = (*env)->FindClass(env, "java/lang/IllegalStateException");
    if (kind != NULL) (void)(*env)->ThrowNew(env, kind, message);
}

static jbyteArray result(JNIEnv *env, const char *value) {
    if (value == NULL) { fail(env, "Osprey returned a null envelope"); return NULL; }
    size_t size = strlen(value);
    if (size > INT32_MAX) { fail(env, "Osprey envelope exceeds the JNI array limit"); return NULL; }
    jbyteArray bytes = (*env)->NewByteArray(env, (jsize)size);
    if (bytes != NULL) (*env)->SetByteArrayRegion(env, bytes, 0, (jsize)size, (const jbyte *)value);
    return bytes;
}

static char *utf8(JNIEnv *env, jbyteArray bytes) {
    jsize size = (*env)->GetArrayLength(env, bytes);
    char *value = malloc((size_t)size + 1);
    if (value == NULL) { fail(env, "Cannot allocate Osprey input"); return NULL; }
    (*env)->GetByteArrayRegion(env, bytes, 0, size, (jbyte *)value);
    if ((*env)->ExceptionCheck(env)) { free(value); return NULL; }
    if (memchr(value, 0, (size_t)size) != NULL) { free(value); fail(env, "Osprey input contains an embedded NUL"); return NULL; }
    value[size] = 0;
    return value;
}

JNIEXPORT jbyteArray JNICALL Java_org_ospreylang_issueinbox_Native_start(JNIEnv *env, jobject self) {
    (void)self;
    if (osprey_main() != 0) { fail(env, "Osprey library initialization failed"); return NULL; }
    return result(env, osprey_mobile_start());
}

JNIEXPORT jbyteArray JNICALL Java_org_ospreylang_issueinbox_Native_dispatch(JNIEnv *env, jobject self, jbyteArray model, jbyteArray event) {
    (void)self;
    char *model_utf8 = utf8(env, model);
    if (model_utf8 == NULL) return NULL;
    char *event_utf8 = utf8(env, event);
    if (event_utf8 == NULL) { free(model_utf8); return NULL; }
    jbyteArray output = result(env, osprey_mobile_dispatch(model_utf8, event_utf8));
    free(event_utf8);
    free(model_utf8);
    return output;
}

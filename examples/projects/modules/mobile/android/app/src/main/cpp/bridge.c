#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "talon.h"

// Kotlin serializes runtime calls, including initialization, on the UI thread.
static int initialized;
static void fail(JNIEnv *env, const char *message) {
    jclass type = (*env)->FindClass(env, "java/lang/IllegalStateException");
    if (type != NULL) (void)(*env)->ThrowNew(env, type, message);
}
static jbyteArray bytes(JNIEnv *env, const char *value) {
    if (value == NULL) { fail(env, "Osprey returned a null envelope"); return NULL; }
    size_t length = strlen(value);
    if (length > INT32_MAX) { fail(env, "Osprey envelope too large"); return NULL; }
    jbyteArray result = (*env)->NewByteArray(env, (jsize)length);
    if (result != NULL) (*env)->SetByteArrayRegion(env, result, 0, (jsize)length, (const jbyte *)value);
    return result;
}
JNIEXPORT jbyteArray JNICALL Java_org_ospreylang_talon_Native_startBytes(JNIEnv *env, jobject self) {
    (void)self;
    if (!initialized) {
        if (osprey_main() != 0) { fail(env, "Osprey initialization failed"); return NULL; }
        initialized = 1;
    }
    return bytes(env, osprey_talonmobile_start());
}
JNIEXPORT jbyteArray JNICALL Java_org_ospreylang_talon_Native_dispatchBytes(JNIEnv *env, jobject self, jbyteArray payload) {
    (void)self;
    if (!initialized) {
        if (osprey_main() != 0) { fail(env, "Osprey initialization failed"); return NULL; }
        initialized = 1;
    }
    jsize size = (*env)->GetArrayLength(env, payload);
    char *input = malloc((size_t)size + 1);
    if (input == NULL) { fail(env, "Cannot allocate Osprey input"); return NULL; }
    (*env)->GetByteArrayRegion(env, payload, 0, size, (jbyte *)input);
    if ((*env)->ExceptionCheck(env)) { free(input); return NULL; }
    if (memchr(input, 0, (size_t)size) != NULL) { free(input); fail(env, "Embedded NUL in payload"); return NULL; }
    input[size] = 0;
    jbyteArray result = bytes(env, osprey_talonmobile_dispatch(input));
    free(input);
    return result;
}

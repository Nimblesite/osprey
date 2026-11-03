#include <jni.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>
#include "libOspreyApp.h"

// [ANDROID-HOST-ABI] Every resident call is serialized on Android's UI thread.
static JNIEnv *current;
static jclass native_class;
static jclass tls_class;
static char *borrowed;

static void fail(JNIEnv *env, const char *message) {
  jclass kind = (*env)->FindClass(env, "java/lang/IllegalStateException");
  if (kind) (void)(*env)->ThrowNew(env, kind, message);
}

static char *bytes(JNIEnv *env, jbyteArray input) {
  if (!input) { fail(env, "Missing UTF-8 input"); return NULL; }
  jsize length = (*env)->GetArrayLength(env, input);
  char *text = malloc((size_t)length + 1);
  if (!text) { fail(env, "Cannot allocate UTF-8 input"); return NULL; }
  (*env)->GetByteArrayRegion(env, input, 0, length, (jbyte *)text);
  if ((*env)->ExceptionCheck(env) || memchr(text, 0, (size_t)length)) {
    free(text); fail(env, "Invalid UTF-8 input"); return NULL;
  }
  text[length] = 0;
  return text;
}

static jbyteArray result(JNIEnv *env, const char *text) {
  if (!text) { fail(env, "Osprey returned no envelope"); return NULL; }
  size_t length = strlen(text);
  if (length > INT32_MAX) { osprey_release_string(text); fail(env, "Envelope exceeds JNI limit"); return NULL; }
  jbyteArray output = (*env)->NewByteArray(env, (jsize)length);
  if (output) (*env)->SetByteArrayRegion(env, output, 0, (jsize)length, (const jbyte *)text);
  osprey_release_string(text);
  return output;
}

static bool prepare(JNIEnv *env, jclass owner) {
  current = env;
  if (!native_class) native_class = (*env)->NewGlobalRef(env, owner);
  if (!tls_class) {
    jclass found = (*env)->FindClass(env, "org/ospreylang/casting/CastTLS");
    if (found) { tls_class = (*env)->NewGlobalRef(env, found); (*env)->DeleteLocalRef(env, found); }
  }
  return native_class && tls_class && !(*env)->ExceptionCheck(env);
}

JNIEXPORT jbyteArray JNICALL Java_org_ospreylang_casting_Native_start(JNIEnv *env, jclass owner, jbyteArray context) {
  if (!prepare(env, owner)) return NULL;
  if (osprey_host_abi_version() != 1 || osprey_main() != 0) { fail(env, "Cannot initialize Osprey ABI"); return NULL; }
  char *source = bytes(env, context);
  if (!source) return NULL;
  const char *text = osprey_app_start(source);
  free(source);
  return result(env, text);
}

JNIEXPORT jbyteArray JNICALL Java_org_ospreylang_casting_Native_dispatch(JNIEnv *env, jclass owner, jbyteArray state, jbyteArray event) {
  if (!prepare(env, owner)) return NULL;
  char *saved = bytes(env, state), *input = bytes(env, event);
  if (!saved || !input) { free(saved); free(input); return NULL; }
  const char *text = osprey_app_dispatch(saved, input);
  free(saved); free(input);
  return result(env, text);
}

const char *native_slice(const char *key) {
  jmethodID method = (*current)->GetStaticMethodID(current, native_class, "slice", "(Ljava/lang/String;)[B");
  jstring name = (*current)->NewStringUTF(current, key);
  jbyteArray value = (*current)->CallStaticObjectMethod(current, native_class, method, name);
  (*current)->DeleteLocalRef(current, name);
  free(borrowed); borrowed = value ? bytes(current, value) : NULL;
  if (value) (*current)->DeleteLocalRef(current, value);
  return borrowed;
}

int64_t native_shape(int64_t code, int64_t index, double a, double b, double c, double d, double e, int64_t tint, int64_t layer) {
  jmethodID method = (*current)->GetStaticMethodID(current, native_class, "shape", "(JJDDDDDJJ)J");
  return (*current)->CallStaticLongMethod(current, native_class, method, (jlong)code, (jlong)index, a,b,c,d,e,(jlong)tint,(jlong)layer);
}

int64_t osprey_tls_connect(const char *host, int64_t port, int64_t timeout) {
  jmethodID method = (*current)->GetStaticMethodID(current, tls_class, "connect", "(Ljava/lang/String;JJ)J");
  jstring name = (*current)->NewStringUTF(current, host);
  jlong value = (*current)->CallStaticLongMethod(current, tls_class, method, name, (jlong)port, (jlong)timeout);
  (*current)->DeleteLocalRef(current, name);
  return value;
}

int64_t osprey_tls_send_hex(int64_t handle, const char *hex) {
  jmethodID method = (*current)->GetStaticMethodID(current, tls_class, "write", "(JLjava/lang/String;)J");
  jstring text = (*current)->NewStringUTF(current, hex);
  jlong value = (*current)->CallStaticLongMethod(current, tls_class, method, (jlong)handle, text);
  (*current)->DeleteLocalRef(current, text);
  return value;
}

static const char *text_result(jstring value) {
  const char *text = value ? (*current)->GetStringUTFChars(current, value, NULL) : NULL;
  free(borrowed); borrowed = text ? strdup(text) : NULL;
  if (text) (*current)->ReleaseStringUTFChars(current, value, text);
  if (value) (*current)->DeleteLocalRef(current, value);
  return borrowed;
}

const char *osprey_tls_recv_hex(int64_t handle, int64_t count, int64_t timeout) {
  jmethodID method = (*current)->GetStaticMethodID(current, tls_class, "read", "(JJJ)Ljava/lang/String;");
  return text_result((*current)->CallStaticObjectMethod(current, tls_class, method, (jlong)handle, (jlong)count, (jlong)timeout));
}

int64_t osprey_tls_status(int64_t handle) {
  jmethodID method = (*current)->GetStaticMethodID(current, tls_class, "status", "(J)J");
  return (*current)->CallStaticLongMethod(current, tls_class, method, (jlong)handle);
}

int64_t osprey_tls_close(int64_t handle) {
  jmethodID method = (*current)->GetStaticMethodID(current, tls_class, "close", "(J)J");
  return (*current)->CallStaticLongMethod(current, tls_class, method, (jlong)handle);
}

const char *osprey_net_local_address(const char *host, int64_t port) {
  jmethodID method = (*current)->GetStaticMethodID(current, tls_class, "route", "(Ljava/lang/String;J)Ljava/lang/String;");
  jstring name = (*current)->NewStringUTF(current, host);
  jstring value = (*current)->CallStaticObjectMethod(current, tls_class, method, name, (jlong)port);
  (*current)->DeleteLocalRef(current, name);
  return text_result(value);
}

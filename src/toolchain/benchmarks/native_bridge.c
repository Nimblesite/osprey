// [NATIVE-SHM-VERIFY] Component round trips, not a framework/app comparison.
// Both paths import identical bytes and compare every byte before releasing.
#include "native_bridge.h"
#include "json_tests_shared.h"
#include <assert.h>
#include <inttypes.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#ifdef NDEBUG
#error This benchmark requires assertions
#endif
#ifdef CLOCK_MONOTONIC_RAW
#define BENCH_CLOCK CLOCK_MONOTONIC_RAW
#else
#define BENCH_CLOCK CLOCK_MONOTONIC
#endif
enum { WARMUP = 100, SAMPLES = 1000 };

static uint64_t now_ns(void) {
  struct timespec time;
  assert(clock_gettime(BENCH_CLOCK, &time) == 0);
  return (uint64_t)time.tv_sec * UINT64_C(1000000000) + (uint64_t)time.tv_nsec;
}

static uint64_t json_roundtrip(char *text, size_t bytes) {
  uint64_t start = now_ns();
  int64_t writer = json_writer_open();
  assert(writer > 0 && json_write_string(writer, text) == 0);
  char *encoded = json_writer_close(writer);
  assert(encoded);
  int64_t document = json_parse(encoded);
  assert(document > 0);
  char path[] = "";
  char *decoded = json_get(document, path);
  assert(decoded && strlen(decoded) == bytes &&
         memcmp(decoded, text, bytes + 1) == 0);
  free(decoded);
  assert(json_free(document) == 0);
  free(encoded);
  return now_ns() - start;
}

static uint64_t native_roundtrip(OspNativeBridge *bridge, const char *text,
                                 size_t bytes) {
  uint64_t start = now_ns();
  td_OwnedLease candidate, reader;
  int64_t revision, observed;
  assert(osp_native_create(bridge, 1, true, text, (int64_t)bytes + 1, NULL, 0,
                           &candidate) == 0);
  assert(osp_native_publish(bridge, &candidate, &revision) == 0);
  assert(osp_native_acquire(bridge, &reader, &observed) == 0 &&
         revision == observed);
  const void *view;
  assert(osp_native_view(bridge, &reader, 1, 0, (int64_t)bytes + 1, &view) ==
         0);
  assert(strlen(view) == bytes && memcmp(view, text, bytes + 1) == 0);
  assert(osp_native_release(bridge, &reader) == 0);
  return now_ns() - start;
}

static int compare(const void *left, const void *right) {
  uint64_t a = *(const uint64_t *)left, b = *(const uint64_t *)right;
  return (a > b) - (a < b);
}

static void summary(const char *kind, size_t bytes, uint64_t *samples) {
  qsort(samples, SAMPLES, sizeof *samples, compare);
  printf("summary,%s,%zu,%" PRIu64 ",%" PRIu64 "\n", kind, bytes,
         samples[SAMPLES / 2], samples[(SAMPLES * 99) / 100]);
}

static void sample_pair(OspNativeBridge *bridge, char *text, size_t bytes,
                        int index, uint64_t *native, uint64_t *json) {
  if (index % 2 == 0) {
    *native = native_roundtrip(bridge, text, bytes);
    *json = json_roundtrip(text, bytes);
  } else {
    *json = json_roundtrip(text, bytes);
    *native = native_roundtrip(bridge, text, bytes);
  }
}

static char *input(size_t bytes) {
  const char pattern[] = "osprey \"quoted\" \\ slash\tend";
  char *text = malloc(bytes + 1);
  assert(text);
  for (size_t i = 0; i < bytes; i++)
    text[i] = pattern[i % (sizeof pattern - 1)];
  text[bytes] = '\0';
  return text;
}

static void measure(OspNativeBridge *bridge, size_t bytes) {
  char *text = input(bytes);
  uint64_t native[SAMPLES], json[SAMPLES];
  for (int i = 0; i < WARMUP; i++)
    sample_pair(bridge, text, bytes, i, native, json);
  for (int i = 0; i < SAMPLES; i++)
    sample_pair(bridge, text, bytes, i, &native[i], &json[i]);
  for (int i = 0; i < SAMPLES; i++)
    printf("sample,%d,%zu,%" PRIu64 ",%" PRIu64 "\n", i, bytes, native[i],
           json[i]);
  summary("native", bytes, native);
  summary("json", bytes, json);
  free(text);
}

int main(void) {
  td_ApplicationInstance instance = {{1}};
  OspNativeBridge *bridge = NULL;
  assert(osp_native_open(&instance, TD_SHARED_FINGERPRINT, 262144, 256, 64,
                         &bridge) == 0);
  puts("kind,index_or_component,payload_bytes,native_ns_or_p50,json_ns_or_p99");
  const size_t sizes[] = {64, 4096, 65536};
  for (size_t i = 0; i < sizeof sizes / sizeof sizes[0]; i++)
    measure(bridge, sizes[i]);
  assert(osp_native_destroy(bridge) == 0);
  return 0;
}

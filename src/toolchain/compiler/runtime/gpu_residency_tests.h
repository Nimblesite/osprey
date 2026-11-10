// Resident device buffers in the selection suite [GPU-BUFFER-RESIDENT]
// [GPU-BUFFER-RESIDENT-OWNER]: products and launches stay on the device until
// read, failed readbacks stay pending and propagate, and finalization releases
// each object through the adapter that owns it whatever is selected later.
// Included once by gpu_selection_tests.c, after its mock adapters and helpers.
#ifndef OSPREY_GPU_RESIDENCY_TESTS_H
#define OSPREY_GPU_RESIDENCY_TESTS_H

// The product's inputs went up once (the same buffer twice binds one object)
// and its result comes down once, when first read; a dropped buffer releases
// its device copy exactly once.
static void resident_product(void *a, void *product, int64_t mark) {
  unsigned up = uploads, down = downloads;
  assert(osprey_gpu_get(product, 0) == mark && downloads == down + 1);
  assert(osprey_gpu_get(product, 0) == mark && downloads == down + 1);
  void *again = osprey_gpu_matmul(a, a, 1, 1, 1, 8);
  assert(again && uploads == up + 1);
  unsigned freed = releases;
  osprey_gpu_finalize(again);
  osprey_gpu_finalize(again);
  assert(releases == freed + 1);
}

// A launch's result stays on the device until read, a buffer uniform binds
// as a resident input with its length, a fold's accumulator comes straight
// back, and a host write forgets the device copy.
static void resident_launch(int64_t mark) {
  void *out = osprey_gpu_alloc(1, 8), *table = osprey_gpu_alloc(4, 8), *in = osprey_gpu_alloc(3, 8);
  int64_t uniforms[2] = {7, (int64_t)(intptr_t)table}, acc = 5;
  unsigned down = downloads;
  assert(launch(out, "sb", uniforms) == 1 && downloads == down);
  assert(last_args == 5 + 1 + 2 && last_threads == 3);
  assert(osprey_gpu_get(out, 0) == mark && downloads == down + 1);
  assert(osprey_gpu_launch("osprey_kernel_1", FAULTING_KERNEL, "", NULL, in, NULL, NULL, &acc, 3) == 1);
  assert(acc == mark && last_threads == 1 && downloads == down + 1);
  unsigned freed = releases;
  osprey_gpu_set(out, 0, 5);
  assert(releases == freed + 1 && osprey_gpu_get(out, 0) == 5 && downloads == down + 1);
}

// [FAILURE-EXPLICIT]: failed synchronization leaves the buffer pending and
// preserves the reason; invalid indices never ask the device to download.
static void readable_failure(int64_t mark) {
  void *out = osprey_gpu_alloc(1, 8);
  assert(launch(out, "", NULL) == 1);
  unsigned down = downloads;
  device_error = 1;
  osp_io_error_clear();
  assert(osprey_gpu_readable(out, -1) == 0 && osprey_gpu_readable(out, 1) == 0 && downloads == down);
  assert(osprey_gpu_readable(out, 0) == -1 && downloads == down + 1);
  assert(strcmp(osp_io_error(), "mock: readback: injected download failure") == 0);
  device_error = 0;
  osp_io_error_clear();
  assert(osprey_gpu_readable(out, 0) == 1 && downloads == down + 2 && !osp_io_error());
  assert(osprey_gpu_get(out, 0) == mark && downloads == down + 2);
}

// [FAILURE-EXPLICIT] A host matrix product must propagate resident read errors.
static void matmul_readback_failure(const char *mode) {
  void *pending = osprey_gpu_alloc(1, 8), *ready = unit();
  assert(launch(pending, "", NULL) == 1);
  unsigned down = downloads;
  select_mode("host");
  osp_io_error_clear();
  assert(!osprey_gpu_matmul(pending, ready, 1, 1, 1, 8));
  assert(downloads == down && strstr(osp_io_error(), "device readback: no GPU adapter") != NULL);
  osp_io_error_clear();
  assert(!osprey_gpu_matmul(ready, pending, 1, 1, 1, 8));
  assert(downloads == down && strstr(osp_io_error(), "device readback: no GPU adapter") != NULL);
  select_mode(mode);
  osp_io_error_clear();
  assert(osprey_gpu_matmul(pending, ready, 1, 1, 1, 8) && downloads == down && !osp_io_error());
}

// [GPU-BUFFER-RESIDENT] Both uploaded inputs and resident outputs retain
// their owning adapter when selection changes; finalization never changes IO.
// Implements [GPU-BUFFER-RESIDENT-OWNER].
static void finalized_pair(void *input, void *out, unsigned owner) {
  unsigned freed = releases, owned = releases_by_adapter[owner];
  osp_io_error_text("test", "preserved", "read error");
  osprey_gpu_finalize(input);
  osprey_gpu_finalize(out);
  assert(releases == freed + 2 && releases_by_adapter[owner] == owned + 2);
  assert(strcmp(osp_io_error(), "test: preserved: read error") == 0);
  osprey_gpu_finalize(input);
  osprey_gpu_finalize(out);
  assert(releases == freed + 2);
}

static void owned_finalization(const char *mode, unsigned owner) {
  const char *next[] = {"host", "invalid-for-release", owner == 2 ? "opencl" : "metal"};
  for (unsigned i = 0; i < sizeof next / sizeof next[0]; i++) {
    select_mode(mode);
    void *input = unit();
    void *out = osprey_gpu_matmul(input, input, 1, 1, 1, 8);
    assert(out);
    select_mode(next[i]);
    finalized_pair(input, out, owner);
  }
  select_mode(mode);
}

// A host write discards an already synchronized device copy through the same
// owner-aware path, even after selection changes to the host.
static void writable_finalization(const char *mode, unsigned owner) {
  select_mode(mode);
  void *input = unit();
  void *out = osprey_gpu_matmul(input, input, 1, 1, 1, 8);
  assert(out && osprey_gpu_readable(out, 0) == 1);
  unsigned freed = releases, owned = releases_by_adapter[owner];
  select_mode("host");
  osprey_gpu_set(out, 0, 99);
  assert(osprey_gpu_get(out, 0) == 99);
  osprey_gpu_finalize(input);
  osprey_gpu_finalize(out);
  assert(releases == freed + 2 && releases_by_adapter[owner] == owned + 2);
  select_mode(mode);
}

#endif

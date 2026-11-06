#ifndef OSPREY_GPU_OPENCL_API_H
#define OSPREY_GPU_OPENCL_API_H
#include "gpu_device.h"
#include <stddef.h>
#include <stdint.h>
#if defined(_WIN32)
#define OSP_CL_CALL __stdcall
#else
#define OSP_CL_CALL
#endif
// OpenCL 1.2 driver ABI. Loaded dynamically: no SDK or vendor library is a
// compiler dependency [GPU-BACKEND-PORTABILITY]. Opaque handles stay opaque.
typedef struct {
  int (OSP_CL_CALL *platforms)(unsigned, void **, unsigned *);
  int (OSP_CL_CALL *devices)(void *, uint64_t, unsigned, void **, unsigned *);
  int (OSP_CL_CALL *info)(void *, unsigned, size_t, void *, size_t *);
  void *(OSP_CL_CALL *context)(const intptr_t *, unsigned, void *const *, void *, void *, int *);
  void *(OSP_CL_CALL *queue)(void *, void *, uint64_t, int *);
  void *(OSP_CL_CALL *program)(void *, unsigned, const char **, const size_t *, int *);
  int (OSP_CL_CALL *build)(void *, unsigned, void *const *, const char *, void *, void *);
  int (OSP_CL_CALL *build_info)(void *, void *, unsigned, size_t, void *, size_t *);
  void *(OSP_CL_CALL *kernel)(void *, const char *, int *);
  void *(OSP_CL_CALL *buffer)(void *, uint64_t, size_t, const void *, int *);
  int (OSP_CL_CALL *arg)(void *, unsigned, size_t, const void *);
  int (OSP_CL_CALL *launch)(void *, void *, unsigned, const size_t *, const size_t *, const size_t *, unsigned, const void *, void *);
  int (OSP_CL_CALL *read)(void *, void *, unsigned, size_t, size_t, void *, unsigned, const void *, void *);
  int (OSP_CL_CALL *release_buffer)(void *);
  int (OSP_CL_CALL *release_kernel)(void *);
  int (OSP_CL_CALL *release_program)(void *);
  int (OSP_CL_CALL *release_queue)(void *);
  int (OSP_CL_CALL *release_context)(void *);
} OspreyOpenCLApi;

int osprey_gpu_opencl_dispatch(const OspreyGpuDispatch *dispatch);
int osprey_gpu_opencl_download(void *object, void *out, uint64_t count);
void osprey_gpu_opencl_release(void *object);
const char *osprey_gpu_opencl_name(void);
uint64_t osprey_gpu_opencl_completed(void);
#if defined(OSPREY_GPU_TESTING)
void osprey_gpu_opencl_test_driver(const OspreyOpenCLApi *api);
#endif
#endif

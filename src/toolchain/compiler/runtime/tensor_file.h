// The safetensors reader every tensor loader shares: `loadTensor` builds a
// host List<float> from it [BUILTIN-FILE-TENSOR], `gpuLoadTensor` and `gpuLoad`
// a GpuBuffer [GPU-BUFFER-TENSOR] [GPU-BUFFER-LOAD]. It knows no container: it
// locates a tensor's bytes and hands the caller each chunk of STORED BYTES, in
// file order, so the host list widens them to double and the GPU buffer packs
// them at the element width its program asked for [GPU-BUFFER-ELEM] — neither
// surface depends on the other, and the widening has one spelling.
#ifndef OSPREY_TENSOR_FILE_H
#define OSPREY_TENSOR_FILE_H

#include <stddef.h>
#include <stdint.h>

// Stored float formats, valued by byte width (BF16 is 2 bytes, tagged 3).
typedef enum { OSP_DT_F64 = 8, OSP_DT_F32 = 4, OSP_DT_F16 = 2, OSP_DT_BF16 = 3 } OspDtype;

// The dtype's spelling in a safetensors header, or NULL for a value outside
// the four float formats.
const char *osp_tensor_dtype_name(OspDtype dtype);

// Where a run of stored elements lives: the byte offset of the first, the
// stored format, and how many there are.
typedef struct {
  int64_t offset;
  OspDtype dtype;
  int64_t count;
} OspTensorSpan;

// Receives the next `n` elements' stored bytes, in file order; one element is
// `osp_tensor_width(span->dtype)` bytes wide.
typedef void (*OspTensorSink)(void *ctx, const unsigned char *bytes, size_t n);

// The byte width of one stored element.
static inline size_t osp_tensor_width(OspDtype dtype) {
  return dtype == OSP_DT_BF16 ? 2 : (size_t)dtype;
}

// The span of tensor `name` in safetensors file `path`: 1, or 0 with the
// reason recorded on the io_error channel under `op`.
int osp_tensor_locate(const char *op, const char *path, const char *name, OspTensorSpan *span);

// Read every element of `span` (count never negative) and hand its stored
// bytes to `sink`: 1, or 0 with the reason recorded. A short read is an error,
// never a shorter tensor: a truncated weight file must not load as a smaller
// model.
int osp_tensor_read(const char *op, const char *path, const OspTensorSpan *span, OspTensorSink sink,
                    void *ctx);

// Widen the little-endian stored element at `at` to a double, exactly:
// normals, subnormals, signed zeros, infinities and NaN each widen to the
// double with the same value.
double osp_tensor_widen(OspDtype dtype, const unsigned char *at);

#endif

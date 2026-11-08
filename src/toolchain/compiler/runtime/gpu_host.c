// Host-backend operations on dense buffers beyond the buffer primitive itself
// (docs/specs/runtime/0034-GPUComputation.md): the row-major dense product behind
// `gpuMatMul` [GPU-MATMUL], and the two ways a trained model's weights enter a
// program without passing through a List — raw little-endian binary32 words
// at a byte offset (`gpuLoad`, [GPU-BUFFER-LOAD]: the layout of llm.c's
// checkpoints and of any raw dump) and one named tensor out of a safetensors
// file (`gpuLoadTensor`, [GPU-BUFFER-TENSOR]: the Hugging Face interchange
// format — an 8-byte little-endian header length, a JSON header mapping each
// name to its dtype, shape and data_offsets, then the raw tensor bytes). Both
// read through tensor_file.c, the reader the host `loadTensor` shares.
//
// Each stored dtype loads at the requested width; only BF16 widens to float32.
// Every failure path records WHY on the io_error channel and returns
// NULL; codegen turns that pair into `Error { message }` [BUILTIN-FILE-ERRMSG].
// Like gpu_runtime.c this unit is backend-neutral: buffers come from
// osprey_gpu_alloc, scratch memory never outlives the call, and a buffer
// allocated for a result that then fails is released before the failure is
// reported, so a NULL result owes the memory backend nothing.

#include "io_error.h"
#include "memory_hooks.h"
#include "gpu_device.h"
#include "tensor_file.h"
#include "float_spelling.h"

#include <errno.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

void *osprey_gpu_alloc(int64_t length, int64_t elem_bytes);
int64_t osprey_gpu_len(void *buffer);
int64_t osprey_gpu_elem_bytes(void *buffer);
void *osprey_gpu_data(void *buffer);

// The NULL of a failed operation, with the buffer it had allocated released.
// The ARC corpus lane found two live objects per failed load before this
// existed: src/osprey/tests/core/gpu/transformer pins it, memory_golden_tests.c counts it.
static void *abandoned(void *buffer) {
  osp_release(buffer);
  return NULL;
}

static const char OP_MATMUL[] = "gpuMatMul";
static const char OP_MATMUL_QUANT[] = "gpuMatMulQuant";
static const char OP_LOAD[] = "gpuLoad";
static const char OP_TENSOR[] = "gpuLoadTensor";

// --- the dense product -------------------------------------------------------

// rows × cols as an element count, or -1 when negative or overflowing.
static int64_t area(int64_t rows, int64_t cols, int64_t elem) {
  if (rows < 0 || cols < 0 || (rows != 0 && cols > INT64_MAX / elem / rows)) {
    return -1;
  }
  return rows * cols;
}

// Reject an operand whose length is not the stated shape, naming which one.
static int shaped(const char *op, void *buffer, int64_t count, const char *operand) {
  if (count >= 0 && osprey_gpu_len(buffer) == count) {
    return 1;
  }
  char reason[96];
  (void)snprintf(reason, sizeof reason, "has %lld elements, the stated shape needs %lld",
                 (long long)osprey_gpu_len(buffer), (long long)count);
  osp_io_error_text(op, operand, reason);
  return 0;
}

// out[r*cols + c] = sum over k, in order from 0.0, of left[r*inner + k] *
// right[k*cols + c], at the operand width. NULL with the reason recorded when either
// operand's length disagrees with the shape or the shape overflows.
static void *matrix_result(void *left, void *right, int64_t rows, int64_t inner, int64_t cols, int64_t elem) {
  if (elem != 4 && elem != 8) {
    osp_io_error_text(OP_MATMUL, "result", "must hold float or float32 elements");
    return NULL;
  }
  int64_t n = area(rows, cols, elem);
  if (n < 0) {
    osp_io_error_text(OP_MATMUL, "result", "rows * cols is negative or overflows");
    return NULL;
  }
  if (!shaped(OP_MATMUL, left, area(rows, inner, elem), "left operand") ||
      !shaped(OP_MATMUL, right, area(inner, cols, elem), "right operand")) {
    return NULL;
  }
  if ((left && osprey_gpu_elem_bytes(left) != elem) || (right && osprey_gpu_elem_bytes(right) != elem)) {
    osp_io_error_text(OP_MATMUL, "operands", "must hold the same float or float32 element type");
    return NULL;
  }
  void *out = osprey_gpu_alloc(n, elem);
  if (!out || osprey_gpu_len(out) != n) {
    osp_io_error_set(OP_MATMUL, "result", ENOMEM);
    return abandoned(out);
  }
  return out;
}

// [GPU-MATMUL]: one source template, instantiated at each arithmetic width.
// Multiplication is a separate full expression: both products and sums round.
#define PRODUCT(SUFFIX, SCALAR) \
static SCALAR read##SUFFIX(const unsigned char *at) { SCALAR value; memcpy(&value, at, sizeof value); return value; } \
static void write##SUFFIX(unsigned char *at, SCALAR value) { memcpy(at, &value, sizeof value); } \
static void product##SUFFIX(unsigned char *out, const unsigned char *a, const unsigned char *b, int64_t rows, int64_t inner, int64_t cols) { \
  for (int64_t r = 0; r < rows; r++) for (int64_t k = 0; k < inner; k++) { \
    SCALAR x = read##SUFFIX(a + (r * inner + k) * sizeof(SCALAR)); \
    for (int64_t c = 0; c < cols; c++) { \
      SCALAR product = x * read##SUFFIX(b + (k * cols + c) * sizeof(SCALAR)); \
      unsigned char *at = out + (r * cols + c) * sizeof(SCALAR); \
      write##SUFFIX(at, read##SUFFIX(at) + product); \
    } \
  } \
}
PRODUCT(32, float)
PRODUCT(64, double)
#undef PRODUCT

// The product on the selected device, where its operands and result stay
// resident [GPU-BUFFER-RESIDENT], or on the host when the host is selected.
void *osprey_gpu_matmul(void *left, void *right, int64_t rows, int64_t inner, int64_t cols, int64_t elem_bytes) {
  void *out = matrix_result(left, right, rows, inner, cols, elem_bytes);
  if (!out) return NULL;
  int device = osprey_gpu_device_matmul(left, right, out, rows, inner, cols);
  if (device != 0) return device > 0 ? out : abandoned(out);
  // [FAILURE-EXPLICIT] Synchronize before either raw accessor can quarantine
  // a failed download; the Result channel retains the reason and owns no out.
  if (rows && inner && cols &&
      (osprey_gpu_readable(left, 0) != 1 || osprey_gpu_readable(right, 0) != 1)) return abandoned(out);
  if (elem_bytes == 4)
    product32(osprey_gpu_data(out), osprey_gpu_data(left), osprey_gpu_data(right), rows, inner, cols);
  else
    product64(osprey_gpu_data(out), osprey_gpu_data(left), osprey_gpu_data(right), rows, inner, cols);
  return out;
}

// --- the quantized product [LLM-MATMUL-QUANT] -----------------------------------------

// The host oracle compiles the library kernels' own lane functions as C, so it
// and every device decode and round each weight through the same source.
#include "gpu_f64.metal"
#include "gpu_math64.metal"
#include "gpu_math_ext.metal"
#define GPU_HOST_QUANT_ORACLE
#include "gpu_device_lib.metal"
#undef GPU_HOST_QUANT_ORACLE
#undef constant
#undef kernel

typedef struct {
  const char *name;
  const char *kernel;
  int64_t elements;
  int64_t bytes;
  void (*weights)(const unsigned char *block, uint lane, float *w);
} QuantFormat;

static const QuantFormat QUANT_FORMATS[] = {
    {"Q4_K", "osprey_matmul_q4_k", 256, 144, q4_k_weights},
    {"Q8_0", "osprey_matmul_q8_0", 32, 34, q8_0_weights},
    {"Q6_K", "osprey_matmul_q6_k", 256, 210, q6_k_weights},
    {"Q5_K", "osprey_matmul_q5_k", 256, 176, q5_k_weights},
    {"Q3_K", "osprey_matmul_q3_k", 256, 110, q3_k_weights},
    {"IQ4_NL", "osprey_matmul_iq4_nl", 32, 18, iq4_nl_weights},
    {"IQ4_XS", "osprey_matmul_iq4_xs", 256, 136, iq4_xs_weights},
    {"IQ3_S", "osprey_matmul_iq3_s", 256, 110, iq3_s_weights},
};
#define QUANT_FORMAT_COUNT (sizeof QUANT_FORMATS / sizeof QUANT_FORMATS[0])

// The block format `type` names, or NULL for one the product does not decode.
static const QuantFormat *quant_format(const char *type) {
  for (size_t i = 0; type && i < QUANT_FORMAT_COUNT; i++)
    if (strcmp(type, QUANT_FORMATS[i].name) == 0) return &QUANT_FORMATS[i];
  return NULL;
}

// Lane `lane` of an output in the kernels' order: its weights of every block it
// visits, summed in block order from 0.0f.
static float lane_sum(const QuantFormat *format, const float *xr, const unsigned char *row, int64_t blocks, uint lane) {
  uint per_block = (uint)format->elements / 8, per_span = QUANT_LANES / per_block;
  float acc = 0.0f;
  for (int64_t blk = lane / per_block; blk < blocks; blk += per_span) {
    float w[8];
    format->weights(row + blk * format->bytes, lane % per_block, w);
    acc = quant_lane(acc, xr + blk * format->elements + (lane % per_block) * 8, w);
  }
  return acc;
}

// quant_reduce's rounds over the 32 lane sums: every lane reads its partner,
// then every lane adds it.
static float butterfly(float *sums) {
  for (uint off = QUANT_LANES / 2; off > 0; off >>= 1) {
    float other[QUANT_LANES];
    for (uint l = 0; l < QUANT_LANES; l++) other[l] = sums[l ^ off];
    for (uint l = 0; l < QUANT_LANES; l++) sums[l] = sums[l] + other[l];
  }
  return sums[0];
}

// The threads each form of the product dispatches: 32 per QUANT_ROWS by
// QUANT_COLS tile for few rows, a QUANT_GROUP per many-row tile.
static void quant_grid(int64_t rows, int64_t cols, uint64_t threads[2]) {
  threads[0] = (uint64_t)((rows + QUANT_ROWS - 1) / QUANT_ROWS * ((cols + QUANT_COLS - 1) / QUANT_COLS) * QUANT_LANES);
  threads[1] = (uint64_t)((rows + QUANT_MM_ROWS_TILE - 1) / QUANT_MM_ROWS_TILE *
                          ((cols + QUANT_MM_COLS_TILE - 1) / QUANT_MM_COLS_TILE) * QUANT_GROUP);
}

// The oracle of the library kernels `osprey_matmul_<type>`, lane for lane.
static void product_quant(const QuantFormat *format, float *out, const float *x, const unsigned char *w, int64_t rows,
                          int64_t inner, int64_t cols) {
  int64_t blocks = inner / format->elements;
  for (int64_t r = 0; r < rows; r++) for (int64_t c = 0; c < cols; c++) {
    float sums[QUANT_LANES];
    for (uint lane = 0; lane < QUANT_LANES; lane++)
      sums[lane] = lane_sum(format, x + r * inner, w + c * blocks * format->bytes, blocks, lane);
    out[r * cols + c] = butterfly(sums);
  }
}

// The names the product decodes, for the error that rejects any other.
static void quant_names(char *text, size_t size) {
  size_t at = 0;
  for (size_t i = 0; i < QUANT_FORMAT_COUNT && at < size; i++)
    at += (size_t)snprintf(text + at, size - at, "%s%s", i ? ", " : "", QUANT_FORMATS[i].name);
}

// out[r*cols + c] = sum over k of x[r*inner + k] * w_c[k], where `w` holds
// `cols` rows of `inner / <weights per block>` blocks of `type` each, as a GGUF
// stores a [cols, inner] tensor, and `x` is a [rows, inner] float32 matrix. On
// the selected device the operands and result stay resident; the host runs the
// oracle.
void *osprey_gpu_matmul_quant(void *x, void *w, const char *type, int64_t rows, int64_t inner, int64_t cols) {
  const QuantFormat *format = quant_format(type);
  if (!format) {
    char names[96];
    char reason[sizeof names + sizeof "unsupported tensor type; expected "];
    quant_names(names, sizeof names);
    (void)snprintf(reason, sizeof reason, "unsupported tensor type; expected %s", names);
    osp_io_error_text(OP_MATMUL_QUANT, "weights", reason);
    return NULL;
  }
  if (inner <= 0 || inner % format->elements != 0) {
    char reason[96];
    (void)snprintf(reason, sizeof reason, "must be a positive multiple of %lld", (long long)format->elements);
    osp_io_error_text(OP_MATMUL_QUANT, "inner", reason);
    return NULL;
  }
  int64_t n = area(rows, cols, 4);
  int64_t blocks = area(cols, inner / format->elements, format->bytes);
  if (n < 0 || blocks < 0) {
    osp_io_error_text(OP_MATMUL_QUANT, "result", "rows * cols is negative or overflows");
    return NULL;
  }
  if (!shaped(OP_MATMUL_QUANT, x, area(rows, inner, 4), "x operand") ||
      !shaped(OP_MATMUL_QUANT, w, blocks * format->bytes, "weights")) {
    return NULL;
  }
  if ((x && osprey_gpu_elem_bytes(x) != 4) || (w && osprey_gpu_elem_bytes(w) != 1)) {
    osp_io_error_text(OP_MATMUL_QUANT, "operands", "x must hold float32 and the weights uint8");
    return NULL;
  }
  const void *words = blocks ? osprey_gpu_data(w) : NULL;
  if ((uintptr_t)words % 4 != 0) {
    osp_io_error_text(OP_MATMUL_QUANT, "weights", "must start on a 4-byte boundary");
    return NULL;
  }
  void *out = osprey_gpu_alloc(n, 4);
  if (!out || osprey_gpu_len(out) != n) {
    osp_io_error_set(OP_MATMUL_QUANT, "result", ENOMEM);
    return abandoned(out);
  }
  uint64_t threads[2];
  quant_grid(rows, cols, threads);
  int device = osprey_gpu_device_matmul_quant(format->kernel, x, w, out, rows, inner, cols, threads);
  if (device != 0) return device > 0 ? out : abandoned(out);
  if (n && (osprey_gpu_readable(x, 0) != 1 || osprey_gpu_readable(w, 0) != 1)) return abandoned(out);
  product_quant(format, osprey_gpu_data(out), osprey_gpu_data(x), osprey_gpu_data(w), rows, inner, cols);
  return out;
}

// --- loading ---------------------------------------------------------------------

// The reader's sink context: where this chunk lands and how wide a stored
// element is.
typedef struct {
  unsigned char *cursor;
  size_t stored;
} BufferLoad;

// Copy each chunk's stored bytes straight into the buffer's payload, byte for
// byte [GPU-BUFFER-ELEM]: an F64 into `float`, an F32 into `float32`, an F16
// into `float16`.
static void into_bytes(void *ctx, const unsigned char *bytes, size_t n) {
  BufferLoad *load = ctx;
  memcpy(load->cursor, bytes, n * load->stored);
  load->cursor += n * load->stored;
}

// Widen each stored BF16 (the upper sixteen bits of a binary32) into the
// float32 buffer exactly: place the stored bits in the upper half of the
// binary32 word [GPU-BUFFER-TENSOR].
static void into_f32_from_bf16(void *ctx, const unsigned char *bytes, size_t n) {
  BufferLoad *load = ctx;
  for (size_t i = 0; i < n; i++) {
    load->cursor[0] = 0;
    load->cursor[1] = 0;
    load->cursor[2] = bytes[2 * i];
    load->cursor[3] = bytes[2 * i + 1];
    load->cursor += 4;
  }
}

// The sink that fills a buffer of `elem`-byte elements from storage of
// `span->dtype`.
static OspTensorSink sink_of(const OspTensorSpan *span, int64_t elem) {
  if (span->dtype == OSP_DT_BF16 && elem == 4) return into_f32_from_bf16;
  return into_bytes;
}

// A fresh buffer of `span->count` elements of `elem` bytes read from `path`. A
// short read is an error, not a shorter buffer: a truncated weight file must
// never load as a smaller model [GPU-BUFFER-LOAD].
static void *load(const char *op, const char *path, const OspTensorSpan *span, int64_t elem) {
  if (span->count < 0) {
    osp_io_error_text(op, path, "a negative element count");
    return NULL;
  }
  void *out = span->count > INT64_MAX / elem ? NULL : osprey_gpu_alloc(span->count, elem);
  unsigned char *cursor = (unsigned char *)osprey_gpu_data(out);
  if (!out || osprey_gpu_len(out) != span->count || (span->count > 0 && !cursor)) {
    osp_io_error_set(op, path, ENOMEM);
    return abandoned(out);
  }
  BufferLoad sink = {cursor, osp_tensor_width(span->dtype)};
  return osp_tensor_read(op, path, span, sink_of(span, elem), &sink) ? out : abandoned(out);
}

// `count` little-endian binary32 values starting `offset` bytes into `path`,
// byte for byte with no widening [GPU-BUFFER-LOAD].
void *osprey_gpu_load_f32(const char *path, int64_t offset, int64_t count) {
  return load(OP_LOAD, path, &(OspTensorSpan){offset, OSP_DT_F32, count}, 4);
}

// The element type an element width names [GPU-BUFFER-TENSOR], or NULL when
// the width is not a float's.
static const char *asked_type(int64_t elem) {
  switch (elem) {
  case 8: return "float";
  case 4: return "float32";
  case 2: return "float16";
  }
  return NULL;
}

// The storage dtype `elem` admits [GPU-BUFFER-TENSOR]: F64 into `float`, F32
// and BF16 into `float32`, F16 into `float16`.
static int admits(OspDtype dtype, int64_t elem) {
  return (dtype == OSP_DT_F64 && elem == 8) || (dtype == OSP_DT_F32 && elem == 4) ||
         (dtype == OSP_DT_BF16 && elem == 4) || (dtype == OSP_DT_F16 && elem == 2);
}

// One tensor out of a safetensors file, at the element width `elem` names. A
// dtype the program did not ask for is an error naming both, never a silent
// conversion.
void *osprey_gpu_load_tensor(const char *path, const char *name, int64_t elem) {
  OspTensorSpan span;
  if (!osp_tensor_locate(OP_TENSOR, path, name, &span)) return NULL;
  if (!admits(span.dtype, elem)) {
    char reason[128];
    const char *asked = asked_type(elem);
    (void)snprintf(reason, sizeof reason, "tensor %s is %s, the program asked for %s", name,
                   osp_tensor_dtype_name(span.dtype), asked ? asked : "a non-float buffer");
    osp_io_error_text(OP_TENSOR, name, reason);
    return NULL;
  }
  return load(OP_TENSOR, path, &span, elem);
}

// The host oracle of the library kernel `osprey_delta_rule`, in its order: thread
// `t` owns one state column through every step.
static void delta_thread(const float *const in[6], float *out, int64_t heads, int64_t dim, int64_t steps, int64_t t) {
  const float *column = in[0];
  int64_t h = t / dim, v = t % dim, plane = h * dim * dim, cells = heads * dim;
  for (int64_t s = 0; s < steps; s++) {
    const float *queries = in[1] + s * cells, *keys = in[2] + s * cells;
    float fade = in[5][s * heads + h], recalled = 0.0f;
    for (int64_t k = 0; k < dim; k++) {
      float kept = column[plane + k * dim + v] * fade;
      float product = kept * keys[h * dim + k];
      recalled = recalled + product;
    }
    float delta = (in[3][s * cells + t] - recalled) * in[4][s * heads + h], read = 0.0f;
    for (int64_t k = 0; k < dim; k++) {
      float kept = column[plane + k * dim + v] * fade;
      float update = keys[h * dim + k] * delta;
      float next = kept + update;
      out[plane + k * dim + v] = next;
      float product = next * queries[h * dim + k];
      read = read + product;
    }
    out[heads * dim * dim + s * cells + t] = read;
    column = out;
  }
}

// [LLM-DELTA-RULE] `state` is a previous result or zeros: heads * dim * dim state
// cells then any outputs, which a step ignores. `queries`, `keys` and `values`
// hold dim values per head per token, `beta` and `decay` one per head per token;
// the token count is how many times heads * dim divides `queries`, and the result
// is the final state then every token's outputs in order ([LLM-PREFILL]).
void *osprey_gpu_delta_rule(void *state, void *queries, void *keys, void *values, void *beta, void *decay, int64_t heads, int64_t dim) {
  static const char OP[] = "gpuDeltaRule";
  int64_t cells = area(heads, dim, 4), plane = cells < 0 ? -1 : area(cells, dim, 4);
  if (cells < 0 || plane < 0 || heads <= 0 || dim <= 0 || plane > INT32_MAX) {
    osp_io_error_text(OP, "shape", "heads and dim must be positive and small enough for 32-bit indices");
    return NULL;
  }
  int64_t steps = queries ? osprey_gpu_len(queries) / cells : 0;
  if (steps < 1 || steps * cells > INT32_MAX - plane) {
    osp_io_error_text(OP, "queries", "must hold heads * dim values per token, for 32-bit indices' worth of tokens");
    return NULL;
  }
  int64_t total = plane + steps * cells;
  if (osprey_gpu_len(state) < plane) {
    osp_io_error_text(OP, "state", "must hold at least heads * dim * dim cells");
    return NULL;
  }
  if (!shaped(OP, queries, steps * cells, "queries") || !shaped(OP, keys, steps * cells, "keys") ||
      !shaped(OP, values, steps * cells, "values") || !shaped(OP, beta, steps * heads, "beta") || !shaped(OP, decay, steps * heads, "decay")) {
    return NULL;
  }
  void *const operands[6] = {state, queries, keys, values, beta, decay};
  for (unsigned i = 0; i < 6; i++) {
    if (osprey_gpu_elem_bytes(operands[i]) != 4) {
      osp_io_error_text(OP, "operands", "every operand must hold float32");
      return NULL;
    }
  }
  void *out = osprey_gpu_alloc(total, 4);
  if (!out || osprey_gpu_len(out) != total) {
    osp_io_error_set(OP, "result", ENOMEM);
    return abandoned(out);
  }
  int device = osprey_gpu_device_delta_rule(operands, out, heads, dim, steps);
  if (device != 0) return device > 0 ? out : abandoned(out);
  const float *in[6];
  for (unsigned i = 0; i < 6; i++) {
    if (osprey_gpu_readable(operands[i], 0) != 1) return abandoned(out);
    in[i] = osprey_gpu_data(operands[i]);
  }
  float *words = osprey_gpu_data(out);
  for (int64_t t = 0; t < heads * dim; t++) delta_thread(in, words, heads, dim, steps, t);
  return out;
}

// [LLM-SIGMOID] `1 / (1 + exp(-x))` of every element.
void *osprey_gpu_sigmoid(void *x) {
  static const char OP[] = "gpuSigmoid";
  if (x && osprey_gpu_elem_bytes(x) != 4) {
    osp_io_error_text(OP, "operand", "x must hold float32");
    return NULL;
  }
  int64_t n = osprey_gpu_len(x);
  void *out = osprey_gpu_alloc(n, 4);
  if (!out || osprey_gpu_len(out) != n) {
    osp_io_error_set(OP, "result", ENOMEM);
    return abandoned(out);
  }
  int device = osprey_gpu_device_sigmoid(x, out, n);
  if (device != 0) return device > 0 ? out : abandoned(out);
  if (n && osprey_gpu_readable(x, 0) != 1) return abandoned(out);
  const float *in = osprey_gpu_data(x);
  float *words = osprey_gpu_data(out);
  for (int64_t i = 0; i < n; i++) words[i] = 1.0f / (1.0f + expf(-in[i]));
  return out;
}

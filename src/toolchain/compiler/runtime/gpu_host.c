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

// The 6-bit scale and minimum of sub-block j of a Q4_K or Q5_K block, as ggml's
// `get_scale_min_k4` packs them.
static unsigned q4k_scale(const unsigned char *scales, unsigned j) {
  return j < 4 ? (unsigned)(scales[j] & 63) : (unsigned)((scales[j + 4] & 15) | ((scales[j - 4] >> 6) << 4));
}
static unsigned q4k_min(const unsigned char *scales, unsigned j) {
  return j < 4 ? (unsigned)(scales[j + 4] & 63) : (unsigned)((scales[j + 4] >> 4) | ((scales[j] >> 6) << 4));
}

// One weight's contribution: the product is a separate full expression, then
// added, so no step contracts ([GPU-MATMUL]'s order contract).
static float quant_add(float acc, float x, float weight) {
  float product = x * weight;
  return acc + product;
}

// The binary16 scale a GGUF block stores at byte `at`.
static float quant_half(const unsigned char *block, int64_t at) {
  return osp_f16_to_f32((uint16_t)(block[at] | (block[at + 1] << 8)));
}

// The host side of the library kernel's per-block step: decode one block's
// weights exactly as ggml's `dequantize_row_<type>` does, and add each of their
// products into `acc`.
static float q4k_step(float acc, const float *x, const unsigned char *b) {
  float d = quant_half(b, 0), dmin = quant_half(b, 2);
  for (unsigned j = 0; j < 8; j++) {
    unsigned sc = q4k_scale(b + 4, j), mn = q4k_min(b + 4, j);
    float d1 = d * (float)sc;
    float m1 = dmin * (float)mn;
    unsigned g = j >> 1, high = j & 1;
    const unsigned char *q = b + 16 + g * 32;
    const float *xe = x + g * 64 + high * 32;
    for (unsigned l = 0; l < 32; l++) {
      unsigned nib = high ? (unsigned)(q[l] >> 4) : (unsigned)(q[l] & 15);
      float weight = d1 * (float)nib;
      weight = weight - m1;
      acc = quant_add(acc, xe[l], weight);
    }
  }
  return acc;
}

static float q6_k_step(float acc, const float *x, const unsigned char *b) {
  float d = quant_half(b, 208);
  for (unsigned j = 0; j < 256; j++) {
    unsigned group = j >> 7, quarter = (j & 127) >> 5, l = j & 31;
    unsigned low = (unsigned)(b[64 * group + l + 32 * (quarter & 1)] >> (4 * (quarter >> 1))) & 15;
    unsigned q = low | (((unsigned)(b[128 + 32 * group + l] >> (2 * quarter)) & 3) << 4);
    float scale = (float)(signed char)b[192 + 8 * group + (l >> 4) + 2 * quarter];
    acc = quant_add(acc, x[j], d * scale * (float)((int)q - 32));
  }
  return acc;
}

static float q8_0_step(float acc, const float *x, const unsigned char *b) {
  float d = quant_half(b, 0);
  for (unsigned j = 0; j < 32; j++)
    acc = quant_add(acc, x[j], d * (float)(signed char)b[2 + j]);
  return acc;
}

static float q5_k_step(float acc, const float *x, const unsigned char *b) {
  float d = quant_half(b, 0), dmin = quant_half(b, 2);
  for (unsigned g = 0; g < 4; g++) {
    for (unsigned h = 0; h < 2; h++) {
      unsigned sub = 2 * g + h;
      unsigned sc = q4k_scale(b + 4, sub), mn = q4k_min(b + 4, sub);
      float d1 = d * (float)sc;
      float m1 = dmin * (float)mn;
      const unsigned char *ql = b + 48 + 32 * g;
      for (unsigned l = 0; l < 32; l++) {
        unsigned nib = h ? (unsigned)(ql[l] >> 4) : (unsigned)(ql[l] & 15);
        unsigned high = (unsigned)((b[16 + l] >> sub) & 1);
        float weight = d1 * (float)(nib + 16 * high);
        weight = weight - m1;
        acc = quant_add(acc, x[g * 64 + h * 32 + l], weight);
      }
    }
  }
  return acc;
}

static uint32_t q3k_word(const unsigned char *s, unsigned at) {
  return (uint32_t)s[at] | ((uint32_t)s[at + 1] << 8) | ((uint32_t)s[at + 2] << 16) | ((uint32_t)s[at + 3] << 24);
}

static unsigned q3k_scale(const unsigned char *b, unsigned index) {
  const unsigned char *s = b + 96;
  uint32_t low = q3k_word(s, 0), high = q3k_word(s, 4), packed = q3k_word(s, 8);
  unsigned word = index >> 2, part = index & 3;
  uint32_t bytes = word < 2 ? (word ? high : low) : ((word == 2 ? low : high) >> 4);
  uint32_t more = ((packed >> (2 * word)) & 0x03030303u) << 4;
  return (((bytes & 0x0f0f0f0fu) | more) >> (8 * part)) & 255u;
}

static float q3_k_step(float acc, const float *x, const unsigned char *b) {
  float d = quant_half(b, 108);
  for (unsigned g = 0; g < 2; g++) {
    for (unsigned j = 0; j < 4; j++) {
      for (unsigned side = 0; side < 2; side++) {
        unsigned sc = q3k_scale(b, 8 * g + 2 * j + side);
        float dl = d * (float)((int)sc - 32);
        const unsigned char *q = b + 32 + 32 * g + 16 * side;
        const unsigned char *hm = b + 16 * side;
        for (unsigned l = 0; l < 16; l++) {
          int low = (int)((q[l] >> (2 * j)) & 3);
          int high = (int)((hm[l] >> (4 * g + j)) & 1);
          acc = quant_add(acc, x[g * 128 + j * 32 + side * 16 + l], dl * (float)(low - (high ? 0 : 4)));
        }
      }
    }
  }
  return acc;
}

// ggml's `kvalues_iq4nl`, the sixteen non-linear levels IQ4_NL and IQ4_XS use.
static const int IQ4_NL_LEVELS[16] = {-127, -104, -83, -65, -49, -35, -22, -10, 1, 13, 25, 38, 53, 69, 89, 113};

static float iq4_nl_step(float acc, const float *x, const unsigned char *b) {
  float d = quant_half(b, 0);
  for (unsigned j = 0; j < 32; j++) {
    unsigned level = j < 16 ? (unsigned)(b[2 + j] & 15) : (unsigned)(b[2 + j - 16] >> 4);
    acc = quant_add(acc, x[j], d * (float)IQ4_NL_LEVELS[level]);
  }
  return acc;
}

static float iq4_xs_step(float acc, const float *x, const unsigned char *b) {
  float d = quant_half(b, 0);
  unsigned scales_h = (unsigned)(b[2] | (b[3] << 8));
  for (unsigned ib = 0; ib < 8; ib++) {
    unsigned ls = (unsigned)((b[4 + (ib >> 1)] >> (4 * (ib & 1))) & 15) | (((scales_h >> (2 * ib)) & 3) << 4);
    float dl = d * (float)((int)ls - 32);
    for (unsigned j = 0; j < 32; j++) {
      unsigned level = j < 16 ? (unsigned)(b[8 + 16 * ib + j] & 15) : (unsigned)(b[8 + 16 * ib + j - 16] >> 4);
      acc = quant_add(acc, x[32 * ib + j], dl * (float)IQ4_NL_LEVELS[level]);
    }
  }
  return acc;
}

// ggml's `iq3s_grid`: 512 words, each four 4-bit grid values packed little-endian.
static const uint32_t IQ3S_GRID[512] = {
    0x01010101u, 0x01010103u, 0x01010105u, 0x0101010bu, 0x0101010fu, 0x01010301u, 0x01010303u, 0x01010305u,
    0x01010309u, 0x0101030du, 0x01010501u, 0x01010503u, 0x0101050bu, 0x01010707u, 0x01010901u, 0x01010905u,
    0x0101090bu, 0x0101090fu, 0x01010b03u, 0x01010b07u, 0x01010d01u, 0x01010d05u, 0x01010f03u, 0x01010f09u,
    0x01010f0fu, 0x01030101u, 0x01030103u, 0x01030105u, 0x01030109u, 0x01030301u, 0x01030303u, 0x0103030bu,
    0x01030501u, 0x01030507u, 0x0103050fu, 0x01030703u, 0x0103070bu, 0x01030909u, 0x01030d03u, 0x01030d0bu,
    0x01030f05u, 0x01050101u, 0x01050103u, 0x0105010bu, 0x0105010fu, 0x01050301u, 0x01050307u, 0x0105030du,
    0x01050503u, 0x0105050bu, 0x01050701u, 0x01050709u, 0x01050905u, 0x0105090bu, 0x0105090fu, 0x01050b03u,
    0x01050b07u, 0x01050f01u, 0x01050f07u, 0x01070107u, 0x01070303u, 0x0107030bu, 0x01070501u, 0x01070505u,
    0x01070703u, 0x01070707u, 0x0107070du, 0x01070909u, 0x01070b01u, 0x01070b05u, 0x01070d0fu, 0x01070f03u,
    0x01070f0bu, 0x01090101u, 0x01090307u, 0x0109030fu, 0x01090503u, 0x01090509u, 0x01090705u, 0x01090901u,
    0x01090907u, 0x01090b03u, 0x01090f01u, 0x010b0105u, 0x010b0109u, 0x010b0501u, 0x010b0505u, 0x010b050du,
    0x010b0707u, 0x010b0903u, 0x010b090bu, 0x010b090fu, 0x010b0d0du, 0x010b0f07u, 0x010d010du, 0x010d0303u,
    0x010d0307u, 0x010d0703u, 0x010d0b05u, 0x010d0f03u, 0x010f0101u, 0x010f0105u, 0x010f0109u, 0x010f0501u,
    0x010f0505u, 0x010f050du, 0x010f0707u, 0x010f0b01u, 0x010f0b09u, 0x03010101u, 0x03010103u, 0x03010105u,
    0x03010109u, 0x03010301u, 0x03010303u, 0x03010307u, 0x0301030bu, 0x0301030fu, 0x03010501u, 0x03010505u,
    0x03010703u, 0x03010709u, 0x0301070du, 0x03010b09u, 0x03010b0du, 0x03010d03u, 0x03010f05u, 0x03030101u,
    0x03030103u, 0x03030107u, 0x0303010du, 0x03030301u, 0x03030309u, 0x03030503u, 0x03030701u, 0x03030707u,
    0x03030903u, 0x03030b01u, 0x03030b05u, 0x03030f01u, 0x03030f0du, 0x03050101u, 0x03050305u, 0x0305030bu,
    0x0305030fu, 0x03050501u, 0x03050509u, 0x03050705u, 0x03050901u, 0x03050907u, 0x03050b0bu, 0x03050d01u,
    0x03050f05u, 0x03070103u, 0x03070109u, 0x0307010fu, 0x03070301u, 0x03070307u, 0x03070503u, 0x0307050fu,
    0x03070701u, 0x03070709u, 0x03070903u, 0x03070d05u, 0x03070f01u, 0x03090107u, 0x0309010bu, 0x03090305u,
    0x03090309u, 0x03090703u, 0x03090707u, 0x03090905u, 0x0309090du, 0x03090b01u, 0x03090b09u, 0x030b0103u,
    0x030b0301u, 0x030b0307u, 0x030b0503u, 0x030b0701u, 0x030b0705u, 0x030b0b03u, 0x030d0501u, 0x030d0509u,
    0x030d050fu, 0x030d0909u, 0x030d090du, 0x030f0103u, 0x030f0107u, 0x030f0301u, 0x030f0305u, 0x030f0503u,
    0x030f070bu, 0x030f0903u, 0x030f0d05u, 0x030f0f01u, 0x05010101u, 0x05010103u, 0x05010107u, 0x0501010bu,
    0x0501010fu, 0x05010301u, 0x05010305u, 0x05010309u, 0x0501030du, 0x05010503u, 0x05010507u, 0x0501050fu,
    0x05010701u, 0x05010705u, 0x05010903u, 0x05010907u, 0x0501090bu, 0x05010b01u, 0x05010b05u, 0x05010d0fu,
    0x05010f01u, 0x05010f07u, 0x05010f0bu, 0x05030101u, 0x05030105u, 0x05030301u, 0x05030307u, 0x0503030fu,
    0x05030505u, 0x0503050bu, 0x05030703u, 0x05030709u, 0x05030905u, 0x05030b03u, 0x05050103u, 0x05050109u,
    0x0505010fu, 0x05050503u, 0x05050507u, 0x05050701u, 0x0505070fu, 0x05050903u, 0x05050b07u, 0x05050b0fu,
    0x05050f03u, 0x05050f09u, 0x05070101u, 0x05070105u, 0x0507010bu, 0x05070303u, 0x05070505u, 0x05070509u,
    0x05070703u, 0x05070707u, 0x05070905u, 0x05070b01u, 0x05070d0du, 0x05090103u, 0x0509010fu, 0x05090501u,
    0x05090507u, 0x05090705u, 0x0509070bu, 0x05090903u, 0x05090f05u, 0x05090f0bu, 0x050b0109u, 0x050b0303u,
    0x050b0505u, 0x050b070fu, 0x050b0901u, 0x050b0b07u, 0x050b0f01u, 0x050d0101u, 0x050d0105u, 0x050d010fu,
    0x050d0503u, 0x050d0b0bu, 0x050d0d03u, 0x050f010bu, 0x050f0303u, 0x050f050du, 0x050f0701u, 0x050f0907u,
    0x050f0b01u, 0x07010105u, 0x07010303u, 0x07010307u, 0x0701030bu, 0x0701030fu, 0x07010505u, 0x07010703u,
    0x07010707u, 0x0701070bu, 0x07010905u, 0x07010909u, 0x0701090fu, 0x07010b03u, 0x07010d07u, 0x07010f03u,
    0x07030103u, 0x07030107u, 0x0703010bu, 0x07030309u, 0x07030503u, 0x07030507u, 0x07030901u, 0x07030d01u,
    0x07030f05u, 0x07030f0du, 0x07050101u, 0x07050305u, 0x07050501u, 0x07050705u, 0x07050709u, 0x07050b01u,
    0x07070103u, 0x07070301u, 0x07070309u, 0x07070503u, 0x07070507u, 0x0707050fu, 0x07070701u, 0x07070903u,
    0x07070907u, 0x0707090fu, 0x07070b0bu, 0x07070f07u, 0x07090107u, 0x07090303u, 0x0709030du, 0x07090505u,
    0x07090703u, 0x07090b05u, 0x07090d01u, 0x07090d09u, 0x070b0103u, 0x070b0301u, 0x070b0305u, 0x070b050bu,
    0x070b0705u, 0x070b0909u, 0x070b0b0du, 0x070b0f07u, 0x070d030du, 0x070d0903u, 0x070f0103u, 0x070f0107u,
    0x070f0501u, 0x070f0505u, 0x070f070bu, 0x09010101u, 0x09010109u, 0x09010305u, 0x09010501u, 0x09010509u,
    0x0901050fu, 0x09010705u, 0x09010903u, 0x09010b01u, 0x09010f01u, 0x09030105u, 0x0903010fu, 0x09030303u,
    0x09030307u, 0x09030505u, 0x09030701u, 0x0903070bu, 0x09030907u, 0x09030b03u, 0x09030b0bu, 0x09050103u,
    0x09050107u, 0x09050301u, 0x0905030bu, 0x09050503u, 0x09050707u, 0x09050901u, 0x09050b0fu, 0x09050d05u,
    0x09050f01u, 0x09070109u, 0x09070303u, 0x09070307u, 0x09070501u, 0x09070505u, 0x09070703u, 0x0907070bu,
    0x09090101u, 0x09090105u, 0x09090509u, 0x0909070fu, 0x09090901u, 0x09090f03u, 0x090b010bu, 0x090b010fu,
    0x090b0503u, 0x090b0d05u, 0x090d0307u, 0x090d0709u, 0x090d0d01u, 0x090f0301u, 0x090f030bu, 0x090f0701u,
    0x090f0907u, 0x090f0b03u, 0x0b010105u, 0x0b010301u, 0x0b010309u, 0x0b010505u, 0x0b010901u, 0x0b010909u,
    0x0b01090fu, 0x0b010b05u, 0x0b010d0du, 0x0b010f09u, 0x0b030103u, 0x0b030107u, 0x0b03010bu, 0x0b030305u,
    0x0b030503u, 0x0b030705u, 0x0b030f05u, 0x0b050101u, 0x0b050303u, 0x0b050507u, 0x0b050701u, 0x0b05070du,
    0x0b050b07u, 0x0b070105u, 0x0b07010fu, 0x0b070301u, 0x0b07050fu, 0x0b070909u, 0x0b070b03u, 0x0b070d0bu,
    0x0b070f07u, 0x0b090103u, 0x0b090109u, 0x0b090501u, 0x0b090705u, 0x0b09090du, 0x0b0b0305u, 0x0b0b050du,
    0x0b0b0b03u, 0x0b0b0b07u, 0x0b0d0905u, 0x0b0f0105u, 0x0b0f0109u, 0x0b0f0505u, 0x0d010303u, 0x0d010307u,
    0x0d01030bu, 0x0d010703u, 0x0d010707u, 0x0d010d01u, 0x0d030101u, 0x0d030501u, 0x0d03050fu, 0x0d030d09u,
    0x0d050305u, 0x0d050709u, 0x0d050905u, 0x0d050b0bu, 0x0d050d05u, 0x0d050f01u, 0x0d070101u, 0x0d070309u,
    0x0d070503u, 0x0d070901u, 0x0d09050bu, 0x0d090907u, 0x0d090d05u, 0x0d0b0101u, 0x0d0b0107u, 0x0d0b0709u,
    0x0d0b0d01u, 0x0d0d010bu, 0x0d0d0901u, 0x0d0f0303u, 0x0d0f0307u, 0x0f010101u, 0x0f010109u, 0x0f01010fu,
    0x0f010501u, 0x0f010505u, 0x0f01070du, 0x0f010901u, 0x0f010b09u, 0x0f010d05u, 0x0f030105u, 0x0f030303u,
    0x0f030509u, 0x0f030907u, 0x0f03090bu, 0x0f050103u, 0x0f050109u, 0x0f050301u, 0x0f05030du, 0x0f050503u,
    0x0f050701u, 0x0f050b03u, 0x0f070105u, 0x0f070705u, 0x0f07070bu, 0x0f070b07u, 0x0f090103u, 0x0f09010bu,
    0x0f090307u, 0x0f090501u, 0x0f090b01u, 0x0f0b0505u, 0x0f0b0905u, 0x0f0d0105u, 0x0f0d0703u, 0x0f0f0101u,
};

static float iq3_s_step(float acc, const float *x, const unsigned char *b) {
  float d = quant_half(b, 0);
  for (unsigned group = 0; group < 4; group++) {
    for (unsigned side = 0; side < 2; side++) {
      unsigned sc = (unsigned)b[106 + group];
      float db = d * (float)(1 + 2 * (int)(side ? (sc >> 4) : (sc & 15)));
      for (unsigned l = 0; l < 4; l++) {
        unsigned mask = (unsigned)b[66 + 2 * group + side];
        unsigned signs = (unsigned)b[74 + 8 * group + 4 * side + l];
        for (unsigned w = 0; w < 8; w++) {
          unsigned qs = (unsigned)b[2 + 16 * group + 8 * side + 2 * l + (w >> 2)];
          unsigned index = qs | ((mask << (8 - 2 * l - (w >> 2))) & 256);
          float weight = db * (float)((IQ3S_GRID[index] >> (8 * (w & 3))) & 255);
          if ((signs >> w) & 1) weight = -weight;
          acc = quant_add(acc, x[group * 64 + side * 32 + l * 8 + w], weight);
        }
      }
    }
  }
  return acc;
}

// One block format the product decodes: its size, and the step that turns one
// block into products in the running sum.
typedef struct {
  const char *name;
  const char *kernel;
  int64_t elements;
  int64_t bytes;
  float (*step)(float acc, const float *x, const unsigned char *block);
} QuantFormat;

static const QuantFormat QUANT_FORMATS[] = {
    {"Q4_K", "osprey_matmul_q4_k", 256, 144, q4k_step},
    {"Q8_0", "osprey_matmul_q8_0", 32, 34, q8_0_step},
    {"Q6_K", "osprey_matmul_q6_k", 256, 210, q6_k_step},
    {"Q5_K", "osprey_matmul_q5_k", 256, 176, q5_k_step},
    {"Q3_K", "osprey_matmul_q3_k", 256, 110, q3_k_step},
    {"IQ4_NL", "osprey_matmul_iq4_nl", 32, 18, iq4_nl_step},
    {"IQ4_XS", "osprey_matmul_iq4_xs", 256, 136, iq4_xs_step},
    {"IQ3_S", "osprey_matmul_iq3_s", 256, 110, iq3_s_step},
};
#define QUANT_FORMAT_COUNT (sizeof QUANT_FORMATS / sizeof QUANT_FORMATS[0])

// The block format `type` names, or NULL for one the product does not decode.
static const QuantFormat *quant_format(const char *type) {
  for (size_t i = 0; type && i < QUANT_FORMAT_COUNT; i++)
    if (strcmp(type, QUANT_FORMATS[i].name) == 0) return &QUANT_FORMATS[i];
  return NULL;
}

// How many threads share one output: more as the product gets smaller, so a
// few rows of x against a weight matrix still fill the device. A pure
// function of the shape, so the host and every device add in the same order.
static int64_t quant_parts(int64_t rows, int64_t cols) {
  int64_t outputs = rows * cols;
  return outputs > ((int64_t)1 << 20) ? 1 : outputs > ((int64_t)1 << 16) ? 4 : 16;
}

// The blocks part `p` of an output covers, summed from 0.0f by the format's step.
static float part_blocks(const QuantFormat *format, const float *xr, const unsigned char *row, int64_t first,
                         int64_t last) {
  float acc = 0.0f;
  for (int64_t blk = first; blk < last; blk++)
    acc = format->step(acc, xr + blk * format->elements, row + blk * format->bytes);
  return acc;
}

// The oracle of the library kernels in gpu_device_lib.metal, step for step:
// part p of an output covers the blocks from p * blocks / parts up to
// (p + 1) * blocks / parts, and the parts are added in order from 0.0f.
static void product_quant(const QuantFormat *format, float *out, const float *x, const unsigned char *w, int64_t rows,
                          int64_t inner, int64_t cols) {
  int64_t blocks = inner / format->elements, parts = quant_parts(rows, cols);
  for (int64_t r = 0; r < rows; r++) for (int64_t c = 0; c < cols; c++) {
    float acc = 0.0f;
    for (int64_t p = 0; p < parts; p++)
      acc = acc + part_blocks(format, x + r * inner, w + c * blocks * format->bytes, p * blocks / parts,
                              (p + 1) * blocks / parts);
    out[r * cols + c] = acc;
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
    char reason[128], names[96];
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
  void *out = osprey_gpu_alloc(n, 4);
  if (!out || osprey_gpu_len(out) != n) {
    osp_io_error_set(OP_MATMUL_QUANT, "result", ENOMEM);
    return abandoned(out);
  }
  int device = osprey_gpu_device_matmul_quant(format->kernel, x, w, out, rows, inner, cols, quant_parts(rows, cols));
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

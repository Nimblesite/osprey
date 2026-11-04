#ifndef OSPREY_FLOAT_SPELLING_H
#define OSPREY_FLOAT_SPELLING_H

#include <stddef.h>
#include <stdint.h>

/* The %g spelling toString(float) gives `d`: the smallest precision from 10
 * to 17 whose spelling reads back as `d` (NaN and infinities at 17). Returns
 * the length written into `buf`, or a negative value / one >= `cap` when it
 * did not fit, exactly as snprintf does. Allocates nothing. */
int osp_float_spelling(double d, char *buf, size_t cap);

/* IEEE binary16 conversion and shortest width-specific spellings
 * [TYPE-NUMERIC-SIZED] [TYPE-NUMERIC-PRINT]. These allocate nothing. */
uint16_t osp_float_to_f16(double value);
float osp_f16_to_f32(uint16_t bits);
int osp_float32_spelling(float value, char *buf, size_t cap);
int osp_float16_spelling(uint16_t bits, char *buf, size_t cap);

#endif /* OSPREY_FLOAT_SPELLING_H */

// The random 64-bit words and bit casts the GPU device-math suites share: one
// xorshift stream per suite (seeded identically, so runs are reproducible)
// and the double <-> word reinterpretations every oracle compares through.
#ifndef OSPREY_GPU_TEST_WORDS_H
#define OSPREY_GPU_TEST_WORDS_H

#include <stdint.h>
#include <string.h>

static uint64_t state = 0x9e3779b97f4a7c15;

static inline uint64_t random_word(void) {
  state ^= state << 13;
  state ^= state >> 7;
  state ^= state << 17;
  return state;
}

static inline double of(uint64_t word) {
  double d;
  memcpy(&d, &word, sizeof d);
  return d;
}

static inline uint64_t bits(double d) {
  uint64_t word;
  memcpy(&word, &d, sizeof word);
  return word;
}

#endif

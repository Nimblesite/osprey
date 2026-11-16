// The Prism synth's player on Windows [UI-MACHINE-FLOW-SYNTH]. Windows has no afplay, and the
// sandbox may not write a clip anywhere afplay's caller would, so this program takes the whole WAV
// as one hex argument, checks it is a RIFF WAVE whose sizes agree, and plays it from memory with
// PlaySound, returning when the clip ends. The synth names it with `--play` and the host runs it.
// Exit status: 0 played, 1 the argument is not a clip, 2 Windows would not play it.
// Build: gcc -O2 play-wav.c -o play-wav.exe -lwinmm
#include <windows.h>
#include <mmsystem.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

// A second of 8-bit mono at 11025 Hz is 11069 bytes; this bounds the clip far above that.
#define MAX_CLIP_BYTES (1024 * 1024)
#define WAV_HEADER_BYTES 44
#define RIFF_PREAMBLE_BYTES 8

// The value of one hex digit, or -1 when `c` is not one.
static int nibble(char c) {
  if (c >= '0' && c <= '9') return c - '0';
  if (c >= 'a' && c <= 'f') return c - 'a' + 10;
  if (c >= 'A' && c <= 'F') return c - 'A' + 10;
  return -1;
}

// The little-endian 32-bit value at `at`; the caller has checked four bytes are there.
static unsigned long le32(const unsigned char *at) {
  return (unsigned long)at[0] | ((unsigned long)at[1] << 8) | ((unsigned long)at[2] << 16) | ((unsigned long)at[3] << 24);
}

// `hex` decoded into a new buffer of `*size` bytes, or NULL with the reason on stderr.
static unsigned char *decoded(const char *hex, size_t *size) {
  size_t digits = strlen(hex);
  if (digits == 0 || digits % 2 != 0 || digits / 2 > MAX_CLIP_BYTES) {
    fprintf(stderr, "play-wav: the clip must be an even number of hex digits, at most %d bytes\n", MAX_CLIP_BYTES);
    return NULL;
  }
  *size = digits / 2;
  unsigned char *bytes = malloc(*size);
  if (!bytes) {
    fprintf(stderr, "play-wav: no memory for the clip\n");
    return NULL;
  }
  for (size_t i = 0; i < *size; i++) {
    int high = nibble(hex[2 * i]);
    int low = nibble(hex[2 * i + 1]);
    if (high < 0 || low < 0) {
      fprintf(stderr, "play-wav: the clip is not hex at digit %zu\n", 2 * i);
      free(bytes);
      return NULL;
    }
    bytes[i] = (unsigned char)(high << 4 | low);
  }
  return bytes;
}

// True when `bytes` is a whole RIFF WAVE: its tags in place and its RIFF size the bytes that follow.
static int is_wave(const unsigned char *bytes, size_t size) {
  return size >= WAV_HEADER_BYTES && memcmp(bytes, "RIFF", 4) == 0 && memcmp(bytes + 8, "WAVE", 4) == 0 &&
         le32(bytes + 4) == size - RIFF_PREAMBLE_BYTES;
}

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "usage: play-wav <clip as hex>\n");
    return 1;
  }
  size_t size = 0;
  unsigned char *clip = decoded(argv[1], &size);
  if (!clip) return 1;
  if (!is_wave(clip, size)) {
    fprintf(stderr, "play-wav: the clip is not a RIFF WAVE whose size matches its %zu bytes\n", size);
    free(clip);
    return 1;
  }
  BOOL played = PlaySoundA((LPCSTR)clip, NULL, SND_MEMORY | SND_SYNC | SND_NODEFAULT);
  free(clip);
  if (!played) {
    fprintf(stderr, "play-wav: Windows would not play the clip\n");
    return 2;
  }
  return 0;
}

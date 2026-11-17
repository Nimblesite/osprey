// [SYNTH-DEVICE-QUEUE] Feed the modular Prism graph's bounded WAV blocks to WinMM.
#include <windows.h>
#include <mmsystem.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

enum { Rate = 48000, MaxFrames = 8192, Channels = 2, Slots = 4, Header = 58,
       MaxBytes = Header + MaxFrames * Channels * 4, WaitMs = 5000 };

typedef struct {
  WAVEHDR header;
  int16_t samples[MaxFrames * Channels];
  uint32_t frames;
} Slot;

typedef struct {
  HWAVEOUT output;
  HANDLE changed;
  Slot slots[Slots];
  uint32_t channels, queued, empty;
  uint64_t accepted, returned;
  int running;
} Device;

static int fail(const char *message) {
  fprintf(stderr, "Prism audio: %s\n", message);
  return 0;
}

static int nibble(unsigned char c) {
  if (c >= '0' && c <= '9') return c - '0';
  if (c >= 'A' && c <= 'F') return c - 'A' + 10;
  if (c >= 'a' && c <= 'f') return c - 'a' + 10;
  return -1;
}

static size_t decode(const char *line, uint8_t *bytes) {
  size_t digits = strcspn(line, "\r\n");
  if (digits % 2 || digits > MaxBytes * 2 || strcmp(line + digits, "\n")) return 0;
  for (size_t i = 0; i < digits / 2; ++i) {
    int high = nibble((unsigned char)line[i * 2]);
    int low = nibble((unsigned char)line[i * 2 + 1]);
    if (high < 0 || low < 0) return 0;
    bytes[i] = (uint8_t)(high * 16 + low);
  }
  return digits / 2;
}

static uint32_t word(const uint8_t *bytes, size_t offset, size_t width) {
  uint32_t value = 0;
  for (size_t i = 0; i < width; ++i) value |= (uint32_t)bytes[offset + i] << (i * 8);
  return value;
}

static int header_ok(const uint8_t *data, size_t size) {
  return size >= Header && !memcmp(data, "RIFF", 4) && word(data, 4, 4) == size - 8 &&
         !memcmp(data + 8, "WAVEfmt ", 8) && word(data, 16, 4) == 18 &&
         word(data, 20, 2) == 3 && word(data, 24, 4) == Rate && word(data, 34, 2) == 32 &&
         word(data, 36, 2) == 0 && !memcmp(data + 38, "fact", 4) &&
         word(data, 42, 4) == 4 && !memcmp(data + 50, "data", 4);
}

static int layout_ok(const uint8_t *data, size_t size, uint32_t channels, uint32_t frames) {
  return channels >= 1 && channels <= Channels && frames >= 1 && frames <= MaxFrames &&
         word(data, 28, 4) == Rate * channels * 4 && word(data, 32, 2) == channels * 4 &&
         word(data, 54, 4) == frames * channels * 4 && size == Header + frames * channels * 4;
}

static int samples(Slot *slot, const uint8_t *data, uint32_t count) {
  for (uint32_t i = 0; i < count; ++i) {
    uint32_t bits = word(data, Header + i * 4, 4);
    float value;
    memcpy(&value, &bits, sizeof(value));
    if (!isfinite(value) || value < -1.0f || value > 1.0f) return fail("Invalid PCM sample");
    slot->samples[i] = (int16_t)(value * 32767.0f);
  }
  return 1;
}

static int open_output(Device *device, uint32_t channels) {
  device->changed = CreateEventW(NULL, FALSE, FALSE, NULL);
  if (!device->changed) return fail("Cannot create device notification");
  WAVEFORMATEX format = {WAVE_FORMAT_PCM, (WORD)channels, Rate, Rate * channels * 2,
                         (WORD)(channels * 2), 16, 0};
  if (waveOutOpen(&device->output, WAVE_MAPPER, &format, (DWORD_PTR)device->changed, 0,
                  CALLBACK_EVENT) != MMSYSERR_NOERROR) return fail("Cannot open the Windows audio output");
  device->channels = channels;
  return waveOutPause(device->output) == MMSYSERR_NOERROR || fail("Cannot prime Windows audio");
}

static int reap(Device *device) {
  for (size_t i = 0; i < Slots; ++i) {
    Slot *slot = &device->slots[i];
    if (!slot->frames || !(slot->header.dwFlags & WHDR_DONE)) continue;
    if (waveOutUnprepareHeader(device->output, &slot->header, sizeof(slot->header)) != MMSYSERR_NOERROR)
      return fail("Cannot reclaim a Windows audio buffer");
    device->returned += slot->frames;
    device->queued--;
    slot->frames = 0;
  }
  return 1;
}

static int wait_available(Device *device, uint32_t below) {
  ULONGLONG deadline = GetTickCount64() + WaitMs;
  while (1) {
    if (!reap(device)) return 0;
    if (device->queued < below) return 1;
    ULONGLONG now = GetTickCount64();
    if (now >= deadline || WaitForSingleObject(device->changed, (DWORD)(deadline - now)) != WAIT_OBJECT_0)
      return fail("Windows audio did not return its buffers");
  }
}

static int start_output(Device *device) {
  if (device->running || !device->queued) return 1;
  if (waveOutRestart(device->output) != MMSYSERR_NOERROR) return fail("Cannot start Windows audio");
  device->running = 1;
  return 1;
}

static int enqueue(Device *device, Slot *slot, uint32_t channels, uint32_t frames) {
  slot->header = (WAVEHDR){.lpData = (LPSTR)slot->samples, .dwBufferLength = frames * channels * 2};
  if (waveOutPrepareHeader(device->output, &slot->header, sizeof(slot->header)) != MMSYSERR_NOERROR)
    return fail("Cannot prepare a Windows audio buffer");
  if (waveOutWrite(device->output, &slot->header, sizeof(slot->header)) != MMSYSERR_NOERROR) {
    waveOutUnprepareHeader(device->output, &slot->header, sizeof(slot->header));
    return fail("Cannot queue a Windows audio buffer");
  }
  slot->frames = frames;
  device->queued++;
  device->accepted += frames;
  return device->queued < 2 || start_output(device);
}

static int push(Device *device, const uint8_t *data, size_t size) {
  if (!header_ok(data, size)) return fail("Invalid float32 WAV header");
  uint32_t channels = word(data, 22, 2), frames = word(data, 46, 4);
  if (!layout_ok(data, size, channels, frames)) return fail("Invalid float32 WAV layout");
  if (!device->output && !open_output(device, channels)) return 0;
  if (channels != device->channels) return fail("The graph's audio channel count changed");
  if (!wait_available(device, Slots)) return 0;
  if (device->running && !device->queued) device->empty++;
  for (size_t i = 0; i < Slots; ++i) {
    Slot *slot = &device->slots[i];
    if (!slot->frames) return samples(slot, data, frames * channels) && enqueue(device, slot, channels, frames);
  }
  return fail("No Windows audio buffer is available");
}

static int drain(Device *device) {
  if (!start_output(device) || !wait_available(device, 1)) return 0;
  device->running = 0;
  return 1;
}

static void close_output(Device *device) {
  if (device->output) {
    waveOutReset(device->output);
    reap(device);
    waveOutClose(device->output);
  }
  if (device->changed) CloseHandle(device->changed);
}

static void receipt(const Device *device) {
  printf("{\"status\":0,\"acceptedFrames\":%llu,\"returnedFrames\":%llu,"
         "\"queuedFrames\":%llu,\"channels\":%lu,\"emptyCallbacks\":%lu,\"running\":%s}\n",
         (unsigned long long)device->accepted, (unsigned long long)device->returned,
         (unsigned long long)(device->accepted - device->returned), (unsigned long)device->channels,
         (unsigned long)device->empty, device->running ? "true" : "false");
  fflush(stdout);
}

int main(int argc, char **argv) {
  static Device device;
  static char line[MaxBytes * 2 + 3];
  static uint8_t bytes[MaxBytes];
  if (argc != 2 || strcmp(argv[1], "audio-device")) return !fail("Expected audio-device");
  while (fgets(line, sizeof(line), stdin)) {
    int ok = !strcmp(line, "!drain\n") ? drain(&device) : push(&device, bytes, decode(line, bytes));
    if (!ok) { close_output(&device); return 1; }
    receipt(&device);
  }
  close_output(&device);
  return ferror(stdin) || ferror(stdout);
}

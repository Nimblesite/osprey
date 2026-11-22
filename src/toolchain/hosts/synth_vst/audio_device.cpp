// [SYNTH-DEVICE-QUEUE] Producer-side decoding; callbacks only return preallocated buffers.
#include "hex_text.h"
#include <algorithm>
#include <array>
#include <atomic>
#include <chrono>
#include <cmath>
#include <cstring>
#include <iostream>
#include <limits>
#include <thread>
#include <vector>
#if defined(__APPLE__)
#include <AudioToolbox/AudioToolbox.h>
#endif

std::string quoted(const std::string&);
namespace {
int failure(const std::string& message) {
    std::cerr << "{\"event\":\"audio.failure\",\"message\":" << quoted(message) << "}\n";
    return 1;
}

#if defined(__APPLE__)
constexpr uint32_t MaxFrames = 8192;
constexpr uint32_t Rate = 48000;
struct Wave { uint32_t channels = 0, frames = 0; std::vector<float> samples; };

uint32_t word(const std::string& bytes, size_t at, size_t count = 4) {
    uint32_t result = 0;
    for (size_t i = 0; i < count; ++i) result |= uint32_t(uint8_t(bytes[at + i])) << (8 * i);
    return result;
}

bool wave(const std::string& bytes, Wave& result) {
    if (bytes.size() < 58 || bytes.compare(0, 4, "RIFF") || bytes.compare(8, 8, "WAVEfmt ") ||
        word(bytes, 4) != bytes.size() - 8 || word(bytes, 16) != 18 || word(bytes, 20, 2) != 3 ||
        word(bytes, 24) != Rate || word(bytes, 34, 2) != 32 || word(bytes, 36, 2) != 0 ||
        bytes.compare(38, 4, "fact") || word(bytes, 42) != 4 || bytes.compare(50, 4, "data")) return false;
    result.channels = word(bytes, 22, 2); result.frames = word(bytes, 46);
    if (result.channels < 1 || result.channels > 2 || result.frames < 1 || result.frames > MaxFrames ||
        word(bytes, 28) != Rate * result.channels * 4 || word(bytes, 32, 2) != result.channels * 4 ||
        word(bytes, 54) != result.frames * result.channels * 4 || bytes.size() != 58 + word(bytes, 54)) return false;
    result.samples.resize(result.frames * result.channels);
    for (size_t i = 0; i < result.samples.size(); ++i) {
        const auto bits = word(bytes, 58 + i * 4);
        std::memcpy(&result.samples[i], &bits, sizeof(float));
        if (!std::isfinite(result.samples[i]) || std::abs(result.samples[i]) > 1.0f) return false;
    }
    return true;
}

class Device {
    struct Slot {
        AudioQueueBufferRef buffer = nullptr;
        std::atomic<bool> available{true};
        uint32_t frames = 0;
    };
    AudioQueueRef queue = nullptr;
    static constexpr size_t MaxLivePackets = 9;
    static constexpr uint32_t LiveQueueFrames = 960; // 20 ms at 48 kHz.
    static constexpr uint32_t RecoveryQueueFrames = 1440;
    static constexpr uint32_t RecoveryStableFrames = Rate / 10; // 100 ms of submitted audio.
    std::array<Slot, MaxLivePackets> slots;
    std::atomic<uint64_t> returned{0}, accepted{0}, emptyCallbacks{0};
    std::atomic<bool> draining{false};
    uint32_t channels = 0;
    uint64_t reportedEmpty = 0, lastEmptyFrame = 0;
    bool running = false, finished = false;
    std::string error;

    static void consumed(void* context, AudioQueueRef, AudioQueueBufferRef buffer) {
        auto& device = *static_cast<Device*>(context);
        auto& slot = *static_cast<Slot*>(buffer->mUserData);
        const auto count = device.returned.fetch_add(slot.frames, std::memory_order_release) + slot.frames;
        if (!device.draining.load(std::memory_order_relaxed) && count == device.accepted.load(std::memory_order_acquire))
            device.emptyCallbacks.fetch_add(1, std::memory_order_relaxed);
        slot.available.store(true, std::memory_order_release);
    }

    bool checked(OSStatus status, const char* operation) {
        if (status == noErr) return true;
        error = std::string(operation) + " failed (" + std::to_string(status) + ")";
        return false;
    }

    bool start() {
        if (running) return true;
        if (!checked(AudioQueueStart(queue, nullptr), "AudioQueueStart")) return false;
        running = true;
        std::cerr << "{\"event\":\"audio.started\",\"rate\":48000,\"channels\":" << channels << "}\n";
        return true;
    }

    bool open(uint32_t width) {
        if (queue) return channels == width || rejected("Output channel layout changed during playback");
        if (!returned.is_lock_free() || !accepted.is_lock_free() || !emptyCallbacks.is_lock_free() ||
            !draining.is_lock_free() || !slots[0].available.is_lock_free()) return rejected("Audio atomics are not lock-free");
        channels = width;
        AudioStreamBasicDescription format{};
        format.mSampleRate = Rate; format.mFormatID = kAudioFormatLinearPCM;
        format.mFormatFlags = kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked;
        format.mBytesPerPacket = format.mBytesPerFrame = channels * sizeof(float);
        format.mFramesPerPacket = 1; format.mChannelsPerFrame = channels; format.mBitsPerChannel = 32;
        if (!checked(AudioQueueNewOutput(&format, consumed, this, nullptr, nullptr, 0, &queue), "AudioQueueNewOutput")) return false;
        for (auto& slot : slots) {
            if (!checked(AudioQueueAllocateBuffer(queue, MaxFrames * 2 * sizeof(float), &slot.buffer), "AudioQueueAllocateBuffer")) return false;
            slot.buffer->mUserData = &slot;
        }
        return true;
    }

    Slot* available(uint32_t frames) {
        const auto sent = accepted.load(std::memory_order_relaxed);
        const auto recovering = reportedEmpty && sent - lastEmptyFrame < RecoveryStableFrames;
        const auto budget = recovering ? RecoveryQueueFrames : LiveQueueFrames;
        const auto count = std::clamp<size_t>((budget + frames - 1) / frames, 4, slots.size());
        const auto beforeEnqueue = (count - 1) * frames;
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(2);
        do {
            // Recovery slots can still be occupied when the normal limit resumes.
            if (!running || sent - returned.load(std::memory_order_acquire) <= beforeEnqueue) {
                for (size_t i = 0; i < count; ++i)
                    if (slots[i].available.load(std::memory_order_acquire)) return &slots[i];
            }
            std::this_thread::sleep_for(std::chrono::microseconds(250));
        } while (std::chrono::steady_clock::now() < deadline);
        error = "Audio output did not return a buffer within two seconds";
        return nullptr;
    }

    bool rejected(const char* message) { error = message; return false; }
public:
    ~Device() { draining.store(true); if (queue) AudioQueueDispose(queue, true); }
    const std::string& problem() const { return error; }

    bool push(const Wave& block) {
        if (finished) return rejected("The audio stream has already drained");
        if (accepted.load() > std::numeric_limits<uint64_t>::max() - block.frames) return rejected("Audio sample clock exhausted");
        if (!open(block.channels)) return false;
        auto* slot = available(block.frames);
        if (!slot) return false;
        slot->available.store(false, std::memory_order_relaxed);
        slot->frames = block.frames;
        slot->buffer->mAudioDataByteSize = uint32_t(block.samples.size() * sizeof(float));
        std::memcpy(slot->buffer->mAudioData, block.samples.data(), slot->buffer->mAudioDataByteSize);
        accepted.fetch_add(block.frames, std::memory_order_release);
        if (!checked(AudioQueueEnqueueBuffer(queue, slot->buffer, 0, nullptr), "AudioQueueEnqueueBuffer")) return false;
        if (running) return true;
        // Two producer blocks prepare the device before its first callback.
        unsigned queued = 0;
        for (auto& held : slots) if (!held.available.load(std::memory_order_acquire)) ++queued;
        return queued < 2 || start();
    }

    bool drain() {
        if (!queue || finished) return true;
        draining.store(true, std::memory_order_relaxed);
        if (!start()) return false;
        const auto deadline = std::chrono::steady_clock::now() + std::chrono::seconds(3);
        while (returned.load(std::memory_order_acquire) != accepted.load(std::memory_order_acquire)) {
            if (std::chrono::steady_clock::now() >= deadline) return rejected("Audio output did not drain within three seconds");
            std::this_thread::sleep_for(std::chrono::milliseconds(1));
        }
        if (!checked(AudioQueueStop(queue, true), "AudioQueueStop")) return false;
        running = false; finished = true;
        std::cerr << "{\"event\":\"audio.drained\",\"frames\":" << returned.load() << ",\"emptyCallbacks\":" << emptyCallbacks.load() << "}\n";
        return true;
    }

    void receipt() {
        const auto sent = accepted.load(std::memory_order_acquire);
        const auto empty = emptyCallbacks.load(std::memory_order_acquire);
        if (empty > reportedEmpty) {
            std::cerr << "{\"event\":\"audio.queue_empty\",\"count\":" << empty
                      << ",\"acceptedFrames\":" << sent << "}\n";
            reportedEmpty = empty;
            lastEmptyFrame = sent;
        }
        const auto done = returned.load(std::memory_order_acquire);
        std::cout << "{\"status\":0,\"acceptedFrames\":" << sent << ",\"returnedFrames\":" << done
                  << ",\"queuedFrames\":" << sent - done << ",\"channels\":" << channels
                  << ",\"emptyCallbacks\":" << empty << ",\"running\":" << (running ? "true" : "false") << "}\n" << std::flush;
    }
};
#endif
}

int audioDevice() {
#if defined(__APPLE__)
    Device device;
    while (std::cin.peek() != std::char_traits<char>::eof()) {
        if (std::cin.peek() == '!') {
            std::array<char, 16> command{};
            if (!std::cin.getline(command.data(), command.size()) || std::string(command.data()) != "!drain") return failure("Unknown audio command");
            if (!device.drain()) return failure(device.problem());
        } else {
            std::string bytes; Wave block;
            if (!readHex(std::cin, bytes, true) || !wave(bytes, block)) return failure("Invalid bounded 48 kHz float32 WAV block; output must be within -1..1");
            if (!device.push(block)) return failure(device.problem());
        }
        device.receipt();
        if (!std::cout) return failure("Audio receipt delivery failed");
    }
    if (std::cin.bad() || !device.drain()) return failure("Audio input or drain failed");
    return 0;
#else
    return failure("This audio device adapter currently requires macOS");
#endif
}

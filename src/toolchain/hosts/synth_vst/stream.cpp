// [SYNTH-HOST-STREAM] Bounded little-endian blocks over a persistent disposable worker.
#include "processor.h"
#include <bit>
#include <limits>
#ifdef _WIN32
#include <fcntl.h>
#include <io.h>
#endif

namespace osprey::vst {
namespace {
bool word(uint32_t& value) {
    unsigned char bytes[4]{};
    if (!std::cin.read(reinterpret_cast<char*>(bytes), sizeof(bytes))) return false;
    value = uint32_t(bytes[0]) | uint32_t(bytes[1]) << 8 | uint32_t(bytes[2]) << 16 | uint32_t(bytes[3]) << 24;
    return true;
}

void written(uint32_t value) {
    const unsigned char bytes[]{static_cast<unsigned char>(value), static_cast<unsigned char>(value >> 8),
        static_cast<unsigned char>(value >> 16), static_cast<unsigned char>(value >> 24)};
    std::cout.write(reinterpret_cast<const char*>(bytes), sizeof(bytes));
}

bool real(float& value) {
    uint32_t bits = 0;
    if (!word(bits)) return false;
    value = std::bit_cast<float>(bits);
    return std::isfinite(value) && std::abs(value) <= 1000000;
}

bool normalized(double& value) {
    uint32_t low = 0, high = 0;
    if (!word(low) || !word(high)) return false;
    value = std::bit_cast<double>((uint64_t(high) << 32) | low);
    return std::isfinite(value) && value >= 0 && value <= 1;
}

bool event(EventList& events, uint32_t frames, uint32_t& previous) {
    uint32_t kind = 0, offset = 0, pitch = 0, channel = 0, identity = 0;
    float velocity = 0;
    if (!word(kind) || !word(offset) || !word(pitch) || !word(channel) || !word(identity) || !real(velocity)) return false;
    if ((kind != 1 && kind != 2) || offset >= frames || offset < previous || pitch > 127 || channel > 15 ||
        identity > uint32_t(std::numeric_limits<int32>::max()) || velocity < 0 || velocity > 1) return false;
    previous = offset;
    Event note{};
    note.type = kind == 1 ? Event::kNoteOnEvent : Event::kNoteOffEvent;
    note.sampleOffset = static_cast<int32>(offset);
    if (kind == 1) note.noteOn = {static_cast<int16>(channel), static_cast<int16>(pitch), 0, velocity, 0, static_cast<int32>(identity)};
    else note.noteOff = {static_cast<int16>(channel), static_cast<int16>(pitch), velocity, static_cast<int32>(identity), 0};
    return events.addEvent(note) == kResultOk;
}

bool parameter(Plugin& plugin, ParameterChanges& changes, uint32_t frames, uint32_t& previous) {
    uint32_t identity = 0, offset = 0;
    double value = 0;
    if (!word(identity) || !word(offset) || !normalized(value) || offset >= frames || offset < previous) return false;
    previous = offset;
    const auto controller = plugin.provider->getControllerPtr();
    if (!controller) return false;
    bool found = false;
    const auto count = controller->getParameterCount();
    if (count < 0 || count > 65536) return false;
    for (int32 i = 0; i < count; ++i) {
        ParameterInfo info{};
        if (controller->getParameterInfo(i, info) != kResultOk) return false;
        if (info.id == identity) found = (info.flags & ParameterInfo::kCanAutomate) && !(info.flags & ParameterInfo::kIsReadOnly);
    }
    int32 index = 0;
    auto* queue = found ? changes.addParameterData(identity, index) : nullptr;
    return queue && queue->addPoint(static_cast<int32>(offset), value, index) == kResultOk;
}

bool readBlock(Plugin& plugin, uint32_t frames, ParameterChanges& changes) {
    uint32_t notes = 0, parameters = 0;
    if (!word(notes) || !word(parameters) || notes > 128 || parameters > 128) return false;
    if (notes && plugin.component->getBusCount(kEvent, kInput) == 0) return false;
    plugin.events.clear();
    plugin.outputEvents.clear();
    clearAudio(plugin.data, false);
    uint32_t noteOffset = 0, parameterOffset = 0;
    for (uint32_t i = 0; i < notes; ++i) if (!event(plugin.events, frames, noteOffset)) return false;
    for (uint32_t i = 0; i < parameters; ++i) if (!parameter(plugin, changes, frames, parameterOffset)) return false;
    for (uint32_t frame = 0; frame < frames; ++frame)
        for (int32 bus = 0; bus < plugin.data.numInputs; ++bus)
            for (int32 channel = 0; channel < plugin.data.inputs[bus].numChannels; ++channel)
                if (!real(plugin.data.inputs[bus].channelBuffers32[channel][frame])) return false;
    return true;
}

uint32_t channels(int32 count, AudioBusBuffers* buses) {
    return count == 0 ? 0 : static_cast<uint32_t>(buses[0].numChannels);
}

bool processBlock(Plugin& plugin, uint32_t frames) {
    ParameterChanges changes{128};
    plugin.error = "Malformed, unsupported or truncated stream block";
    if (!readBlock(plugin, frames, changes)) return false;
    plugin.data.numSamples = static_cast<int32>(frames);
    plugin.data.inputParameterChanges = &changes;
    const auto result = plugin.processor->process(plugin.data);
    plugin.data.inputParameterChanges = nullptr;
    double peak = 0;
    plugin.error = "Plug-in rejected a block or produced invalid audio";
    if (result != kResultOk || !measured(plugin.data, peak)) return false;
    written(frames);
    written(plugin.processor->getLatencySamples());
    for (uint32_t frame = 0; frame < frames; ++frame)
        for (int32 channel = 0; channel < plugin.data.outputs[0].numChannels; ++channel)
            written(std::bit_cast<uint32_t>(plugin.data.outputs[0].channelBuffers32[channel][frame]));
    std::cout.flush();
    plugin.error = "Stream output closed";
    return bool(std::cout);
}
}

bool Plugin::stream() {
#ifdef _WIN32
    _setmode(_fileno(stdin), _O_BINARY);
    _setmode(_fileno(stdout), _O_BINARY);
#endif
    error = "Streaming currently requires one output bus and at most one input/event bus";
    if (data.numInputs > 1 || data.numOutputs != 1 || data.outputs[0].numChannels < 1 ||
        component->getBusCount(kEvent, kInput) > 1) return false;
    written(0x3156534f); // OSV1, followed by channel counts and MIDI support.
    written(channels(data.numInputs, data.inputs));
    written(channels(data.numOutputs, data.outputs));
    written(component->getBusCount(kEvent, kInput) ? 1 : 0);
    std::cout.flush();
    while (std::cin.peek() != std::char_traits<char>::eof()) {
        uint32_t frames = 0;
        error = "Invalid or truncated stream frame count";
        if (!word(frames) || frames == 0 || frames > uint32_t(blockSize) ||
            context.projectTimeSamples > std::numeric_limits<int64>::max() - frames) return false;
        if (!processBlock(*this, frames)) return false;
        context.projectTimeSamples += frames;
    }
    error = "Stream input failed";
    return !std::cin.bad();
}
}

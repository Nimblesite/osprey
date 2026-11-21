// [SYNTH-HOST-VST] Shared plug-in lifecycle for probes and persistent worker processing.
#include "processor.h"

namespace osprey::vst {
bool Plugin::open(const std::string& path, const std::string& identifier) {
    module = VST3::Hosting::Module::create(path, error);
    if (!module) return false;
    module->getFactory().setHostContext(host);
    PluginContextFactory::instance().setPluginContext(host);
    for (const auto& info : module->getFactory().classInfos()) {
        if (info.ID().toString() != identifier || info.category() != kVstAudioEffectClass) continue;
        provider = owned(new PlugProvider(module->getFactory(), info, true));
        if (!provider->initialize()) { error = "Plug-in initialization failed"; return false; }
        component = provider->getComponentPtr();
        processor = FUnknownPtr<IAudioProcessor>(component);
        if (!component || !processor) { error = "No audio processor interface"; return false; }
        return synchronizeController() && setup();
    }
    error = "Requested class ID not found";
    return false;
}

bool Plugin::activate(MediaType media, BusDirection direction) {
    const auto count = component->getBusCount(media, direction);
    if (count < 0 || count > 8) return false;
    for (int32 index = 0; index < count; ++index) {
        BusInfo info{};
        if (component->getBusInfo(media, direction, index, info) != kResultOk) return false;
        if (info.channelCount < 0 || info.channelCount > 64) return false;
        if (component->activateBus(media, direction, index, true) != kResultOk) return false;
    }
    return true;
}

bool Plugin::setup() {
    error = "Unsupported buses or float32 processing configuration";
    if (processor->canProcessSampleSize(kSample32) != kResultOk) return false;
    for (auto media : {kAudio, kEvent})
        for (auto direction : {kInput, kOutput})
            if (!activate(media, direction)) return false;
    ProcessSetup setup{kOffline, kSample32, blockSize, sampleRate};
    if (processor->setupProcessing(setup) != kResultOk) return false;
    if (!data.prepare(*component, blockSize, kSample32)) return false;
    if (component->setActive(true) != kResultOk) return false;
    active = true;
    if (processor->setProcessing(true) != kResultOk) return false;
    processing = true;
    context.sampleRate = sampleRate;
    data.processContext = &context;
    data.processMode = kOffline;
    data.numSamples = blockSize;
    data.inputEvents = &events;
    data.outputEvents = &outputEvents;
    return true;
}

bool note(EventList& events, bool down) {
    Event event{};
    event.type = down ? Event::kNoteOnEvent : Event::kNoteOffEvent;
    event.busIndex = 0;
    event.sampleOffset = 0;
    if (down) event.noteOn = {0, 60, 0, 0.8f, 0, 42};
    else event.noteOff = {0, 60, 0, 42, 0};
    return events.addEvent(event) == kResultOk;
}

void clearAudio(HostProcessData& data, bool impulse) {
    for (int32 bus = 0; bus < data.numInputs; ++bus) {
        auto& input = data.inputs[bus];
        input.silenceFlags = 0;
        for (int32 channel = 0; channel < input.numChannels; ++channel) {
            std::fill_n(input.channelBuffers32[channel], blockSize, 0.0f);
            if (impulse) input.channelBuffers32[channel][0] = 0.25f;
        }
    }
    for (int32 bus = 0; bus < data.numOutputs; ++bus) {
        auto& output = data.outputs[bus];
        output.silenceFlags = 0;
        for (int32 channel = 0; channel < output.numChannels; ++channel)
            std::fill_n(output.channelBuffers32[channel], blockSize, 0.0f);
    }
}

bool measured(const HostProcessData& data, double& peak) {
    for (int32 bus = 0; bus < data.numOutputs; ++bus)
        for (int32 channel = 0; channel < data.outputs[bus].numChannels; ++channel)
            for (int32 frame = 0; frame < data.numSamples; ++frame) {
                const auto value = data.outputs[bus].channelBuffers32[channel][frame];
                if (!std::isfinite(value) || std::abs(value) > 1000000) return false;
                peak = std::max(peak, static_cast<double>(std::abs(value)));
            }
    return true;
}

bool saved(IComponent& component, MemoryStream& stream) {
    return component.getState(&stream) == kResultOk && stream.getSize() > 0 &&
           stream.getSize() <= 16 * 1024 * 1024;
}

// Controllers that do not consume processor state report kNotImplemented (e.g. TAL-NoiseMaker).
static bool notifyController(IEditController& controller, MemoryStream& stream) {
    const auto result = controller.setComponentState(&stream);
    return result == kResultOk || result == kNotImplemented;
}

// A separate edit controller must receive the processor's initial state before any UI read.
bool Plugin::synchronizeController() {
    const auto controller = provider->getControllerPtr();
    if (!controller || FUnknownPtr<IEditController>(component).getInterface()) return true;
    MemoryStream stream;
    error = "Initial processor/controller state synchronization failed";
    return saved(*component, stream) &&
           stream.seek(0, IBStream::kIBSeekSet, nullptr) == kResultOk &&
           notifyController(*controller, stream);
}

bool sameState(const MemoryStream& first, const MemoryStream& second) {
    return first.getSize() == second.getSize() &&
           std::memcmp(first.getData(), second.getData(), static_cast<size_t>(first.getSize())) == 0;
}

bool automatable(IEditController& controller, ParameterInfo& info) {
    const auto count = controller.getParameterCount();
    if (count < 0 || count > 65536) return false;
    for (int32 index = 0; index < count; ++index) {
        if (controller.getParameterInfo(index, info) != kResultOk) return false;
        if ((info.flags & ParameterInfo::kCanAutomate) && !(info.flags & ParameterInfo::kIsReadOnly))
            return true;
    }
    return false;
}

bool automated(Plugin& plugin, ParamID identifier, ParamValue value) {
    ParameterChanges changes{1};
    int32 index = 0;
    auto* queue = changes.addParameterData(identifier, index);
    if (!queue || queue->addPoint(17, value, index) != kResultOk) return false;
    clearAudio(plugin.data, false);
    plugin.data.inputParameterChanges = &changes;
    const auto result = plugin.processor->process(plugin.data);
    plugin.data.inputParameterChanges = nullptr;
    return result == kResultOk;
}

bool flushState(Plugin& plugin) {
    plugin.events.clear();
    plugin.data.numSamples = 0;
    const auto result = plugin.processor->process(plugin.data);
    plugin.data.numSamples = blockSize;
    return result == kResultOk;
}

bool restore(Plugin& plugin, IEditController& controller, MemoryStream& stream) {
    if (stream.seek(0, IBStream::kIBSeekSet, nullptr) != kResultOk) return false;
    if (plugin.component->setState(&stream) != kResultOk) return false;
    if (stream.seek(0, IBStream::kIBSeekSet, nullptr) != kResultOk) return false;
    return notifyController(controller, stream) && flushState(plugin);
}

bool Plugin::stateRoundTrip() {
    const auto controller = provider->getControllerPtr();
    ParameterInfo parameter{};
    MemoryStream original, changed, restored;
    error = "No usable controller, automatable parameter or component state";
    if (!controller || !automatable(*controller, parameter) || !saved(*component, original)) return false;
    const auto initial = controller->getParamNormalized(parameter.id);
    const auto edited = initial < 0.5 ? 0.75 : 0.25;
    error = "Controller rejected the parameter value";
    if (controller->setParamNormalized(parameter.id, edited) != kResultOk) return false;
    error = "Processor rejected automation or state capture";
    if (!automated(*this, parameter.id, edited) || !saved(*component, changed)) return false;
    error = "Processor state did not change after automation";
    if (sameState(original, changed)) return false;
    error = "Processor or controller state restoration failed";
    if (!restore(*this, *controller, original)) return false;
    error = "Restored processor state differs from saved state";
    if (!saved(*component, restored) || !sameState(original, restored)) return false;
    error = "Restored controller value differs from initial value: " + std::to_string(initial) + " vs " + std::to_string(controller->getParamNormalized(parameter.id));
    return std::abs(controller->getParamNormalized(parameter.id) - initial) < 0.000001;
}

bool Plugin::run(double& peak) {
    const bool acceptsNotes = component->getBusCount(kEvent, kInput) > 0;
    for (int32 block = 0; block < 200; ++block) {
        events.clear();
        outputEvents.clear();
        if (acceptsNotes && (block == 0 || block == 96) && !note(events, block == 0)) return false;
        clearAudio(data, block == 0);
        context.projectTimeSamples = static_cast<int64>(block) * blockSize;
        if (processor->process(data) != kResultOk || !measured(data, peak)) return false;
    }
    return true;
}
}

int probe(const std::string& path, const std::string& identifier) {
    osprey::vst::Plugin plugin;
    if (!plugin.open(path, identifier)) return failed(plugin.error);
    double peak = 0;
    if (!plugin.run(peak)) return failed("Plug-in processing failed");
    std::cout << "{\"event\":\"vst.probe\",\"frames\":51200,\"peak\":" << peak
              << ",\"latency\":" << plugin.processor->getLatencySamples() << "}\n";
    return peak > 0.000001 ? 0 : failed("The lifecycle probe produced no measurable audio.");
}

int verifyState(const std::string& path, const std::string& identifier) {
    osprey::vst::Plugin plugin;
    if (!plugin.open(path, identifier)) return failed(plugin.error);
    if (!plugin.stateRoundTrip()) return failed(plugin.error);
    std::cout << "{\"event\":\"vst.state-restored\",\"automationOffset\":17}\n";
    return 0;
}

int streamPlugin(const std::string& path, const std::string& identifier) {
    osprey::vst::Plugin plugin;
    if (!plugin.open(path, identifier)) return failed(plugin.error);
    return plugin.stream(std::cin, std::cout) ? 0 : failed(plugin.error);
}

// [SYNTH-HOST-VST] Bounded offline lifecycle probe used before integrating worker streaming.
#include "public.sdk/source/vst/hosting/module.h"
#include "public.sdk/source/vst/hosting/plugprovider.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/processdata.h"
#include "public.sdk/source/vst/hosting/eventlist.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstprocesscontext.h"
#include <algorithm>
#include <cmath>
#include <iostream>

int failed(const std::string& message);

using namespace Steinberg;
using namespace Steinberg::Vst;
namespace {
constexpr int32 blockSize = 256;
constexpr double sampleRate = 48000;

class Plugin {
public:
    VST3::Hosting::Module::Ptr module;
    IPtr<HostApplication> host = owned(new HostApplication);
    IPtr<PlugProvider> provider;
    IPtr<IComponent> component;
    IPtr<IAudioProcessor> processor;
    HostProcessData data;
    EventList events{16};
    EventList outputEvents{128};
    bool active = false;
    bool processing = false;
    std::string error;
    ~Plugin() {
        if (processing) processor->setProcessing(false);
        if (active) component->setActive(false);
        PluginContextFactory::instance().setPluginContext(nullptr);
    }
    bool open(const std::string& path, const std::string& identifier);
    bool setup();
    bool activate(MediaType media, BusDirection direction);
    bool run(double& peak);
};

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
        return setup();
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

bool Plugin::run(double& peak) {
    ProcessContext context{};
    context.sampleRate = sampleRate;
    data.processContext = &context;
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
    Plugin plugin;
    if (!plugin.open(path, identifier)) return failed(plugin.error);
    double peak = 0;
    if (!plugin.run(peak)) return failed("Plug-in processing failed");
    std::cout << "{\"event\":\"vst.probe\",\"frames\":51200,\"peak\":" << peak
              << ",\"latency\":" << plugin.processor->getLatencySamples() << "}\n";
    return peak > 0.000001 ? 0 : failed("The lifecycle probe produced no measurable audio.");
}

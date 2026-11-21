#pragma once
// [SYNTH-HOST-VST] Shared worker lifecycle; never exposed to application code.
#include "public.sdk/source/vst/hosting/module.h"
#include "public.sdk/source/vst/hosting/plugprovider.h"
#include "public.sdk/source/vst/hosting/hostclasses.h"
#include "public.sdk/source/vst/hosting/processdata.h"
#include "public.sdk/source/vst/hosting/eventlist.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include "pluginterfaces/vst/ivstprocesscontext.h"
#include "public.sdk/source/vst/hosting/parameterchanges.h"
#include "public.sdk/source/common/memorystream.h"
#include <cstring>
#include <optional>
#include <unordered_set>
#include <algorithm>
#include <cmath>
#include <iostream>

std::string quoted(const std::string& value);
int failed(const std::string& message);

using namespace Steinberg;
using namespace Steinberg::Vst;
namespace osprey::vst {
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
    ProcessContext context{};
    EventList events{128};
    EventList outputEvents{128};
    bool active = false;
    bool processing = false;
    std::string error;
    bool acceptsParameter(ParamID id);
private:
    std::optional<std::unordered_set<ParamID>> automatableIds;
public:
    ~Plugin() {
        if (processing) processor->setProcessing(false);
        if (active) component->setActive(false);
        PluginContextFactory::instance().setPluginContext(nullptr);
    }
    bool open(const std::string& path, const std::string& identifier);
    bool setup();
    bool activate(MediaType media, BusDirection direction);
    bool run(double& peak);
    bool stateRoundTrip();
    bool synchronizeController();
    bool stream(std::istream& input, std::ostream& output, uint64_t frameLimit = 0);
};

bool measured(const HostProcessData& data, double& peak);
void clearAudio(HostProcessData& data, bool impulse);
}

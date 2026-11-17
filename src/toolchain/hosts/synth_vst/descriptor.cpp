// [SYNTH-HOST-DESCRIPTOR] Inspect real buses and controller parameters before creating graph ports.
#include "processor.h"
#include "public.sdk/source/vst/utility/stringconvert.h"
#include <sstream>
#include <iomanip>

namespace {
using namespace Steinberg;
using namespace Steinberg::Vst;

bool parameters(osprey::vst::Plugin& plugin, std::ostream& out) {
    const auto controller = plugin.provider->getControllerPtr();
    if (!controller) return true;
    const auto count = controller->getParameterCount();
    if (count < 0 || count > 1024) return false;
    for (int32 index = 0; index < count; ++index) {
        ParameterInfo parameter{};
        if (controller->getParameterInfo(index, parameter) != kResultOk) return false;
        const auto initial = controller->getParamNormalized(parameter.id);
        if (!std::isfinite(initial) || initial < 0 || initial > 1) return false;
        if (index) out << ',';
        out << "{\"id\":" << parameter.id << ",\"name\":" << quoted(StringConvert::convert(parameter.title, 128))
            << ",\"unit\":" << quoted(StringConvert::convert(parameter.units, 128))
            << ",\"initial\":" << std::setprecision(17) << initial << ",\"automatable\":"
            << ((parameter.flags & ParameterInfo::kCanAutomate) && !(parameter.flags & ParameterInfo::kIsReadOnly) ? "true" : "false") << '}';
    }
    return true;
}

bool descriptor(osprey::vst::Plugin& plugin, const std::string& identifier, std::ostream& out) {
    if (plugin.data.numInputs > 1 || plugin.data.numOutputs != 1 ||
        plugin.component->getBusCount(kEvent, kInput) > 1) return false;
    for (const auto& info : plugin.module->getFactory().classInfos()) {
        if (info.ID().toString() != identifier) continue;
        out << "{\"id\":" << quoted(identifier) << ",\"name\":" << quoted(info.name())
            << ",\"inputs\":" << (plugin.data.numInputs ? plugin.data.inputs[0].numChannels : 0)
            << ",\"outputs\":" << plugin.data.outputs[0].numChannels << ",\"notes\":"
            << (plugin.component->getBusCount(kEvent, kInput) ? "true" : "false") << ",\"parameters\":[";
        if (!parameters(plugin, out)) return false;
        out << "]}";
        return true;
    }
    return false;
}
}

int describePlugin(const std::string& path, const std::string& identifier) {
    osprey::vst::Plugin plugin;
    if (!plugin.open(path, identifier)) return failed(plugin.error);
    std::ostringstream out;
    if (!descriptor(plugin, identifier, out)) return failed("Unsupported layout or invalid plug-in parameter metadata");
    std::cout << out.str() << '\n';
    return 0;
}

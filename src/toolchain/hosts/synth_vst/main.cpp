// [SYNTH-HOST-VST] Third-party module loading lives in this disposable worker process.
#include "public.sdk/source/vst/hosting/module.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include <iostream>
#include <string>

int probe(const std::string& path, const std::string& identifier);
int verifyState(const std::string& path, const std::string& identifier);

namespace {
// JSON escaping is confined to this process boundary; metadata is untrusted text.
std::string quoted(const std::string& value) {
    constexpr char hex[] = "0123456789abcdef";
    std::string result = "\"";
    for (unsigned char c : value) {
        if (c == '\"' || c == '\\') { result += '\\'; result += static_cast<char>(c); }
        else if (c < 32) { result += "\\u00"; result += hex[c >> 4]; result += hex[c & 15]; }
        else result += static_cast<char>(c);
    }
    return result + '\"';
}

}

int failed(const std::string& message) {
    std::cerr << "{\"event\":\"vst.failure\",\"message\":" << quoted(message) << "}\n";
    return 1;
}

namespace {
int scan(const std::string& path) {
    std::string error;
    auto module = VST3::Hosting::Module::create(path, error);
    if (!module) return failed(error);
    unsigned count = 0;
    for (const auto& info : module->getFactory().classInfos()) {
        if (info.category() != kVstAudioEffectClass) continue;
        std::cout << "{\"event\":\"vst.class\",\"id\":" << quoted(info.ID().toString())
                  << ",\"name\":" << quoted(info.name()) << ",\"categories\":"
                  << quoted(info.subCategoriesString()) << "}\n";
        ++count;
    }
    return count ? 0 : failed("The module contains no VST3 audio components.");
}
}

int main(int argc, char** argv) {
    try {
        if (argc == 4 && std::string(argv[1]) == "verify-state") return verifyState(argv[2], argv[3]);
        if (argc == 4 && std::string(argv[1]) == "probe") return probe(argv[2], argv[3]);
        if (argc == 3 && std::string(argv[1]) == "scan") return scan(argv[2]);
        return failed("Usage: osprey-vst-worker scan <plugin.vst3>");
    } catch (const std::exception& error) {
        return failed(error.what());
    } catch (...) {
        return failed("Native plug-in threw an unknown exception.");
    }
}

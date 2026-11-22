// [SYNTH-HOST-VST] Third-party module loading lives in this disposable worker process.
#include "public.sdk/source/vst/hosting/module.h"
#include "pluginterfaces/vst/ivstaudioprocessor.h"
#include <algorithm>
#include <filesystem>
#include <iostream>
#include <vector>
#include <string>

int audioDevice();
int streamText(const std::string& path, const std::string& identifier, bool session);
int describePlugin(const std::string& path, const std::string& identifier);
int streamPlugin(const std::string& path, const std::string& identifier);
int probe(const std::string& path, const std::string& identifier);
int verifyRestart(const std::string& path, const std::string& identifier);
#ifdef __APPLE__
int editPlugin(const std::string& path, const std::string& identifier);
#endif
int verifyState(const std::string& path, const std::string& identifier);

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
                  << ",\"bundle\":" << quoted(path) << ",\"name\":" << quoted(info.name()) << ",\"categories\":"
                  << quoted(info.subCategoriesString()) << "}\n";
        ++count;
    }
    return count ? 0 : failed("The module contains no VST3 audio components.");
}

int discover(const std::filesystem::path& root) {
    if (!root.is_absolute()) return failed("Choose an absolute VST3 folder.");
    std::vector<std::filesystem::path> bundles;
    for (std::filesystem::recursive_directory_iterator it(root), end; it != end; ++it) {
        if (it->path().extension() != ".vst3") continue;
        it.disable_recursion_pending();
        bundles.push_back(it->path());
    }
    std::sort(bundles.begin(), bundles.end());
    std::cout << "[";
    bool first = true;
    for (const auto& bundle : bundles) {
        std::cout << (first ? "" : ",") << quoted(bundle.string());
        first = false;
    }
    std::cout << "]\n";
    return 0;
}
}

int main(int argc, char** argv) {
    // Transactions flush explicitly. A tied stdin would flush stdout for every
    // hex character, putting thousands of unnecessary locks on each audio block.
    std::ios::sync_with_stdio(false);
    std::cin.tie(nullptr);
    try {
        if (argc == 2 && std::string(argv[1]) == "audio-device") return audioDevice();
        if (argc == 4 && std::string(argv[1]) == "stream-text") return streamText(argv[2], argv[3], false);
        if (argc == 4 && std::string(argv[1]) == "stream-session") return streamText(argv[2], argv[3], true);
        if (argc == 4 && std::string(argv[1]) == "describe") return describePlugin(argv[2], argv[3]);
        if (argc == 4 && std::string(argv[1]) == "stream") return streamPlugin(argv[2], argv[3]);
        if (argc == 4 && std::string(argv[1]) == "verify-restart") return verifyRestart(argv[2], argv[3]);
#ifdef __APPLE__
        if (argc == 4 && std::string(argv[1]) == "editor-session") return editPlugin(argv[2], argv[3]);
#endif
        if (argc == 4 && std::string(argv[1]) == "verify-state") return verifyState(argv[2], argv[3]);
        if (argc == 4 && std::string(argv[1]) == "probe") return probe(argv[2], argv[3]);
        if (argc == 3 && std::string(argv[1]) == "list") return discover(argv[2]);
        if (argc == 3 && std::string(argv[1]) == "scan") return scan(argv[2]);
        return failed("Usage: osprey-vst-worker audio-device | list <folder> | scan <plugin.vst3> | describe|probe|verify-state|verify-restart|stream|stream-text|stream-session <plugin.vst3> <class-id>");
    } catch (const std::exception& error) {
        return failed(error.what());
    } catch (...) {
        return failed("Native plug-in threw an unknown exception.");
    }
}

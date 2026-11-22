// [SYNTH-HOST-STREAM] Bounded text transport for hosts whose capability slices carry UTF-8.
#include "processor.h"
#include "hex_text.h"
#include <sstream>

namespace {
void encoded(const std::string& bytes) {
    constexpr char digits[] = "0123456789ABCDEF";
    std::string hex(bytes.size() * 2, '0');
    for (size_t index = 0; index < bytes.size(); ++index) {
        const auto byte = static_cast<unsigned char>(bytes[index]);
        hex[index * 2] = digits[byte >> 4];
        hex[index * 2 + 1] = digits[byte & 15];
    }
    std::cout.write(hex.data(), static_cast<std::streamsize>(hex.size()));
}

// State frames use a disjoint magic, the selected class ID, and length-prefixed processor
// bytes. A presence byte distinguishes an absent controller state from a valid empty one.
constexpr TSize stateLimit = 1024 * 1024;
bool snapshot(osprey::vst::Plugin& plugin, const std::string& id) {
    MemoryStream component, editor;
    auto controller = plugin.provider->getControllerPtr();
    plugin.error = "Could not capture bounded VST3 state";
    if (id.size() != 32 || plugin.component->getState(&component) != kResultOk) return false;
    const auto status = controller ? controller->getState(&editor) : kNotImplemented;
    if (status != kResultOk && status != kNotImplemented) return false;
    const auto n = component.getSize(), m = editor.getSize();
    if (n < 0 || m < 0 || n > stateLimit - 41 || m > stateLimit - 41 - n) return false;
    std::string bytes = "OPS1" + id;
    for (unsigned i = 0; i < 4; ++i) bytes.push_back(static_cast<char>(n >> (8*i)));
    bytes.push_back(status == kResultOk);
    if (n) bytes.append(component.getData(), n);
    if (m) bytes.append(editor.getData(), m);
    encoded(bytes);
    return bool(std::cout);
}

bool recover(osprey::vst::Plugin& plugin, const std::string& id, std::string& bytes) {
    plugin.error = "Malformed VST3 state or mismatched plug-in class";
    if (bytes.size() < 41 || bytes.compare(4, 32, id) != 0) return false;
    size_t n = 0;
    for (unsigned i = 0; i < 4; ++i) n |= size_t(static_cast<unsigned char>(bytes[36+i])) << (8*i);
    if (n > bytes.size()-41 || (bytes[40] != 0 && bytes[40] != 1)) return false;
    const auto m = bytes.size()-41-n;
    if (!bytes[40] && m) return false;
    MemoryStream component(bytes.data()+41, n), editor(bytes.data()+41+n, m);
    const auto controller = plugin.provider->getControllerPtr();
    plugin.error = "Plug-in rejected the saved VST3 state";
    if (plugin.component->setState(&component) != kResultOk) return false;
    if (controller) {
        if (component.seek(0, IBStream::kIBSeekSet, nullptr) != kResultOk) return false;
        const auto result = controller->setComponentState(&component);
        if (result != kResultOk && result != kNotImplemented) return false;
    }
    if (bytes[40] && (!controller || controller->setState(&editor) != kResultOk)) return false;
    return plugin.flush();
}

bool transaction(osprey::vst::Plugin& plugin, const std::string& bytes) {
    std::istringstream input(bytes);
    std::ostringstream output;
    const auto frameLimit = 65536 / static_cast<uint32_t>(plugin.data.outputs[0].numChannels);
    if (!plugin.stream(input, output, frameLimit)) return false;
    encoded(output.str());
    if (!std::cout) plugin.error = "Text stream output failed";
    return bool(std::cout);
}
}

int streamText(const std::string& path, const std::string& identifier, bool session) {
    osprey::vst::Plugin plugin;
    if (!plugin.open(path, identifier)) return failed(plugin.error);
    if (plugin.data.numOutputs != 1 || plugin.data.outputs[0].numChannels < 1)
        return failed("Text streaming needs one nonempty output bus");
    do {
        if (session && std::cin.peek() == std::char_traits<char>::eof())
            return std::cin.bad() ? failed("Session input failed") : 0;
        std::string bytes;
        if (!readHex(std::cin, bytes, session)) return failed("Malformed or excessive stream hex input");
        if (session && bytes == "SAVE") {
            if (!snapshot(plugin, identifier)) return failed(plugin.error);
        } else if (session && bytes.starts_with("OPS1")) {
            if (!recover(plugin, identifier, bytes)) return failed(plugin.error);
            encoded("OK");
        } else if (!transaction(plugin, bytes)) return failed(plugin.error);
        if (session) std::cout << '\n';
        std::cout.flush();
        if (!std::cout) return failed("Text stream output failed");
    } while (session);
    return 0;
}

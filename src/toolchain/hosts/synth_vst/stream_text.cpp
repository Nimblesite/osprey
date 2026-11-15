// [SYNTH-HOST-STREAM] Bounded text transport for hosts whose capability slices carry UTF-8.
#include "processor.h"
#include "hex_text.h"
#include <sstream>

namespace {
void encoded(const std::string& bytes) {
    constexpr char digits[] = "0123456789ABCDEF";
    for (unsigned char byte : bytes) std::cout << digits[byte >> 4] << digits[byte & 15];
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
        if (!transaction(plugin, bytes)) return failed(plugin.error);
        if (session) std::cout << '\n';
        std::cout.flush();
        if (!std::cout) return failed("Text stream output failed");
    } while (session);
    return 0;
}

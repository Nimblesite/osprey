// [SYNTH-HOST-STREAM] Bounded text transport for hosts whose capability slices carry UTF-8.
#include "processor.h"
#include <sstream>

namespace {
int nibble(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

bool decoded(std::string& bytes, bool line) {
    char first = 0, second = 0;
    while (std::cin.get(first)) {
        if (line && first == '\n') return !bytes.empty();
        if (bytes.size() >= 1024 * 1024 || !std::cin.get(second)) return false;
        const int high = nibble(first), low = nibble(second);
        if (high < 0 || low < 0) return false;
        bytes.push_back(static_cast<char>((high << 4) | low));
    }
    return !line && !std::cin.bad();
}

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
        if (!decoded(bytes, session)) return failed("Malformed or excessive stream hex input");
        if (!transaction(plugin, bytes)) return failed(plugin.error);
        if (session) std::cout << '\n';
        std::cout.flush();
        if (!std::cout) return failed("Text stream output failed");
    } while (session);
    return 0;
}

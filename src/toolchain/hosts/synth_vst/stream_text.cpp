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

bool decoded(std::string& bytes) {
    char first = 0, second = 0;
    while (std::cin.get(first)) {
        if (bytes.size() >= 1024 * 1024 || !std::cin.get(second)) return false;
        const int high = nibble(first), low = nibble(second);
        if (high < 0 || low < 0) return false;
        bytes.push_back(static_cast<char>((high << 4) | low));
    }
    return !std::cin.bad();
}

void encoded(const std::string& bytes) {
    constexpr char digits[] = "0123456789ABCDEF";
    for (unsigned char byte : bytes) std::cout << digits[byte >> 4] << digits[byte & 15];
}
}

int streamText(const std::string& path, const std::string& identifier) {
    std::string bytes;
    if (!decoded(bytes)) return failed("Malformed or excessive stream hex input");
    osprey::vst::Plugin plugin;
    if (!plugin.open(path, identifier)) return failed(plugin.error);
    if (plugin.data.numOutputs != 1 || plugin.data.outputs[0].numChannels < 1)
        return failed("Text streaming needs one nonempty output bus");
    std::istringstream input(bytes);
    std::ostringstream output;
    const auto frameLimit = 65536 / static_cast<uint32_t>(plugin.data.outputs[0].numChannels);
    if (!plugin.stream(input, output, frameLimit)) return failed(plugin.error);
    encoded(output.str());
    return std::cout ? 0 : failed("Text stream output failed");
}

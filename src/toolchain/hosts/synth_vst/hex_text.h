#pragma once
// [SYNTH-HOST-STREAM] Shared strict, bounded hex framing for native text transports.
#include <istream>
#include <string>

inline int hexNibble(char c) {
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

inline bool readHex(std::istream& input, std::string& bytes, bool line) {
    char first = 0, second = 0;
    while (input.get(first)) {
        if (line && first == '\n') return !bytes.empty();
        if (bytes.size() >= 1024 * 1024 || !input.get(second)) return false;
        const int high = hexNibble(first), low = hexNibble(second);
        if (high < 0 || low < 0) return false;
        bytes.push_back(static_cast<char>((high << 4) | low));
    }
    return !line && !input.bad();
}

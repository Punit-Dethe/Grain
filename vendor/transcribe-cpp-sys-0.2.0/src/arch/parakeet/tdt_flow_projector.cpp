#include "tdt_flow_projector.h"

#include <algorithm>

namespace transcribe::parakeet {
namespace {

bool split_valid_utf8(const std::string & bytes, size_t & prefix_len,
                      std::array<char, 3> & carry, size_t & carry_len) {
    size_t i = 0;
    while (i < bytes.size()) {
        const auto b0 = static_cast<unsigned char>(bytes[i]);
        size_t width = 0;
        if (b0 <= 0x7f) {
            width = 1;
        } else if (b0 >= 0xc2 && b0 <= 0xdf) {
            width = 2;
        } else if (b0 >= 0xe0 && b0 <= 0xef) {
            width = 3;
        } else if (b0 >= 0xf0 && b0 <= 0xf4) {
            width = 4;
        } else {
            return false;
        }

        const size_t available = bytes.size() - i;
        const size_t inspect = std::min(width, available);
        for (size_t j = 1; j < inspect; ++j) {
            const auto bx = static_cast<unsigned char>(bytes[i + j]);
            if (bx < 0x80 || bx > 0xbf) {
                return false;
            }
        }
        if (inspect >= 2) {
            const auto b1 = static_cast<unsigned char>(bytes[i + 1]);
            if ((b0 == 0xe0 && b1 < 0xa0) || (b0 == 0xed && b1 > 0x9f) ||
                (b0 == 0xf0 && b1 < 0x90) || (b0 == 0xf4 && b1 > 0x8f)) {
                return false;
            }
        }
        if (available < width) {
            carry_len = available;
            if (carry_len > carry.size()) {
                return false;
            }
            std::copy_n(bytes.data() + i, carry_len, carry.data());
            prefix_len = i;
            return true;
        }
        i += width;
    }
    prefix_len = bytes.size();
    carry_len = 0;
    return true;
}

}  // namespace

bool project_tdt_bytes(const std::string & decoded_bytes, bool final_window,
                       TdtTextProjectorState & state, std::string & delta) {
    std::string bytes;
    bytes.reserve(state.utf8_carry_len + decoded_bytes.size());
    bytes.append(state.utf8_carry.data(), state.utf8_carry_len);
    bytes.append(decoded_bytes);

    std::array<char, 3> next_carry{};
    size_t next_carry_len = 0;
    size_t prefix_len = 0;
    if (!split_valid_utf8(bytes, prefix_len, next_carry, next_carry_len) ||
        (final_window && next_carry_len != 0)) {
        return false;
    }

    auto working = state;
    std::string next_delta;
    next_delta.reserve(prefix_len + 1);
    for (size_t i = 0; i < prefix_len; ++i) {
        const char ch = bytes[i];
        if (ch == ' ') {
            if (working.has_output) {
                working.pending_space = true;
            }
            continue;
        }
        if (working.pending_space) {
            next_delta.push_back(' ');
            working.pending_space = false;
        }
        next_delta.push_back(ch);
        working.has_output = true;
    }

    working.utf8_carry = next_carry;
    working.utf8_carry_len = next_carry_len;
    if (final_window) {
        working.pending_space = false;
    }

    state = working;
    delta = std::move(next_delta);
    return true;
}

}  // namespace transcribe::parakeet

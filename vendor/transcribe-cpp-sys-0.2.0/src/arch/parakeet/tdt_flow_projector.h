#pragma once

#include <array>
#include <cstddef>
#include <string>

namespace transcribe::parakeet {

struct TdtTextProjectorState {
    std::array<char, 3> utf8_carry{};
    size_t              utf8_carry_len = 0;
    bool                pending_space  = false;
    bool                has_output     = false;
};

// Project one call-local decoded byte fragment. State retains at most three
// incomplete UTF-8 bytes plus whitespace flags. On failure, state is unchanged.
bool project_tdt_bytes(const std::string & decoded_bytes, bool final_window,
                       TdtTextProjectorState & state, std::string & delta);

}  // namespace transcribe::parakeet

#pragma once

#include "transcribe.h"

#include <cstdint>

namespace transcribe::parakeet {

transcribe_status validate_tdt_flow_window(
    uint64_t sequence,
    int64_t context_start,
    int64_t fresh_start,
    int64_t commit_end,
    int64_t context_end,
    bool final_window,
    int n_samples,
    int32_t max_window_samples,
    int64_t encoder_stride_samples,
    uint64_t expected_sequence,
    bool has_cursor,
    int64_t committed_sample);

}  // namespace transcribe::parakeet

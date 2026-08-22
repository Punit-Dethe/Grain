#include "tdt_flow_validation.h"

#include <limits>

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
    int64_t committed_sample) {
    if (context_start < 0 || context_start > fresh_start || fresh_start >= commit_end ||
        commit_end > context_end || sequence != expected_sequence ||
        expected_sequence == std::numeric_limits<uint64_t>::max() ||
        (has_cursor && fresh_start != committed_sample) ||
        (final_window && context_end != commit_end)) {
        return TRANSCRIBE_ERR_INVALID_ARG;
    }

    const int64_t window_samples = context_end - context_start;
    if (window_samples <= 0 || window_samples > max_window_samples ||
        window_samples > std::numeric_limits<int>::max() ||
        n_samples != static_cast<int>(window_samples) || encoder_stride_samples <= 0 ||
        context_start % encoder_stride_samples != 0 ||
        fresh_start % encoder_stride_samples != 0 ||
        (!final_window && (commit_end % encoder_stride_samples != 0 ||
                           context_end % encoder_stride_samples != 0))) {
        return TRANSCRIBE_ERR_INVALID_ARG;
    }
    return TRANSCRIBE_OK;
}

}  // namespace transcribe::parakeet

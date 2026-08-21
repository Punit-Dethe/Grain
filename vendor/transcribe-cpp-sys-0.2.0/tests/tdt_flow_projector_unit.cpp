#include "arch/parakeet/decoder.h"
#include "arch/parakeet/tdt_flow_projector.h"
#include "arch/parakeet/tdt_flow_validation.h"
#include "transcribe-meta.h"
#include "transcribe/parakeet.h"
#include "gguf.h"

#include <algorithm>
#include <cassert>
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

using transcribe::parakeet::TdtTextProjectorState;
using transcribe::parakeet::project_tdt_bytes;

static bool same_state(const TdtTextProjectorState & a, const TdtTextProjectorState & b) {
    return a.utf8_carry == b.utf8_carry && a.utf8_carry_len == b.utf8_carry_len &&
           a.pending_space == b.pending_space && a.has_output == b.has_output;
}

int main() {
    {
        transcribe_tdt_flow_params params{};
        transcribe_tdt_flow_params_init(&params);
        assert(params.struct_size == sizeof(params));
        assert(params.max_window_samples == 30 * 16000);
        assert(!params.allow_unadvertised_tdt_head);
    }

    {
        using transcribe::parakeet::validate_tdt_flow_window;
        constexpr int64_t stride = 1280;
        constexpr int32_t max_window = 30 * 16000;
        assert(validate_tdt_flow_window(0, 0, 0, 1280, 2560, false, 2560,
                                        max_window, stride, 0, false, 0) == TRANSCRIBE_OK);
        assert(validate_tdt_flow_window(1, 0, 1280, 1301, 1301, true, 1301,
                                        max_window, stride, 1, true, 1280) == TRANSCRIBE_OK);
        assert(validate_tdt_flow_window(0, 0, 0, 0, 1280, false, 1280,
                                        max_window, stride, 0, false, 0) == TRANSCRIBE_ERR_INVALID_ARG);
        assert(validate_tdt_flow_window(1, 0, 0, 1280, 2560, false, 2560,
                                        max_window, stride, 0, false, 0) == TRANSCRIBE_ERR_INVALID_ARG);
        assert(validate_tdt_flow_window(1, 0, 0, 1280, 2560, false, 2560,
                                        max_window, stride, 1, true, 1280) == TRANSCRIBE_ERR_INVALID_ARG);
        assert(validate_tdt_flow_window(0, 1, 1, 1280, 2560, false, 2559,
                                        max_window, stride, 0, false, 0) == TRANSCRIBE_ERR_INVALID_ARG);
        assert(validate_tdt_flow_window(0, 0, 0, 1281, 2560, false, 2560,
                                        max_window, stride, 0, false, 0) == TRANSCRIBE_ERR_INVALID_ARG);
        assert(validate_tdt_flow_window(0, 0, 0, 1280, 2560, true, 2560,
                                        max_window, stride, 0, false, 0) == TRANSCRIBE_ERR_INVALID_ARG);
        assert(validate_tdt_flow_window(0, 0, 0, 1280, 2560, false, 2559,
                                        max_window, stride, 0, false, 0) == TRANSCRIBE_ERR_INVALID_ARG);
        assert(validate_tdt_flow_window(0, 0, 0, 1280, 2560, false, 2560,
                                        2000, stride, 0, false, 0) == TRANSCRIBE_ERR_INVALID_ARG);
        assert(validate_tdt_flow_window(UINT64_MAX, 0, 0, 1280, 2560, false, 2560,
                                        max_window, stride, UINT64_MAX, false, 0) == TRANSCRIBE_ERR_INVALID_ARG);
    }

    {
        constexpr const char * key = "stt.capability.tdt_flow";
        gguf_context * metadata = gguf_init_empty();
        assert(metadata != nullptr);
        bool enabled = true;
        assert(transcribe::read_optional_bool_kv(metadata, key, "tdt-test", false, enabled) == TRANSCRIBE_OK);
        assert(!enabled);

        gguf_set_val_bool(metadata, key, false);
        enabled = true;
        assert(transcribe::read_optional_bool_kv(metadata, key, "tdt-test", false, enabled) == TRANSCRIBE_OK);
        assert(!enabled);

        gguf_set_val_bool(metadata, key, true);
        enabled = false;
        assert(transcribe::read_optional_bool_kv(metadata, key, "tdt-test", false, enabled) == TRANSCRIBE_OK);
        assert(enabled);
        gguf_free(metadata);

        metadata = gguf_init_empty();
        assert(metadata != nullptr);
        gguf_set_val_str(metadata, key, "true");
        enabled = false;
        assert(transcribe::read_optional_bool_kv(metadata, key, "tdt-test", false, enabled) == TRANSCRIBE_ERR_GGUF);
        assert(!enabled);
        gguf_free(metadata);
    }

    {
        const std::vector<int> durations{3, 8, 4, 1};
        std::vector<int64_t> one_shot;
        int step = 0;
        int symbols = 0;
        for (const int duration : durations) {
            one_shot.push_back(step);
            transcribe::parakeet::advance_tdt_cursor(duration, 5, step, symbols);
        }

        std::vector<int64_t> partitioned;
        size_t decision = 0;
        int carry = 0;
        int64_t offset = 0;
        symbols = 0;
        for (const int slice : {5, 5, 5, 5}) {
            int local_step = carry;
            while (local_step < slice && decision < durations.size()) {
                partitioned.push_back(offset + local_step);
                transcribe::parakeet::advance_tdt_cursor(
                    durations[decision++], 5, local_step, symbols);
            }
            carry = std::max(0, local_step - slice);
            offset += slice;
        }
        assert(decision == durations.size());
        assert(partitioned == one_shot);
        assert(one_shot == std::vector<int64_t>({0, 3, 11, 15}));
    }

    {
        int step = 0;
        int symbols = 0;
        for (int i = 0; i < 4; ++i) {
            transcribe::parakeet::advance_tdt_cursor(0, 5, step, symbols);
            assert(step == 0);
        }
        transcribe::parakeet::advance_tdt_cursor(0, 5, step, symbols);
        assert(step == 1);
        assert(symbols == 0);
    }

    {
        transcribe::parakeet::HostDecoderWeights weights;
        transcribe::parakeet::TdtDecoderState state;
        state.reset(1, 2);
        state.prev_token_id = 4;
        state.duration_skip = 3;
        state.lstm_state.h[0][0] = 7.0f;
        const auto committed = state;
        std::vector<transcribe::parakeet::TdtToken> tokens(1);
        tokens[0].id = 99;
        const auto status = transcribe::parakeet::decode_tdt_greedy_stateful(
            weights, nullptr, 1, 1, state, 0, 1, tokens);
        assert(status == TRANSCRIBE_ERR_INVALID_ARG);
        assert(state.prev_token_id == committed.prev_token_id);
        assert(state.duration_skip == committed.duration_skip);
        assert(state.lstm_state.h == committed.lstm_state.h);
        assert(state.lstm_state.c == committed.lstm_state.c);
        assert(tokens.size() == 1 && tokens[0].id == 99);
    }

    {
        TdtTextProjectorState state;
        std::string delta;
        assert(project_tdt_bytes("\xe2\x82", false, state, delta));
        assert(delta.empty());
        assert(state.utf8_carry_len == 2);
        assert(project_tdt_bytes("\xac", false, state, delta));
        assert(delta == "\xe2\x82\xac");
        assert(state.utf8_carry_len == 0);
    }

    {
        TdtTextProjectorState state;
        std::string delta;
        assert(project_tdt_bytes("  Hello,", false, state, delta));
        assert(delta == "Hello,");
        assert(project_tdt_bytes("   WORLD!  ", false, state, delta));
        assert(delta == " WORLD!");
        assert(state.pending_space);
        assert(project_tdt_bytes("Next?   ", true, state, delta));
        assert(delta == " Next?");
        assert(!state.pending_space);
    }

    for (const std::string invalid : {
             std::string("\x80", 1),
             std::string("\xc0\x80", 2),
             std::string("\xed\xa0\x80", 3),
             std::string("\xf4\x90\x80\x80", 4),
             std::string("\xe2\x28\xa1", 3),
         }) {
        TdtTextProjectorState state;
        state.has_output = true;
        state.pending_space = true;
        const auto committed = state;
        std::string delta = "unchanged";
        assert(!project_tdt_bytes(invalid, false, state, delta));
        assert(same_state(state, committed));
        assert(delta == "unchanged");
    }

    {
        TdtTextProjectorState state;
        std::string delta;
        assert(project_tdt_bytes("\xf0\x9f\x92", false, state, delta));
        const auto committed = state;
        delta = "unchanged";
        assert(!project_tdt_bytes("", true, state, delta));
        assert(same_state(state, committed));
        assert(delta == "unchanged");
    }

    {
        TdtTextProjectorState state;
        std::string delta;
        for (size_t i = 0; i < 100000; ++i) {
            assert(project_tdt_bytes(i % 2 == 0 ? "Word " : "Next ", false, state, delta));
            assert(state.utf8_carry_len <= 3);
            assert(delta.size() <= 5);
        }
        assert(sizeof(state) <= 32);
        assert(project_tdt_bytes("Done.", true, state, delta));
        assert(delta == " Done.");
    }

    return 0;
}

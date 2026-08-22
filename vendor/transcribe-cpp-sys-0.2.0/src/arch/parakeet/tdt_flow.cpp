// arch/parakeet/tdt_flow.cpp - bounded transactional TDT Flow extension.

#include "parakeet.h"
#include "tdt_flow_projector.h"
#include "tdt_flow_validation.h"

#include "transcribe-abi.h"
#include "transcribe-arch.h"
#include "transcribe-log.h"
#include "transcribe-model.h"
#include "transcribe-session.h"
#include "transcribe/parakeet.h"

#include <algorithm>
#include <array>
#include <cctype>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <exception>
#include <limits>
#include <memory>
#include <new>
#include <string>
#include <utility>
#include <vector>

namespace {

constexpr int32_t k_default_max_window_samples = 30 * 16000;
constexpr int32_t k_absolute_max_window_samples = 60 * 16000;
constexpr size_t  k_max_language_bytes = 63;

bool is_lang_tag_piece(const std::string & p) {
    const size_t n = p.size();
    if (n < 7 || p.front() != '<' || p.back() != '>') {
        return false;
    }
    size_t       i     = 1;
    const size_t end   = n - 1;
    const size_t lang0 = i;
    while (i < end && p[i] >= 'a' && p[i] <= 'z') {
        ++i;
    }
    const size_t lang_len = i - lang0;
    if (lang_len < 2 || lang_len > 3 || i >= end || p[i] != '-') {
        return false;
    }
    ++i;
    const size_t reg0 = i;
    while (i < end && std::isalpha(static_cast<unsigned char>(p[i]))) {
        ++i;
    }
    const size_t reg_len = i - reg0;
    return reg_len >= 2 && reg_len <= 4 && i == end;
}

bool is_strippable_special(const transcribe::Tokenizer & tok, int id) {
    return tok.is_control(id) || is_lang_tag_piece(tok.token(id));
}

bool project_tokens(const transcribe::Tokenizer &                      tok,
                    const std::vector<transcribe::parakeet::TdtToken> & tokens,
                    bool                                                keep_special_tags,
                    bool                                                final_window,
                    transcribe::parakeet::TdtTextProjectorState &       state,
                    std::string &                                        delta) {
    std::vector<int> ids;
    ids.reserve(tokens.size());
    for (const auto & token : tokens) {
        if (!keep_special_tags && is_strippable_special(tok, token.id)) {
            continue;
        }
        ids.push_back(token.id);
    }

    const std::string decoded = ids.empty() ? std::string{} :
        tok.decode(ids.data(), static_cast<int>(ids.size()));
    return transcribe::parakeet::project_tdt_bytes(decoded, final_window, state, delta);
}

bool copy_bounded_cstr(const char * src, std::string & out) {
    out.clear();
    if (src == nullptr) {
        return true;
    }
    size_t n = 0;
    while (n <= k_max_language_bytes && src[n] != '\0') {
        ++n;
    }
    if (n > k_max_language_bytes) {
        return false;
    }
    out.assign(src, n);
    return true;
}

bool abort_from_session(void * userdata) {
    auto * session = static_cast<transcribe_session *>(userdata);
    return session != nullptr && session->poll_abort();
}

template <typename Fn> transcribe_status api_guard_status(const char * name, Fn && body) {
    try {
        return body();
    } catch (const std::bad_alloc &) {
        transcribe::log_msg(TRANSCRIBE_LOG_LEVEL_ERROR, "%s: out of memory", name);
        return TRANSCRIBE_ERR_OOM;
    } catch (const std::exception & e) {
        transcribe::log_msg(TRANSCRIBE_LOG_LEVEL_ERROR, "%s: caught exception: %s", name, e.what());
        return TRANSCRIBE_ERR_BACKEND;
    } catch (...) {
        transcribe::log_msg(TRANSCRIBE_LOG_LEVEL_ERROR, "%s: caught unknown exception", name);
        return TRANSCRIBE_ERR_BACKEND;
    }
}

template <typename Fn> void api_guard_void(const char * name, Fn && body) {
    try {
        body();
    } catch (const std::exception & e) {
        transcribe::log_msg(TRANSCRIBE_LOG_LEVEL_WARN, "%s: teardown exception: %s", name, e.what());
    } catch (...) {
        transcribe::log_msg(TRANSCRIBE_LOG_LEVEL_WARN, "%s: unknown teardown exception", name);
    }
}

}  // namespace

struct transcribe_tdt_flow {
    transcribe::parakeet::ParakeetSession * session = nullptr;
    transcribe::parakeet::ParakeetModel *   model   = nullptr;

    transcribe_run_params run_params{};
    std::string           language;
    std::string           target_language;

    transcribe::parakeet::TdtDecoderState       decoder;
    transcribe::parakeet::TdtTextProjectorState projector;
    std::string                           text_delta;

    int32_t  max_window_samples = 0;
    uint64_t expected_sequence  = 0;
    int64_t  committed_sample   = 0;
    bool     has_cursor         = false;
    bool     saw_final_window   = false;
    bool     finished           = false;
    bool     active             = true;
};

extern "C" void transcribe_tdt_flow_params_init(struct transcribe_tdt_flow_params * params) {
    if (params == nullptr) {
        return;
    }
    std::memset(params, 0, sizeof(*params));
    params->struct_size       = sizeof(*params);
    params->max_window_samples = k_default_max_window_samples;
}

extern "C" void transcribe_tdt_flow_window_init(struct transcribe_tdt_flow_window * window) {
    if (window == nullptr) {
        return;
    }
    std::memset(window, 0, sizeof(*window));
    window->struct_size = sizeof(*window);
}

extern "C" void transcribe_tdt_flow_update_init(struct transcribe_tdt_flow_update * update) {
    if (update == nullptr) {
        return;
    }
    std::memset(update, 0, sizeof(*update));
    update->struct_size = sizeof(*update);
}

extern "C" transcribe_status transcribe_tdt_flow_begin(struct transcribe_session *               session,
                                                        const struct transcribe_run_params *      run_params,
                                                        const struct transcribe_tdt_flow_params * params,
                                                        struct transcribe_tdt_flow **             out_flow) {
    return api_guard_status("transcribe_tdt_flow_begin", [&]() -> transcribe_status {
        if (session == nullptr || session->model == nullptr || out_flow == nullptr) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }
        *out_flow = nullptr;

        transcribe_run_params effective_run{};
        transcribe_run_params_init(&effective_run);
        if (run_params != nullptr) {
            if (const auto st = transcribe::check_input_struct_size(run_params->struct_size, sizeof(*run_params));
                st != TRANSCRIBE_OK) {
                return st;
            }
            effective_run = *run_params;
        }
        if (effective_run.task != TRANSCRIBE_TASK_TRANSCRIBE || effective_run.target_language != nullptr ||
            effective_run.family != nullptr || effective_run.pnc != TRANSCRIBE_PNC_MODE_DEFAULT ||
            effective_run.itn != TRANSCRIBE_ITN_MODE_DEFAULT ||
            effective_run.diarize != TRANSCRIBE_DIARIZE_MODE_DEFAULT) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }

        int32_t max_window = k_default_max_window_samples;
        bool    allow_unadvertised_tdt_head = false;
        if (params != nullptr) {
            if (const auto st = transcribe::check_input_struct_size(params->struct_size, sizeof(*params));
                st != TRANSCRIBE_OK) {
                return st;
            }
            max_window                 = params->max_window_samples;
            allow_unadvertised_tdt_head = params->allow_unadvertised_tdt_head;
        }
        if (max_window <= 0 || max_window > k_absolute_max_window_samples) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }
        if (!transcribe::has_feature(session->model, TRANSCRIBE_FEATURE_TDT_FLOW) &&
            !allow_unadvertised_tdt_head) {
            return TRANSCRIBE_ERR_NOT_IMPLEMENTED;
        }
        if (session->model->arch == nullptr || session->model->arch->name == nullptr ||
            std::strcmp(session->model->arch->name, "parakeet") != 0) {
            return TRANSCRIBE_ERR_NOT_IMPLEMENTED;
        }

        auto * parakeet_model = static_cast<transcribe::parakeet::ParakeetModel *>(session->model);
        if (parakeet_model->host_decoder.head_kind != transcribe::parakeet::HostHeadKind::TDT) {
            return TRANSCRIBE_ERR_NOT_IMPLEMENTED;
        }

        auto flow = std::make_unique<transcribe_tdt_flow>();
        flow->session = static_cast<transcribe::parakeet::ParakeetSession *>(session);
        flow->model   = parakeet_model;
        if (!copy_bounded_cstr(effective_run.language, flow->language) ||
            !copy_bounded_cstr(effective_run.target_language, flow->target_language)) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }
        flow->run_params                 = effective_run;
        flow->run_params.language        = flow->language.empty() ? nullptr : flow->language.c_str();
        flow->run_params.target_language = flow->target_language.empty() ? nullptr : flow->target_language.c_str();
        flow->run_params.family          = nullptr;
        flow->max_window_samples         = max_window;
        flow->decoder.reset(static_cast<int>(flow->model->host_decoder.predictor.lstm.size()),
                            flow->model->host_decoder.predictor.pred_hidden);
        *out_flow = flow.release();
        return TRANSCRIBE_OK;
    });
}

extern "C" transcribe_status transcribe_tdt_flow_encoder_stride_samples(
    const struct transcribe_tdt_flow * flow, int32_t * out_stride_samples) {
    return api_guard_status("transcribe_tdt_flow_encoder_stride_samples", [&]() -> transcribe_status {
        if (flow == nullptr || out_stride_samples == nullptr || !flow->active || flow->model == nullptr) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }
        const int64_t stride = static_cast<int64_t>(flow->model->hparams.fe_hop_length) *
                               static_cast<int64_t>(flow->model->hparams.enc_subsampling_factor);
        if (stride <= 0 || stride > std::numeric_limits<int32_t>::max()) {
            return TRANSCRIBE_ERR_BACKEND;
        }
        *out_stride_samples = static_cast<int32_t>(stride);
        return TRANSCRIBE_OK;
    });
}

extern "C" transcribe_status transcribe_tdt_flow_process(struct transcribe_tdt_flow *              flow,
                                                          const float *                             pcm,
                                                          int                                       n_samples,
                                                          const struct transcribe_tdt_flow_window * window,
                                                          struct transcribe_tdt_flow_update *       update) {
    return api_guard_status("transcribe_tdt_flow_process", [&]() -> transcribe_status {
        if (flow == nullptr || pcm == nullptr || window == nullptr || update == nullptr || !flow->active ||
            flow->finished || flow->saw_final_window) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }
        if (const auto st = transcribe::check_input_struct_size(window->struct_size, sizeof(*window));
            st != TRANSCRIBE_OK) {
            return st;
        }
        if (const auto st = transcribe::check_struct_size(update->struct_size, sizeof(*update)); st != TRANSCRIBE_OK) {
            return st;
        }

        const int64_t start  = window->context_start_sample;
        const int64_t fresh  = window->fresh_start_sample;
        const int64_t commit = window->commit_end_sample;
        const int64_t end    = window->context_end_sample;
        const int64_t stride = static_cast<int64_t>(flow->model->hparams.fe_hop_length) *
                               static_cast<int64_t>(flow->model->hparams.enc_subsampling_factor);
        if (const auto st = transcribe::parakeet::validate_tdt_flow_window(
                window->sequence, start, fresh, commit, end, window->final_window, n_samples,
                flow->max_window_samples, stride, flow->expected_sequence, flow->has_cursor,
                flow->committed_sample);
            st != TRANSCRIBE_OK) {
            return st;
        }

        // Encoder/session scratch may change on a failed attempt, but the
        // opaque committed predictor/projector/cursor state below does not.
        flow->session->clear_result();
        flow->session->was_aborted = false;
        const transcribe_status encode_status = transcribe::parakeet::run_one_shot_inner(
            flow->session, flow->model, pcm, n_samples, &flow->run_params, nullptr, /*encode_only=*/true);
        if (encode_status != TRANSCRIBE_OK) {
            return encode_status;
        }

        const int d_enc = flow->model->hparams.enc_d_model;
        if (d_enc <= 0 || flow->session->enc_host.empty() || flow->session->enc_host.size() % d_enc != 0) {
            return TRANSCRIBE_ERR_BACKEND;
        }
        const int T_enc = static_cast<int>(flow->session->enc_host.size() / static_cast<size_t>(d_enc));
        const int64_t owned_start64 = (fresh - start) / stride;
        const int64_t owned_end64   = window->final_window ? (commit - start + stride - 1) / stride :
                                                               (commit - start) / stride;
        if (owned_start64 < 0 || owned_end64 <= owned_start64 || owned_end64 > T_enc ||
            fresh / stride > std::numeric_limits<int>::max()) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }
        const int owned_start = static_cast<int>(owned_start64);
        const int owned_frames = static_cast<int>(owned_end64 - owned_start64);
        const float * owned_enc = flow->session->enc_host.data() +
                                  static_cast<size_t>(owned_start) * static_cast<size_t>(d_enc);

        auto working_decoder   = flow->decoder;
        auto working_projector = flow->projector;
        std::vector<transcribe::parakeet::TdtToken> tokens;
        const transcribe_status decode_status = transcribe::parakeet::decode_tdt_greedy_stateful(
            flow->model->host_decoder, owned_enc, owned_frames, d_enc, working_decoder, fresh / stride,
            flow->session->n_threads, tokens, abort_from_session, flow->session);
        if (decode_status != TRANSCRIBE_OK) {
            return decode_status;
        }

        std::string delta;
        if (!project_tokens(flow->model->tok, tokens, flow->run_params.keep_special_tags, window->final_window,
                            working_projector, delta)) {
            return TRANSCRIBE_ERR_BACKEND;
        }

        // Single commit point for all state visible across descriptors.
        flow->decoder          = std::move(working_decoder);
        flow->projector        = working_projector;
        flow->text_delta       = std::move(delta);
        flow->committed_sample = commit;
        flow->has_cursor       = true;
        flow->saw_final_window = window->final_window;
        ++flow->expected_sequence;

        transcribe_tdt_flow_update staged{};
        staged.struct_size          = sizeof(staged);
        staged.text_delta           = flow->text_delta.data();
        staged.text_delta_bytes     = flow->text_delta.size();
        staged.sequence             = window->sequence;
        staged.committed_end_sample = commit;
        staged.final_window         = window->final_window;
        transcribe::copy_out_prefix(update, &staged, update->struct_size, sizeof(staged));
        return TRANSCRIBE_OK;
    });
}

extern "C" transcribe_status transcribe_tdt_flow_finish(struct transcribe_tdt_flow * flow) {
    return api_guard_status("transcribe_tdt_flow_finish", [&]() -> transcribe_status {
        if (flow == nullptr || !flow->active) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }
        if (flow->finished) {
            return TRANSCRIBE_OK;
        }
        if (!flow->saw_final_window || flow->projector.utf8_carry_len != 0) {
            return TRANSCRIBE_ERR_INVALID_ARG;
        }
        flow->decoder = {};
        flow->text_delta.clear();
        flow->finished = true;
        return TRANSCRIBE_OK;
    });
}

extern "C" void transcribe_tdt_flow_reset(struct transcribe_tdt_flow * flow) {
    api_guard_void("transcribe_tdt_flow_reset", [&]() {
        if (flow == nullptr || !flow->active) {
            return;
        }
        flow->decoder = {};
        flow->projector = {};
        flow->text_delta.clear();
        flow->active = false;
    });
}

extern "C" void transcribe_tdt_flow_free(struct transcribe_tdt_flow * flow) {
    api_guard_void("transcribe_tdt_flow_free", [&]() { delete flow; });
}

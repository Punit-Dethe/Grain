/*
 * include/transcribe/parakeet.h - Parakeet-family public extension surface.
 *
 * Includes transcribe.h; safe to include in C or C++ TUs. Holds the
 * streaming extension structs (cache-aware and chunked-attention
 * variants) and their kind constants and init functions.
 *
 * Acceptance is per-loaded-model-variant: nemotron-speech-streaming-en-0.6b
 * (cache-aware) accepts TRANSCRIBE_EXT_KIND_PARAKEET_STREAM and rejects
 * TRANSCRIBE_EXT_KIND_PARAKEET_BUFFERED_STREAM; parakeet-unified-en-0.6b
 * (chunked_limited_with_rc) does the opposite. Probe via
 * transcribe_model_accepts_ext_kind before pointing
 * transcribe_stream_params::family at one of these structs.
 *
 * FourCC kinds are reserved in docs/extension-kinds.md.
 */

#ifndef TRANSCRIBE_PARAKEET_H
#define TRANSCRIBE_PARAKEET_H

#include "transcribe.h"

#ifdef __cplusplus
extern "C" {
#endif

/* 'PKST' little-endian = 0x54534B50 */
#define TRANSCRIBE_EXT_KIND_PARAKEET_STREAM          0x54534B50u
/* 'PKBS' little-endian = 0x53424B50 */
#define TRANSCRIBE_EXT_KIND_PARAKEET_BUFFERED_STREAM 0x53424B50u

/*
 * Cache-aware streaming knob (nemotron-speech-streaming-en-0.6b).
 *
 *   att_context_right
 *
 *     Right-context (lookahead) selector in encoder frames. The cache-
 *     aware streaming variants are trained on a menu of (left, right)
 *     pairs simultaneously - the user picks one at inference time to
 *     trade latency for accuracy. nemotron's published menu is
 *     right ∈ {13, 6, 1, 0}, corresponding to lookahead of
 *     {1040, 480, 80, 0} ms at the 80ms encoder frame rate.
 *
 *     -1 (default): use the model's default setting (first entry of
 *                   att_context_size_choices = max-accuracy /
 *                   max-latency).
 *     < -1:         caller bug; transcribe_stream_begin returns
 *                   TRANSCRIBE_ERR_INVALID_ARG.
 *     >= 0:         select the corresponding (left, att_context_right)
 *                   entry from the model's training menu. 0 is
 *                   legitimate when 0-frame lookahead is in the menu.
 *                   transcribe_stream_begin returns
 *                   TRANSCRIBE_ERR_INVALID_ARG if the requested right
 *                   is not in the menu.
 *
 *     The published menu and the lookahead (in milliseconds) each
 *     entry corresponds to are documented in the model's family doc
 *     (docs/models/nemotron-speech-streaming-en-0.6b.md); -1 selects
 *     the model's default (max-accuracy / max-latency) entry.
 */
struct transcribe_parakeet_stream_ext {
    struct transcribe_ext ext;
    int32_t               att_context_right;
};

/* Fills ext.size/kind and att_context_right = -1 (model default). */
TRANSCRIBE_API void transcribe_parakeet_stream_ext_init(struct transcribe_parakeet_stream_ext * ext);

/*
 * Chunked-attention (buffered) streaming knob (parakeet-unified-en-0.6b).
 *
 * parakeet-unified-en-0.6b is trained with chunked_limited_with_rc
 * attention over a menu of (left, chunk, right) context tuples
 * expressed in 80ms encoder frames. The user picks the active tuple at
 * stream_begin time; the encoder re-runs over each new
 * [left | chunk | right] PCM window. Each field is in MILLISECONDS;
 * the runtime converts to encoder frames at the model's frame rate.
 *
 * Per-field sentinels (each field independently):
 *
 *   -1     "use the model default for this field." Unified-en-0.6b's
 *          best-accuracy default is L=5600 ms / C=1040 ms / R=1040 ms,
 *          which the published WER numbers correspond to.
 *   < -1   caller bug; transcribe_stream_begin returns
 *          TRANSCRIBE_ERR_INVALID_ARG.
 *   0      a legitimate requested value when 0 frames is in the model's
 *          menu for that field. Not all fields admit 0.
 *   > 0    must be an exact positive multiple of the encoder frame size
 *          (80 ms for every shipped FastConformer streaming variant).
 *          A value that does not divide the frame returns
 *          TRANSCRIBE_ERR_INVALID_ARG; the runtime never silently floors.
 *
 * After per-field resolution the (L, C, R) frame tuple is validated
 * against the model's training menu
 * (stt.parakeet.encoder.att_chunk_{left,chunk,right}_choices); tuples
 * outside the menu return TRANSCRIBE_ERR_INVALID_ARG.
 */
struct transcribe_parakeet_buffered_stream_ext {
    struct transcribe_ext ext;
    int32_t               left_ms;
    int32_t               chunk_ms;
    int32_t               right_ms;
};

/* Fills ext.size/kind and left/chunk/right_ms = -1 (model default). */
TRANSCRIBE_API void transcribe_parakeet_buffered_stream_ext_init(struct transcribe_parakeet_buffered_stream_ext * ext);

/* ----------------------------------------------------------------------- */
/* Bounded TDT Flow                                                        */
/* ----------------------------------------------------------------------- */

/*
 * Opaque per-run state for explicitly capable TDT models. This path is not
 * transcribe_stream_* and does not consult supports_streaming. Probe
 * TRANSCRIBE_FEATURE_TDT_FLOW on the loaded model before begin.
 */
struct transcribe_tdt_flow;

struct transcribe_tdt_flow_params {
    uint64_t struct_size;
    /* Hard cap for every supplied [context_start, context_end) PCM window. */
    int32_t max_window_samples;
    /*
     * Allow a caller-reviewed Parakeet TDT artifact that predates the explicit
     * stt.capability.tdt_flow GGUF key. Non-Parakeet and non-TDT heads remain
     * rejected. False by default so ordinary feature probing stays fail-closed.
     */
    bool allow_unadvertised_tdt_head;
};

struct transcribe_tdt_flow_window {
    uint64_t struct_size;
    uint64_t sequence;
    int64_t  context_start_sample;
    int64_t  fresh_start_sample;
    int64_t  commit_end_sample;
    int64_t  context_end_sample;
    bool     final_window;
};

struct transcribe_tdt_flow_update {
    uint64_t    struct_size;
    const char *text_delta;
    size_t      text_delta_bytes;
    uint64_t    sequence;
    int64_t     committed_end_sample;
    bool        final_window;
};

TRANSCRIBE_API void transcribe_tdt_flow_params_init(struct transcribe_tdt_flow_params * params);
TRANSCRIBE_API void transcribe_tdt_flow_window_init(struct transcribe_tdt_flow_window * window);
TRANSCRIBE_API void transcribe_tdt_flow_update_init(struct transcribe_tdt_flow_update * update);

/*
 * Begin captures run options and allocates bounded predictor/projector state.
 * It rejects models without TRANSCRIBE_FEATURE_TDT_FLOW unless the caller sets
 * allow_unadvertised_tdt_head after independently reviewing the exact artifact.
 * Every path still rejects non-Parakeet/non-TDT heads, unsupported task
 * controls, and invalid maximum windows.
 */
TRANSCRIBE_API transcribe_status transcribe_tdt_flow_begin(
    struct transcribe_session *               session,
    const struct transcribe_run_params *      run_params,
    const struct transcribe_tdt_flow_params * params,
    struct transcribe_tdt_flow **             out_flow);

/* Returns this loaded flow's exact PCM samples per encoder frame. */
TRANSCRIBE_API transcribe_status transcribe_tdt_flow_encoder_stride_samples(
    const struct transcribe_tdt_flow * flow,
    int32_t *                          out_stride_samples);

/*
 * Recompute one bounded context window and decode only:
 *   [context_start_sample, fresh_start_sample) left context
 *   [fresh_start_sample, commit_end_sample)    owned audio
 *   [commit_end_sample, context_end_sample)    right lookahead
 *
 * State and output commit atomically after full success. text_delta is owned
 * by flow, contains only newly committed UTF-8, and remains valid until the
 * next process/reset/free call. PCM and encoder output are never retained.
 */
TRANSCRIBE_API transcribe_status transcribe_tdt_flow_process(
    struct transcribe_tdt_flow *              flow,
    const float *                             pcm,
    int                                       n_samples,
    const struct transcribe_tdt_flow_window * window,
    struct transcribe_tdt_flow_update *       update);

/* Requires a successful process with final_window=true; idempotent. */
TRANSCRIBE_API transcribe_status transcribe_tdt_flow_finish(struct transcribe_tdt_flow * flow);
/* Idempotently abandon state. Safe before free. */
TRANSCRIBE_API void transcribe_tdt_flow_reset(struct transcribe_tdt_flow * flow);
TRANSCRIBE_API void transcribe_tdt_flow_free(struct transcribe_tdt_flow * flow);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* TRANSCRIBE_PARAKEET_H */
